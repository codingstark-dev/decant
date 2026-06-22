//! File writer — mirrors fetched bytes to the output directory, preserving URL structure.
//!
//! URL path → local file path mapping rules:
//! - `/`                → `index.html`
//! - `/about`           → `about/index.html`  (if no extension)
//! - `/about.html`      → `about.html`
//! - `/assets/app.css`  → `assets/app.css`

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::CoreError;

/// Map a URL to a local file path inside `output_dir`.
pub fn url_to_path(output_dir: &Path, url: &Url) -> PathBuf {
    let url_path = url.path();

    // Strip leading slash and decode percent-encoding.
    let decoded = percent_decode(url_path.trim_start_matches('/'));

    let mut path = output_dir.to_path_buf();

    if decoded.is_empty() || decoded == "/" {
        path.push("index.html");
    } else {
        // Check if the last segment has a file extension.
        let p = Path::new(&decoded);
        if p.extension().is_some() {
            path.push(&decoded);
        } else {
            // Treat as a directory → add index.html.
            path.push(&decoded);
            path.push("index.html");
        }
    }

    append_query_suffix(path, url.query())
}

/// Write bytes to `path`, creating all parent directories as needed.
/// Returns the SHA-256 hex digest of the written content.
pub async fn write_file(path: &Path, bytes: &[u8]) -> Result<String, CoreError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| CoreError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
    }

    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| CoreError::Io {
            path: path.display().to_string(),
            source: e,
        })?;

    file.write_all(bytes).await.map_err(|e| CoreError::Io {
        path: path.display().to_string(),
        source: e,
    })?;

    // Compute SHA-256 for the manifest.
    let digest = hex::encode(Sha256::digest(bytes));
    Ok(format!("sha256:{digest}"))
}

/// Percent-decode a URL path segment (e.g. `%20` → space, `%2F` → `/`).
///
/// Uses the `percent-encoding` crate to decode percent-escape sequences safely.
/// Handles invalid sequences gracefully.
pub fn percent_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

fn append_query_suffix(mut path: PathBuf, query: Option<&str>) -> PathBuf {
    let Some(query) = query.filter(|value| !value.is_empty()) else {
        return path;
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path;
    };

    let digest = hex::encode(Sha256::digest(query.as_bytes()));
    let query_suffix = &digest[..16];
    let file_path = Path::new(file_name);

    let suffixed = match (
        file_path.file_stem().and_then(|stem| stem.to_str()),
        file_path
            .extension()
            .and_then(|extension| extension.to_str()),
    ) {
        (Some(stem), Some(extension)) if !stem.is_empty() => {
            format!("{stem}.q-{query_suffix}.{extension}")
        }
        _ => format!("{file_name}.q-{query_suffix}"),
    };

    path.set_file_name(suffixed);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_maps_to_index_html() {
        let url = Url::parse("https://example.com/").unwrap();
        let path = url_to_path(Path::new("/out"), &url);
        assert_eq!(path, Path::new("/out/index.html"));
    }

    #[test]
    fn path_with_extension_preserved() {
        let url = Url::parse("https://example.com/assets/app.css").unwrap();
        let path = url_to_path(Path::new("/out"), &url);
        assert_eq!(path, Path::new("/out/assets/app.css"));
    }

    #[test]
    fn path_without_extension_gets_index_html() {
        let url = Url::parse("https://example.com/about").unwrap();
        let path = url_to_path(Path::new("/out"), &url);
        assert_eq!(path, Path::new("/out/about/index.html"));
    }

    #[test]
    fn percent_encoded_spaces_decoded() {
        // The url crate already decodes %20 in the path it provides.
        // We still test our percent_decode function directly.
        assert_eq!(percent_decode("my%20page"), "my page");
        assert_eq!(percent_decode("a%2Fb"), "a/b");
        assert_eq!(percent_decode("no-encoding"), "no-encoding");
        assert_eq!(percent_decode("%GG"), "%GG"); // invalid sequence passed through
    }

    #[test]
    fn percent_decode_extreme_cases() {
        // Empty string
        assert_eq!(percent_decode(""), "");
        // Consecutive percent signs and invalid patterns
        assert_eq!(percent_decode("foo%%xyz"), "foo%%xyz");
        assert_eq!(percent_decode("foo%"), "foo%");
        assert_eq!(percent_decode("foo%2"), "foo%2");
        // UTF-8 check: check-mark symbol
        assert_eq!(percent_decode("%E2%9C%93"), "✓");
        // Mixed text
        assert_eq!(
            percent_decode("hello%20world%21%20%E2%9C%93"),
            "hello world! ✓"
        );
    }

    #[test]
    fn query_string_gets_filesystem_safe_suffix() {
        let url = Url::parse("https://example.com/search?q=hello&page=2").unwrap();
        let path = url_to_path(Path::new("/out"), &url);

        assert!(path.starts_with(Path::new("/out/search")));
        assert_ne!(path, Path::new("/out/search/index.html"));
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        assert!(file_name.starts_with("index.q-"));
        assert!(file_name.ends_with(".html"));
        assert!(!file_name.contains('?'));
        assert!(!file_name.contains('&'));
        assert!(!file_name.contains('='));
        assert!(!file_name.contains('#'));
        assert!(!file_name.contains('/'));
        assert!(!file_name.contains(std::path::MAIN_SEPARATOR));
    }

    #[test]
    fn query_distinguished_assets_map_to_distinct_deterministic_paths() {
        let first = Url::parse("https://example.com/assets/app.css?v=1").unwrap();
        let first_again = Url::parse("https://example.com/assets/app.css?v=1#ignored").unwrap();
        let second = Url::parse("https://example.com/assets/app.css?v=2").unwrap();

        let first_path = url_to_path(Path::new("/out"), &first);
        let first_again_path = url_to_path(Path::new("/out"), &first_again);
        let second_path = url_to_path(Path::new("/out"), &second);

        assert_eq!(first_path, first_again_path);
        assert_ne!(first_path, second_path);
        assert_eq!(first_path.parent(), Some(Path::new("/out/assets")));
        assert_eq!(second_path.parent(), Some(Path::new("/out/assets")));
        assert!(
            first_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(
                    |file_name| file_name.starts_with("app.q-") && file_name.ends_with(".css")
                )
        );
        assert!(
            second_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(
                    |file_name| file_name.starts_with("app.q-") && file_name.ends_with(".css")
                )
        );
    }

    #[test]
    fn deeply_nested_asset_path() {
        let url = Url::parse("https://example.com/a/b/c/d.js").unwrap();
        let path = url_to_path(Path::new("/out"), &url);
        assert_eq!(path, Path::new("/out/a/b/c/d.js"));
    }
}
