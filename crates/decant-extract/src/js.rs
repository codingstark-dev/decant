//! JS parsing and dynamic chunk extraction.
//!
//! This module scans JavaScript source code for relative/absolute references
//! to other JS files, allowing decant to download dynamically loaded chunks.

use url::Url;

/// Scan JavaScript bytes for quoted relative or absolute `.js` file paths.
///
/// Looks for single-quoted, double-quoted, or backtick-enclosed string literals
/// that look like paths (starting with `./`, `../`, or `/`), end with `.js`, and
/// only contain valid path characters.
///
/// Dynamic chunk URLs are returned resolved against `base_url`.
pub fn extract_js_dependencies(js_bytes: &[u8], base_url: &Url) -> Vec<Url> {
    let js = String::from_utf8_lossy(js_bytes);
    let mut discovered = Vec::new();
    let quote_chars = ['\'', '"', '`'];

    let chars: Vec<char> = js.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if quote_chars.contains(&c) {
            let quote = c;
            let mut found_match = false;
            let mut path_chars = Vec::new();
            let mut escaped = false;

            for j in 1..2048 {
                if i + j >= chars.len() {
                    break;
                }
                let next_c = chars[i + j];
                if escaped {
                    escaped = false;
                    path_chars.push(next_c);
                } else if next_c == '\\' {
                    escaped = true;
                } else if next_c == quote {
                    found_match = true;
                    break;
                } else {
                    if next_c == '\n' {
                        break;
                    }
                    path_chars.push(next_c);
                }
            }

            if found_match && !path_chars.is_empty() {
                let content: String = path_chars.into_iter().collect();
                let content_trimmed = content.trim();

                if is_javascript_module_path(content_trimmed)
                    && is_runtime_js_asset(content_trimmed, base_url)
                {
                    // In ES modules, relative imports must start with ./, ../ or /
                    // Vite preloads can also use assets/ paths which contain /
                    let is_path_structure = content_trimmed.starts_with("./")
                        || content_trimmed.starts_with("../")
                        || content_trimmed.starts_with("/")
                        || content_trimmed.contains('/');

                    if is_path_structure {
                        let is_valid_chars = content_trimmed.chars().all(|ch| {
                            ch.is_ascii_alphanumeric()
                                || ch == '.'
                                || ch == '_'
                                || ch == '-'
                                || ch == '/'
                                || ch == '@'
                        });

                        if is_valid_chars {
                            let mut resolved_url = None;
                            if content_trimmed.starts_with("assets/") {
                                if let Ok(root) = base_url.join("/") {
                                    if let Ok(abs_url) = root.join(content_trimmed) {
                                        resolved_url = Some(abs_url);
                                    }
                                }
                            } else if content_trimmed.starts_with("static/")
                                && base_url.path().contains("/_next/")
                            {
                                if let Ok(next_root) = base_url.join("/_next/") {
                                    if let Ok(abs_url) = next_root.join(content_trimmed) {
                                        resolved_url = Some(abs_url);
                                    }
                                }
                            }

                            if resolved_url.is_none() {
                                if let Ok(abs_url) = base_url.join(content_trimmed) {
                                    resolved_url = Some(abs_url);
                                }
                            }

                            if let Some(abs_url) = resolved_url {
                                if abs_url.scheme() == "http" || abs_url.scheme() == "https" {
                                    discovered.push(abs_url);
                                }
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    discovered.sort();
    discovered.dedup();
    discovered
}

fn is_javascript_module_path(path: &str) -> bool {
    path.ends_with(".js") || path.ends_with(".mjs")
}

fn is_runtime_js_asset(path: &str, base_url: &Url) -> bool {
    if path.starts_with("node_modules/") || path.contains("/node_modules/") {
        return false;
    }

    if base_url
        .host_str()
        .is_some_and(|host| host.contains("googletagmanager.com"))
    {
        return false;
    }

    !path.contains("googletagmanager.com/gtm.js") && !path.contains("googletagmanager.com/gtag/js")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_js_dependencies() {
        let js = r#"
            import(`./architecture-BiNfUSt-.js`);
            const blog = import("./blog-2upzhdei.js");
            const privacy = import Parisian from './privacy-Bu0nJDRD.js';
            const relative = "../chunks/other.js";
            const absolute = "/assets/root.js";
            const invalid = "not-a-path.js";
            const escaped = "with-\"-escaped.js";
        "#;
        let base = Url::parse("https://example.com/assets/index.js").unwrap();
        let urls = extract_js_dependencies(js.as_bytes(), &base);

        assert_eq!(urls.len(), 5);
        assert!(
            urls.contains(
                &Url::parse("https://example.com/assets/architecture-BiNfUSt-.js").unwrap()
            )
        );
        assert!(urls.contains(&Url::parse("https://example.com/assets/blog-2upzhdei.js").unwrap()));
        assert!(
            urls.contains(&Url::parse("https://example.com/assets/privacy-Bu0nJDRD.js").unwrap())
        );
        assert!(urls.contains(&Url::parse("https://example.com/chunks/other.js").unwrap()));
        assert!(urls.contains(&Url::parse("https://example.com/assets/root.js").unwrap()));
    }

    #[test]
    fn next_static_chunk_paths_resolve_from_next_root() {
        let js = r#"
            self.__BUILD_MANIFEST = {
                "/": ["static/chunks/0wvvvwsvs5j68.js", "static/chunks/0.4wbo1xxqsn..js"]
            };
        "#;
        let base = Url::parse("https://www.mantri.dev/_next/static/chunks/app.js").unwrap();
        let urls = extract_js_dependencies(js.as_bytes(), &base);

        assert_eq!(urls.len(), 2);
        assert!(urls.contains(
            &Url::parse("https://www.mantri.dev/_next/static/chunks/0wvvvwsvs5j68.js").unwrap()
        ));
        assert!(urls.contains(
            &Url::parse("https://www.mantri.dev/_next/static/chunks/0.4wbo1xxqsn..js").unwrap()
        ));
        assert!(
            !urls
                .iter()
                .any(|url| url.path().contains("/_next/static/chunks/static/chunks/"))
        );
    }

    #[test]
    fn ignores_dev_package_and_analytics_js_references() {
        let js = r#"
            const package_source = "/node_modules/eventemitter3/index.js";
            const package_nested = "/node_modules/hoist-non-react-statics/dist/hoist-non-react-statics.cjs.js";
            const gtm = "https://www.googletagmanager.com/gtm.js";
            const framer_chunk = "./PX9hIOIVM.BvUUs-6a.mjs";
            const real_chunk = "./chunk-app.js";
        "#;
        let base = Url::parse("https://example.com/assets/app.js").unwrap();
        let urls = extract_js_dependencies(js.as_bytes(), &base);

        assert_eq!(
            urls,
            vec![
                Url::parse("https://example.com/assets/PX9hIOIVM.BvUUs-6a.mjs").unwrap(),
                Url::parse("https://example.com/assets/chunk-app.js").unwrap()
            ]
        );

        let gtm_base = Url::parse("https://www.googletagmanager.com/gtm.js").unwrap();
        let gtm_urls = extract_js_dependencies(br#""/local-helper.js""#, &gtm_base);
        assert!(gtm_urls.is_empty());
    }
}
