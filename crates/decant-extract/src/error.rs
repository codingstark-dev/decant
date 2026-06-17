//! Error types for `decant-extract`.

use thiserror::Error;

/// All errors that can arise during extraction.
#[derive(Debug, Error)]
pub enum ExtractError {
    /// HTML parsing produced an unexpected structure.
    #[error("HTML parse error: {0}")]
    HtmlParse(String),

    /// CSS parsing failed.
    #[error("CSS parse error: {0}")]
    CssParse(String),

    /// A URL reference inside HTML/CSS could not be resolved.
    #[error("URL resolution error for `{href}` relative to `{base}`: {source}")]
    UrlResolve {
        /// The href attribute value that could not be resolved.
        href: String,
        /// The base URL against which resolution was attempted.
        base: String,
        /// The underlying URL parse error.
        #[source]
        source: url::ParseError,
    },

    /// JSON serialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// I/O error (e.g. writing context.md).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
