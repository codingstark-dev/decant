//! Browser backend configuration and discovery.

use std::path::PathBuf;

/// The rendering backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BrowserBackend {
    /// Headless Google Chrome or Chromium.
    Chrome,
    /// Lightpanda, a lightweight, ultra-fast CDP-compatible browser.
    Lightpanda,
}

impl BrowserBackend {
    /// Returns whether the backend supports taking visual screenshots.
    pub fn supports_screenshots(&self) -> bool {
        match self {
            Self::Chrome => true,
            Self::Lightpanda => false,
        }
    }

    /// Resolve the path to the Lightpanda binary.
    ///
    /// Reads `LIGHTPANDA_BIN` env var first, falling back to searching the system PATH.
    pub fn resolve_lightpanda() -> Option<PathBuf> {
        resolve_lightpanda_internal(std::env::var("LIGHTPANDA_BIN").ok(), find_in_path, |p| {
            p.is_file()
        })
    }

    /// Resolve the path to the Chrome binary.
    ///
    /// Reads `CHROME_PATH` env var first, falling back to searching the system PATH
    /// for common Chrome/Chromium executable names.
    pub fn resolve_chrome() -> Option<PathBuf> {
        resolve_chrome_internal(std::env::var("CHROME_PATH").ok(), find_in_path, |p| {
            p.is_file()
        })
    }
}

fn resolve_lightpanda_internal(
    env_val: Option<String>,
    find_in_path_fn: impl Fn(&str) -> Option<PathBuf>,
    is_file_fn: impl Fn(&std::path::Path) -> bool,
) -> Option<PathBuf> {
    if let Some(val) = env_val {
        let path = PathBuf::from(val);
        if is_file_fn(&path) {
            return Some(path);
        }
    }
    find_in_path_fn("lightpanda")
}

fn resolve_chrome_internal(
    env_val: Option<String>,
    find_in_path_fn: impl Fn(&str) -> Option<PathBuf>,
    is_file_fn: impl Fn(&std::path::Path) -> bool,
) -> Option<PathBuf> {
    if let Some(val) = env_val {
        let path = PathBuf::from(val);
        if is_file_fn(&path) {
            return Some(path);
        }
    }
    for name in &[
        "google-chrome",
        "chromium",
        "chrome",
        "Google Chrome",
        "Chromium",
    ] {
        if let Some(path) = find_in_path_fn(name) {
            return Some(path);
        }
    }

    // On macOS, Chrome is often at a specific standard path
    #[cfg(target_os = "macos")]
    {
        let macos_paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];
        for path_str in &macos_paths {
            let path = PathBuf::from(path_str);
            if is_file_fn(&path) {
                return Some(path);
            }
        }
    }

    None
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_screenshots() {
        assert!(BrowserBackend::Chrome.supports_screenshots());
        assert!(!BrowserBackend::Lightpanda.supports_screenshots());
    }

    #[test]
    fn test_resolve_lightpanda_internal() {
        // 1. Env variable set and points to an existing file
        let resolved = resolve_lightpanda_internal(
            Some("/path/to/my-lightpanda".to_string()),
            |_| None,
            |_| true,
        );
        assert_eq!(resolved, Some(PathBuf::from("/path/to/my-lightpanda")));

        // 2. Env variable set but file does not exist, falls back to path
        let resolved = resolve_lightpanda_internal(
            Some("/path/to/missing-lightpanda".to_string()),
            |name| {
                assert_eq!(name, "lightpanda");
                Some(PathBuf::from("/usr/local/bin/lightpanda"))
            },
            |_| false,
        );
        assert_eq!(resolved, Some(PathBuf::from("/usr/local/bin/lightpanda")));

        // 3. Env variable not set, resolves via path
        let resolved = resolve_lightpanda_internal(
            None,
            |name| {
                assert_eq!(name, "lightpanda");
                Some(PathBuf::from("/bin/lightpanda"))
            },
            |_| true,
        );
        assert_eq!(resolved, Some(PathBuf::from("/bin/lightpanda")));
    }

    #[test]
    fn test_resolve_chrome_internal() {
        // 1. Env variable set and points to existing file
        let resolved =
            resolve_chrome_internal(Some("/path/to/my-chrome".to_string()), |_| None, |_| true);
        assert_eq!(resolved, Some(PathBuf::from("/path/to/my-chrome")));

        // 2. Env variable set but not a file, falls back to path search
        let resolved = resolve_chrome_internal(
            Some("/path/to/dir".to_string()),
            |name| {
                if name == "google-chrome" {
                    Some(PathBuf::from("/usr/bin/google-chrome"))
                } else {
                    None
                }
            },
            |_| false,
        );
        assert_eq!(resolved, Some(PathBuf::from("/usr/bin/google-chrome")));

        // 3. Env variable not set, resolves via path search (first match)
        let resolved = resolve_chrome_internal(
            None,
            |name| {
                if name == "chromium" {
                    Some(PathBuf::from("/bin/chromium"))
                } else {
                    None
                }
            },
            |_| true,
        );
        assert_eq!(resolved, Some(PathBuf::from("/bin/chromium")));
    }
}
