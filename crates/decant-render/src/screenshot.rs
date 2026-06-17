//! Multi-viewport screenshot capture module.

use crate::browser::Browser;
use crate::error::RenderError;
use url::Url;

/// Viewport dimensions configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Viewport {
    /// Name of the viewport preset (e.g. "mobile").
    pub name: &'static str,
    /// Width of viewport in pixels.
    pub width: u32,
    /// Height of viewport in pixels.
    pub height: u32,
}

/// Predefined viewport preset for mobile (iPhone 14 standard).
pub const MOBILE: Viewport = Viewport {
    name: "mobile",
    width: 390,
    height: 844,
};

/// Predefined viewport preset for tablet (iPad standard).
pub const TABLET: Viewport = Viewport {
    name: "tablet",
    width: 768,
    height: 1024,
};

/// Predefined viewport preset for desktop (laptop standard).
pub const DESKTOP: Viewport = Viewport {
    name: "desktop",
    width: 1440,
    height: 900,
};

/// Captured screenshot bytes and metadata.
#[derive(Debug, Clone)]
pub struct Screenshot {
    /// The viewport configuration used.
    pub viewport: Viewport,
    /// Captured PNG image bytes.
    pub png_bytes: Vec<u8>,
}

/// Capture screenshots of a page under multiple viewports.
///
/// # Errors
///
/// Returns a `RenderError` if navigation, resizing, or screenshot capture fails.
pub async fn capture_viewports(
    browser: &Browser,
    url: &Url,
    viewports: &[Viewport],
) -> Result<Vec<Screenshot>, RenderError> {
    let inner_browser = browser.inner();
    let mut screenshots = Vec::new();

    // Open new tab
    let page = inner_browser
        .new_page(url.as_str())
        .await
        .map_err(|e| RenderError::Screenshot {
            url: url.to_string(),
            message: format!("Failed to open new page: {e}"),
        })?;

    // Wait a brief moment for page to load initially
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    for vp in viewports {
        use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;

        page.execute(SetDeviceMetricsOverrideParams::new(
            vp.width as i64,
            vp.height as i64,
            1.0,
            vp.name == "mobile",
        ))
        .await
        .map_err(|e| RenderError::Screenshot {
            url: url.to_string(),
            message: format!("Failed to set viewport to {}x{}: {e}", vp.width, vp.height),
        })?;

        // Wait for reflow
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Capture screenshot
        let png_bytes = page
            .screenshot(
                chromiumoxide::page::ScreenshotParams::builder()
                    .format(
                        chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png,
                    )
                    .full_page(true)
                    .build(),
            )
            .await
            .map_err(|e| RenderError::Screenshot {
                url: url.to_string(),
                message: format!("Failed to capture screenshot for viewport {}: {e}", vp.name),
            })?;

        screenshots.push(Screenshot {
            viewport: *vp,
            png_bytes,
        });
    }

    let _ = page.close().await;

    Ok(screenshots)
}
