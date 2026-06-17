//! `decant-extract` — HTML/CSS parsing, link rewriting, design-token and manifest extraction.
//!
//! This crate is intentionally stateless and has no network I/O.
//! It receives bytes (HTML, CSS) and returns structured data.

pub mod context;
pub mod css;
pub mod error;
pub mod html;
pub mod js;
pub mod manifest;
pub mod tokens;

pub use css::extract_and_rewrite_css;
pub use error::ExtractError;
pub use html::{ExtractedLinks, detect_regions, extract_and_rewrite, extract_title};
pub use js::extract_js_dependencies;
pub use manifest::{Asset, Manifest, PageEntry};
pub use tokens::DesignTokens;
