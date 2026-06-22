//! SPA rendering module using chromiumoxide.

use crate::browser::Browser;
use crate::error::RenderError;
use crate::runtime::{
    ObservedRequest, ObservedResource, ObservedResponse, RenderedPage, RuntimeResourceCollector,
    observed_kind_from_cdp,
};
use chromiumoxide::cdp::browser_protocol::network::{
    self, EventRequestWillBeSent, EventResponseReceived,
};
use decant_core::Cookie;
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use url::Url;

/// Hard deadline for the entire render operation.
const RENDER_TIMEOUT_SECS: u64 = 15;

/// Render the HTML of a page after executing JavaScript.
///
/// # Errors
///
/// Returns a `RenderError` if page loading or evaluation fails.
pub async fn render_html(
    browser: &Browser,
    url: &Url,
    cookies: &[Cookie],
    wait_ms: u64,
) -> Result<String, RenderError> {
    Ok(render_page(browser, url, cookies, wait_ms, false)
        .await?
        .html)
}

/// Render a page after executing JavaScript and return runtime resources.
///
/// # Errors
///
/// Returns a `RenderError` if page loading or evaluation fails.
pub async fn render_page(
    browser: &Browser,
    url: &Url,
    cookies: &[Cookie],
    wait_ms: u64,
    observe_runtime_resources: bool,
) -> Result<RenderedPage, RenderError> {
    tokio::time::timeout(
        Duration::from_secs(RENDER_TIMEOUT_SECS),
        render_page_inner(browser, url, cookies, wait_ms, observe_runtime_resources),
    )
    .await
    .unwrap_or_else(|_| {
        Err(RenderError::Page {
            url: url.to_string(),
            message: format!(
                "Render timed out after {RENDER_TIMEOUT_SECS}s - \
                 falling back to static fetch"
            ),
        })
    })
}

async fn render_page_inner(
    browser: &Browser,
    url: &Url,
    cookies: &[Cookie],
    wait_ms: u64,
    observe_runtime_resources: bool,
) -> Result<RenderedPage, RenderError> {
    let inner_browser = browser.inner();
    let mut diagnostics = Vec::new();

    let page = tokio::time::timeout(
        Duration::from_secs(5),
        inner_browser.new_page("about:blank"),
    )
    .await
    .map_err(|_| RenderError::Page {
        url: url.to_string(),
        message: "Timed out opening new tab".into(),
    })?
    .map_err(|e| RenderError::Page {
        url: url.to_string(),
        message: format!("Failed to open new page: {e}"),
    })?;

    let runtime_observer = if observe_runtime_resources {
        match RuntimeResourceObserver::start(browser, &page, url).await {
            Ok(observer) => Some(observer),
            Err(message) => {
                diagnostics.push(message);
                None
            }
        }
    } else {
        None
    };

    if !cookies.is_empty() {
        use chromiumoxide::cdp::browser_protocol::network::CookieParam;
        use chromiumoxide::cdp::browser_protocol::network::SetCookiesParams;

        let mut cdp_cookies = Vec::new();
        for cookie in cookies {
            let domain = if cookie.domain.is_empty() {
                url.host_str().map(str::to_string)
            } else {
                Some(cookie.domain.clone())
            };

            let param = CookieParam::builder()
                .name(cookie.name.clone())
                .value(cookie.value.clone())
                .url(url.to_string())
                .domain(domain.unwrap_or_default())
                .path("/")
                .secure(url.scheme() == "https")
                .build()
                .map_err(|e| RenderError::Page {
                    url: url.to_string(),
                    message: format!("Failed to build CookieParam: {e}"),
                })?;
            cdp_cookies.push(param);
        }

        page.execute(SetCookiesParams::new(cdp_cookies))
            .await
            .map_err(|e| RenderError::Page {
                url: url.to_string(),
                message: format!("Failed to set cookies: {e}"),
            })?;
    }

    match tokio::time::timeout(Duration::from_secs(5), page.goto(url.as_str())).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            tracing::warn!("Navigation error for {url}: {e}, attempting content extraction");
        }
        Err(_) => {
            tracing::warn!("Navigation timed out for {url}, attempting content extraction");
        }
    }

    let pause = wait_ms.max(500);
    tokio::time::sleep(Duration::from_millis(pause)).await;

    let html = tokio::time::timeout(Duration::from_secs(5), page.content())
        .await
        .map_err(|_| RenderError::Page {
            url: url.to_string(),
            message: "Timed out extracting page content".into(),
        })?
        .map_err(|e| RenderError::Page {
            url: url.to_string(),
            message: format!("Failed to get page content: {e}"),
        })?;

    if runtime_observer.is_some() {
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let _ = page.close().await;

    let mut rendered = RenderedPage::html_only(html);
    rendered.diagnostics = diagnostics;
    if let Some(observer) = runtime_observer {
        rendered.observed_resources = observer.finish();
    }

    Ok(rendered)
}

struct RuntimeResourceObserver {
    request_task: tokio::task::JoinHandle<()>,
    response_task: tokio::task::JoinHandle<()>,
    collector: Arc<Mutex<RuntimeResourceCollector>>,
}

impl RuntimeResourceObserver {
    async fn start(
        browser: &Browser,
        page: &chromiumoxide::Page,
        url: &Url,
    ) -> Result<Self, String> {
        if browser.backend() != crate::backend::BrowserBackend::Chrome {
            return Err("Runtime resource observation is only enabled for Chrome".to_string());
        }

        let request_events = page
            .event_listener::<EventRequestWillBeSent>()
            .await
            .map_err(|e| format!("Failed to listen for network requests: {e}"))?;
        let response_events = page
            .event_listener::<EventResponseReceived>()
            .await
            .map_err(|e| format!("Failed to listen for network responses: {e}"))?;

        page.execute(network::EnableParams::default())
            .await
            .map_err(|e| format!("Failed to enable network observation for {url}: {e}"))?;

        let collector = Arc::new(Mutex::new(RuntimeResourceCollector::default()));
        let request_task = spawn_request_collector(request_events, collector.clone());
        let response_task = spawn_response_collector(response_events, collector.clone());

        Ok(Self {
            request_task,
            response_task,
            collector,
        })
    }

    fn finish(self) -> Vec<ObservedResource> {
        self.request_task.abort();
        self.response_task.abort();
        match Arc::try_unwrap(self.collector) {
            Ok(mutex) => mutex
                .into_inner()
                .map(RuntimeResourceCollector::finish)
                .unwrap_or_default(),
            Err(collector) => collector
                .lock()
                .map(|collector| collector.snapshot())
                .unwrap_or_default(),
        }
    }
}

fn spawn_request_collector(
    mut events: chromiumoxide::listeners::EventStream<EventRequestWillBeSent>,
    collector: Arc<Mutex<RuntimeResourceCollector>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let request = ObservedRequest {
                url: event.request.url.clone(),
                method: event.request.method.clone(),
                kind: event.r#type.as_ref().and_then(observed_kind_from_cdp),
            };
            if let Ok(mut collector) = collector.lock() {
                collector.record_request(event.request_id.as_ref().to_string(), request);
            }
        }
    })
}

fn spawn_response_collector(
    mut events: chromiumoxide::listeners::EventStream<EventResponseReceived>,
    collector: Arc<Mutex<RuntimeResourceCollector>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            let Some(kind) = observed_kind_from_cdp(&event.r#type) else {
                continue;
            };
            let response = ObservedResponse {
                url: event.response.url.clone(),
                kind,
                status: u16::try_from(event.response.status).ok(),
                mime_type: Some(event.response.mime_type.clone()),
            };
            if let Ok(mut collector) = collector.lock() {
                collector.record_response(event.request_id.as_ref(), response);
            }
        }
    })
}
