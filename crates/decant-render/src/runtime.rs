//! Runtime resource metadata.
//!
//! # noqa: SIZE_OK - the collector types and CDP ordering logic are intentionally
//! colocated so late response/request reconciliation stays testable as one unit.

use std::collections::HashMap;

use chromiumoxide::cdp::browser_protocol::network::ResourceType;
use url::Url;

/// A rendered page plus resources observed by the browser at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    /// The browser-rendered HTML.
    pub html: String,
    /// Static-like resources Chrome observed while rendering.
    pub observed_resources: Vec<ObservedResource>,
    /// Non-fatal notes about runtime collection.
    pub diagnostics: Vec<String>,
}

impl RenderedPage {
    /// Build an HTML-only rendered page.
    #[must_use]
    pub fn html_only(html: String) -> Self {
        Self {
            html,
            observed_resources: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

/// Browser-observed resource metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedResource {
    /// Absolute HTTP(S) URL seen by the browser.
    pub url: String,
    /// HTTP method seen for the request.
    pub method: String,
    /// Stable browser resource category.
    pub kind: ObservedResourceKind,
    /// HTTP response status, if Chrome emitted a response event.
    pub status: Option<u16>,
    /// Browser-reported response MIME type, if available.
    pub mime_type: Option<String>,
}

/// Stable browser resource category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservedResourceKind {
    /// Document.
    Document,
    /// Stylesheet.
    Stylesheet,
    /// Image.
    Image,
    /// Audio/video.
    Media,
    /// Web font.
    Font,
    /// Script.
    Script,
    /// Text track.
    TextTrack,
    /// Web app manifest.
    Manifest,
    /// Prefetch.
    Prefetch,
    /// Other static-like URL accepted by extension or MIME policy.
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct ObservedRequest {
    pub(crate) url: String,
    pub(crate) method: String,
    pub(crate) kind: Option<ObservedResourceKind>,
}

#[derive(Debug, Clone)]
pub(crate) struct ObservedResponse {
    pub(crate) url: String,
    pub(crate) kind: ObservedResourceKind,
    pub(crate) status: Option<u16>,
    pub(crate) mime_type: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeResourceCollector {
    requests: HashMap<String, ObservedRequest>,
    responses: HashMap<String, ObservedResponse>,
    resources: HashMap<String, ObservedResource>,
}

pub(crate) fn observed_kind_from_cdp(kind: &ResourceType) -> Option<ObservedResourceKind> {
    match kind {
        ResourceType::Document => Some(ObservedResourceKind::Document),
        ResourceType::Stylesheet => Some(ObservedResourceKind::Stylesheet),
        ResourceType::Image => Some(ObservedResourceKind::Image),
        ResourceType::Media => Some(ObservedResourceKind::Media),
        ResourceType::Font => Some(ObservedResourceKind::Font),
        ResourceType::Script => Some(ObservedResourceKind::Script),
        ResourceType::TextTrack => Some(ObservedResourceKind::TextTrack),
        ResourceType::Prefetch => Some(ObservedResourceKind::Prefetch),
        ResourceType::Manifest => Some(ObservedResourceKind::Manifest),
        ResourceType::Other => Some(ObservedResourceKind::Other),
        ResourceType::Xhr
        | ResourceType::Fetch
        | ResourceType::EventSource
        | ResourceType::WebSocket
        | ResourceType::SignedExchange
        | ResourceType::Ping
        | ResourceType::CspViolationReport
        | ResourceType::Preflight => None,
    }
}

impl RuntimeResourceCollector {
    pub(crate) fn record_request(&mut self, request_id: String, request: ObservedRequest) {
        if accepts_request(&request) {
            if let Some(response) = self.responses.remove(&request_id) {
                insert_resource(&mut self.resources, &request, response);
            }
            self.requests.insert(request_id, request);
        }
    }

    pub(crate) fn record_response(&mut self, request_id: &str, response: ObservedResponse) {
        if !accepts_response(&response) {
            return;
        }
        let Some(request) = self.requests.get(request_id) else {
            self.responses.insert(request_id.to_string(), response);
            return;
        };
        if !accepts_request(request) {
            return;
        }
        insert_resource(&mut self.resources, request, response);
    }

    pub(crate) fn finish(self) -> Vec<ObservedResource> {
        sort_resources(self.resources.into_values().collect())
    }

    pub(crate) fn snapshot(&self) -> Vec<ObservedResource> {
        sort_resources(self.resources.values().cloned().collect())
    }
}

fn sort_resources(mut resources: Vec<ObservedResource>) -> Vec<ObservedResource> {
    resources.sort_by(|left, right| left.url.cmp(&right.url));
    resources
}

fn insert_resource(
    resources: &mut HashMap<String, ObservedResource>,
    request: &ObservedRequest,
    response: ObservedResponse,
) {
    resources
        .entry(response.url.clone())
        .and_modify(|resource| {
            resource.kind = response.kind;
            resource.status = response.status;
            resource.mime_type.clone_from(&response.mime_type);
        })
        .or_insert_with(|| ObservedResource {
            url: response.url,
            method: request.method.clone(),
            kind: response.kind,
            status: response.status,
            mime_type: response.mime_type,
        });
}

const fn accepts_kind(kind: ObservedResourceKind) -> bool {
    match kind {
        ObservedResourceKind::Document
        | ObservedResourceKind::Stylesheet
        | ObservedResourceKind::Image
        | ObservedResourceKind::Media
        | ObservedResourceKind::Font
        | ObservedResourceKind::Script
        | ObservedResourceKind::TextTrack
        | ObservedResourceKind::Manifest
        | ObservedResourceKind::Prefetch
        | ObservedResourceKind::Other => true,
    }
}

fn accepts_request(request: &ObservedRequest) -> bool {
    request.method.eq_ignore_ascii_case("GET")
        && is_http_url(&request.url)
        && request
            .kind
            .map_or_else(|| accepts_static_extension(&request.url), accepts_kind)
}

fn accepts_response(response: &ObservedResponse) -> bool {
    response
        .status
        .is_some_and(|status| (200..300).contains(&status))
}

fn is_http_url(raw_url: &str) -> bool {
    Url::parse(raw_url).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn classify_url(raw_url: &str) -> ObservedResourceKind {
    let Ok(url) = Url::parse(raw_url) else {
        return ObservedResourceKind::Other;
    };
    let path = url.path().to_ascii_lowercase();
    if matches!(extension(&path), Some("js" | "mjs" | "cjs")) {
        return ObservedResourceKind::Script;
    }
    if matches!(extension(&path), Some("css")) {
        return ObservedResourceKind::Stylesheet;
    }
    if matches!(
        extension(&path),
        Some("avif" | "gif" | "ico" | "jpeg" | "jpg" | "png" | "svg" | "webp")
    ) {
        return ObservedResourceKind::Image;
    }
    if matches!(
        extension(&path),
        Some("woff" | "woff2" | "ttf" | "otf" | "eot")
    ) {
        return ObservedResourceKind::Font;
    }
    if matches!(
        extension(&path),
        Some("mp3" | "mp4" | "ogg" | "webm" | "wav")
    ) {
        return ObservedResourceKind::Media;
    }
    if matches!(extension(&path), Some("webmanifest" | "manifest")) {
        return ObservedResourceKind::Manifest;
    }
    ObservedResourceKind::Other
}

fn accepts_static_extension(raw_url: &str) -> bool {
    !matches!(classify_url(raw_url), ObservedResourceKind::Other)
}

fn extension(path: &str) -> Option<&str> {
    path.rsplit_once('.').map(|(_, extension)| extension)
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
