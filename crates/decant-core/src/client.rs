//! HTTP client — a thin wrapper around `reqwest` configured for decant:
//! - HTTP/2 preferred, rustls TLS backend (no OpenSSL dependency).
//! - Gzip + Brotli decompression enabled.
//! - Configurable `User-Agent` that always identifies the tool.
//! - 30-second connect / 60-second read timeouts.

use reqwest::{Client, Response};
use url::Url;

use crate::CoreError;

/// Default user-agent string.  Override with `--user-agent` CLI flag.
pub const DEFAULT_USER_AGENT: &str = concat!(
    "decant/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/himanshum/decant)"
);

/// Build the shared `reqwest::Client`.  Call once at startup.
///
/// # Errors
///
/// Returns a `CoreError` if client building or header parsing fails.
pub fn build_client(
    user_agent: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<Client, CoreError> {
    let ua = user_agent.unwrap_or(DEFAULT_USER_AGENT);
    let mut builder = Client::builder()
        .user_agent(ua)
        .use_rustls_tls()
        // HTTP/2 is negotiated automatically via ALPN with the rustls backend;
        // no explicit opt-in is needed on reqwest 0.12.
        .connect_timeout(std::time::Duration::from_secs(30))
        .timeout(std::time::Duration::from_secs(60))
        .gzip(true)
        .brotli(true);

    let mut headers = reqwest::header::HeaderMap::new();
    for (k, v) in extra_headers {
        let name = reqwest::header::HeaderName::from_bytes(k.as_bytes()).map_err(|_| {
            CoreError::InvalidHeader {
                name: k.clone(),
                reason: "invalid header name".to_string(),
            }
        })?;
        let value = reqwest::header::HeaderValue::from_bytes(v.as_bytes()).map_err(|_| {
            CoreError::InvalidHeader {
                name: k.clone(),
                reason: "invalid header value".to_string(),
            }
        })?;
        headers.insert(name, value);
    }
    builder = builder.default_headers(headers);

    builder.build().map_err(|e| CoreError::Http {
        url: "<client-init>".to_string(),
        source: e,
    })
}

/// Fetch a URL, returning the response. Rate limiting must be applied by the caller.
///
/// # Errors
///
/// Returns a `CoreError` if the HTTP request fails or the server returns a non-2xx status.
pub async fn fetch(client: &Client, url: &Url) -> Result<Response, CoreError> {
    tracing::debug!("GET {url}");
    let resp = client
        .get(url.as_str())
        .send()
        .await
        .map_err(|e| CoreError::Http {
            url: url.to_string(),
            source: e,
        })?;
    if !resp.status().is_success() {
        return Err(CoreError::Http {
            url: url.to_string(),
            source: resp.error_for_status().unwrap_err(),
        });
    }
    Ok(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_client_defaults() {
        let client = build_client(None, &[]);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_custom_ua() {
        let client = build_client(Some("my-custom-ua"), &[]);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_valid_headers() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer token123".to_string()),
            ("X-Custom-Header".to_string(), "CustomValue".to_string()),
        ];
        let client = build_client(None, &headers);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_client_invalid_header_name() {
        let headers = vec![
            ("Invalid Header Name".to_string(), "Value".to_string()), // spaces are invalid in HTTP header names
        ];
        let client = build_client(None, &headers);
        assert!(client.is_err());
        if let Err(CoreError::InvalidHeader { name, .. }) = client {
            assert_eq!(name, "Invalid Header Name");
        } else {
            panic!("Expected InvalidHeader error");
        }
    }

    #[test]
    fn test_build_client_invalid_header_value() {
        let headers = vec![
            ("X-Header".to_string(), "Value\nWithNewline".to_string()), // newlines are invalid in header values
        ];
        let client = build_client(None, &headers);
        assert!(client.is_err());
        if let Err(CoreError::InvalidHeader { name, .. }) = client {
            assert_eq!(name, "X-Header");
        } else {
            panic!("Expected InvalidHeader error");
        }
    }
}
