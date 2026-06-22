//! `decant-render` — headless-Chrome SPA rendering (feature-gated).
//!
//! When compiled **without** the `render` feature this crate is an empty stub —
//! it exports a `RenderError` type and a placeholder `not_available()` function
//! so that the main `decant` crate can reference it unconditionally and gate at runtime.
//!
//! Enable the `render` Cargo feature to pull in `chromiumoxide` and the real impl:
//! ```sh
//! cargo build --features render
//! ```

pub mod error;

#[cfg(feature = "render")]
pub mod backend;
#[cfg(feature = "render")]
pub mod browser;
#[cfg(feature = "render")]
pub mod page;
#[cfg(feature = "render")]
pub mod runtime;
#[cfg(feature = "render")]
pub mod screenshot;

pub use error::RenderError;

#[cfg(feature = "render")]
pub use backend::BrowserBackend;
#[cfg(feature = "render")]
pub use browser::Browser;
#[cfg(feature = "render")]
pub use page::{render_html, render_page};
#[cfg(feature = "render")]
pub use runtime::{ObservedResource, ObservedResourceKind, RenderedPage};
#[cfg(feature = "render")]
pub use screenshot::{DESKTOP, MOBILE, Screenshot, TABLET, Viewport, capture_viewports};

/// Returns `Err` with a helpful message when the crate was not compiled with `--features render`.
///
/// # Errors
///
/// Returns `RenderError::FeatureNotEnabled`.
#[cfg(not(feature = "render"))]
pub fn not_available() -> Result<(), RenderError> {
    Err(RenderError::FeatureNotEnabled)
}

#[cfg(not(feature = "render"))]
/// Stub module for browser backends.
pub mod backend {
    /// Stub type for BrowserBackend when render feature is disabled.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub enum BrowserBackend {
        /// Chrome
        Chrome,
        /// Lightpanda,
        Lightpanda,
    }
}

#[cfg(not(feature = "render"))]
/// Stub module for browser instance.
pub mod browser {
    /// Stub type for Browser when render feature is disabled.
    pub struct Browser;
}

#[cfg(not(feature = "render"))]
/// Stub module for viewport screenshots.
pub mod screenshot {
    /// Stub type for Viewport when render feature is disabled.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct Viewport {
        /// Name
        pub name: &'static str,
        /// Width
        pub width: u32,
        /// Height
        pub height: u32,
    }
    /// MOBILE preset stub
    pub const MOBILE: Viewport = Viewport {
        name: "mobile",
        width: 390,
        height: 844,
    };
    /// TABLET preset stub
    pub const TABLET: Viewport = Viewport {
        name: "tablet",
        width: 768,
        height: 1024,
    };
    /// DESKTOP preset stub
    pub const DESKTOP: Viewport = Viewport {
        name: "desktop",
        width: 1440,
        height: 900,
    };
}

#[cfg(not(feature = "render"))]
pub use backend::BrowserBackend;
#[cfg(not(feature = "render"))]
pub use browser::Browser;
#[cfg(not(feature = "render"))]
pub use screenshot::{DESKTOP, MOBILE, TABLET, Viewport};
