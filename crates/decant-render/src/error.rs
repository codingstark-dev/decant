//! Error types for `decant-render`.

use thiserror::Error;

/// Errors that can arise from the headless-Chrome renderer.
#[derive(Debug, Error)]
pub enum RenderError {
    /// The crate was compiled without the `render` feature.
    #[error(
        "`decant` was built without the `render` feature. \
         Reinstall with: cargo install decant --features render"
    )]
    FeatureNotEnabled,

    /// chromiumoxide / browser launch failed.
    #[cfg(feature = "render")]
    #[error("browser launch failed: {0}")]
    BrowserLaunch(String),

    /// Lightpanda binary not found in PATH or environment.
    #[cfg(feature = "render")]
    #[error("lightpanda binary not found: {0}")]
    LightpandaNotFound(String),

    /// Chrome binary not found in PATH or environment.
    #[cfg(feature = "render")]
    #[error("chrome binary not found: {0}")]
    ChromeNotFound(String),

    /// CDP connection failed.
    #[cfg(feature = "render")]
    #[error("CDP connection failed: {0}")]
    CdpConnection(String),

    /// Page navigation or evaluation failed.
    #[cfg(feature = "render")]
    #[error("page error for `{url}`: {message}")]
    Page {
        /// The URL that triggered the page error.
        url: String,
        /// The error message.
        message: String,
    },

    /// Screenshot capture failed.
    #[cfg(feature = "render")]
    #[error("screenshot failed for `{url}`: {message}")]
    Screenshot {
        /// The URL that triggered the screenshot error.
        url: String,
        /// The error message.
        message: String,
    },
}
