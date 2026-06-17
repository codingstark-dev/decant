//! Error types for `decant-core`.

use thiserror::Error;

/// All errors that can arise in the fetch engine.
#[derive(Debug, Error)]
pub enum CoreError {
    /// An HTTP-level error from `reqwest`.
    #[error("HTTP error fetching {url}: {source}")]
    Http {
        /// The URL that triggered the HTTP error.
        url: String,
        /// The underlying `reqwest` error.
        #[source]
        source: reqwest::Error,
    },

    /// The URL could not be parsed or normalized.
    #[error("invalid URL `{0}`: {1}")]
    InvalidUrl(String, #[source] url::ParseError),

    /// I/O error writing to disk.
    #[error("I/O error writing `{path}`: {source}")]
    Io {
        /// The file path that could not be written.
        path: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// robots.txt disallows this URL.
    #[error("robots.txt disallows `{0}`")]
    RobotsDisallowed(String),

    /// Rate limiter error (should not normally surface to the user).
    #[error("rate limiter error: {0}")]
    RateLimit(String),

    /// The crawl was cancelled (e.g. user pressed Ctrl-C).
    #[error("crawl cancelled")]
    Cancelled,

    /// An invalid HTTP header name or value was provided.
    #[error("invalid HTTP header `{name}`: {reason}")]
    InvalidHeader {
        /// The invalid header name.
        name: String,
        /// Why it is invalid.
        reason: String,
    },
}
