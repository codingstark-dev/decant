//! SPA rendering module using chromiumoxide.

use crate::browser::Browser;
use crate::error::RenderError;
use decant_core::Cookie;
use std::time::Duration;
use url::Url;

/// Hard deadline for the entire render operation (new_page + goto + content).
/// Lightpanda is known to hang indefinitely on some sites/platforms; this
/// ensures we always fall back to static fetch within a bounded time.
const RENDER_TIMEOUT_SECS: u64 = 15;

/// Render the HTML of a page after executing JavaScript.
///
/// The full operation (tab open → navigate → extract) is bounded by
/// [`RENDER_TIMEOUT_SECS`].  Any individual step that does not complete in
/// time is treated as a warning and we continue to extract whatever DOM the
/// browser has so far.
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
    tokio::time::timeout(
        Duration::from_secs(RENDER_TIMEOUT_SECS),
        render_html_inner(browser, url, cookies, wait_ms),
    )
    .await
    .unwrap_or_else(|_| {
        Err(RenderError::Page {
            url: url.to_string(),
            message: format!(
                "Render timed out after {RENDER_TIMEOUT_SECS}s — \
                 falling back to static fetch"
            ),
        })
    })
}

async fn render_html_inner(
    browser: &Browser,
    url: &Url,
    cookies: &[Cookie],
    wait_ms: u64,
) -> Result<String, RenderError> {
    let inner_browser = browser.inner();

    // Open a new page (tab) — can hang on some backends, so give it 5 s.
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

    // Inject cookies if any
    if !cookies.is_empty() {
        use chromiumoxide::cdp::browser_protocol::network::CookieParam;
        use chromiumoxide::cdp::browser_protocol::network::SetCookiesParams;

        let mut cdp_cookies = Vec::new();
        for c in cookies {
            let domain = if c.domain.is_empty() {
                url.host_str().map(|h| h.to_string())
            } else {
                Some(c.domain.clone())
            };

            let param = CookieParam::builder()
                .name(c.name.clone())
                .value(c.value.clone())
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

    // Navigate to the target URL — 5 s timeout; proceed even on timeout so we
    // still get whatever the browser managed to load.
    match tokio::time::timeout(Duration::from_secs(5), page.goto(url.as_str())).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            tracing::warn!("Navigation error for {url}: {e}, attempting content extraction");
        }
        Err(_) => {
            tracing::warn!("Navigation timed out for {url}, attempting content extraction");
        }
    }

    // Brief wait so JS can run.
    let pause = wait_ms.max(500);
    tokio::time::sleep(Duration::from_millis(pause)).await;

    // Extract the rendered DOM — 5 s timeout.
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

    let _ = page.close().await;

    Ok(html)
}
