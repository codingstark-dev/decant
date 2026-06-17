//! HTML parsing — link extraction and path rewriting.
//!
//! Given an HTML document and its source URL, this module:
//! 1. Finds all navigable links (`<a href>`, `<link href>`, `<script src>`, `<img src>`, etc.).
//! 2. Rewrites absolute/root-relative references to relative local paths so the
//!    mirrored site works offline.
//! 3. Returns the list of discovered URLs for the frontier.

use scraper::{Html, Selector};
use url::Url;

use crate::ExtractError;

/// All URLs discovered in an HTML document.
#[derive(Debug, Default, Clone)]
pub struct ExtractedLinks {
    /// Page-level links (follow for recursion, if in scope).
    pub page_links: Vec<Url>,
    /// Asset links (always download; don't recurse).
    pub asset_links: Vec<Url>,
}

/// Extract all links from `html_bytes` relative to `base_url`.
///
/// Returns discovered links and the rewritten HTML (with all absolute/root-relative
/// URLs replaced by relative local paths).
pub fn extract_and_rewrite<F>(
    html_bytes: &[u8],
    base_url: &Url,
    map_url_to_rel_path: F,
) -> Result<(ExtractedLinks, String), ExtractError>
where
    F: Fn(&Url) -> Option<String>,
{
    let html = String::from_utf8_lossy(html_bytes);
    let document = Html::parse_document(&html);
    let mut links = ExtractedLinks::default();

    let mut collected_refs = std::collections::HashSet::new();

    // Collect page-level <a href> links.
    let a_sel = Selector::parse("a[href]").unwrap();
    for el in document.select(&a_sel) {
        if let Some(href) = el.value().attr("href") {
            collected_refs.insert(href.to_string());
            if let Ok(abs) = base_url.join(href) {
                if abs.scheme() == "http" || abs.scheme() == "https" {
                    links.page_links.push(abs);
                }
            }
        }
    }

    // Collect asset links: <link href>, <script src>, <img src>, <source src/srcset>.
    let asset_sel = Selector::parse("link[href], script[src], img[src], source[src]").unwrap();
    for el in document.select(&asset_sel) {
        let href = el.value().attr("href").or_else(|| el.value().attr("src"));
        if let Some(href) = href {
            collected_refs.insert(href.to_string());
            if let Ok(abs) = base_url.join(href) {
                if abs.scheme() == "http" || abs.scheme() == "https" {
                    links.asset_links.push(abs);
                }
            }
        }
    }

    // Rewrite URLs in the HTML to relative local paths.
    let mut rewritten = html.into_owned();
    for original in collected_refs {
        if let Ok(abs_url) = base_url.join(&original) {
            if let Some(mut rel_path) = map_url_to_rel_path(&abs_url) {
                // If there's a fragment on the original URL, preserve it
                if let Some(frag) = abs_url.fragment() {
                    rel_path = format!("{}#{}", rel_path, frag);
                }

                // Perform exact attribute replacement to prevent false positives.
                let d_href_from = format!("href=\"{}\"", original);
                let d_href_to = format!("href=\"{}\"", rel_path);
                rewritten = rewritten.replace(&d_href_from, &d_href_to);

                let s_href_from = format!("href='{}'", original);
                let s_href_to = format!("href='{}'", rel_path);
                rewritten = rewritten.replace(&s_href_from, &s_href_to);

                let d_src_from = format!("src=\"{}\"", original);
                let d_src_to = format!("src=\"{}\"", rel_path);
                rewritten = rewritten.replace(&d_src_from, &d_src_to);

                let s_src_from = format!("src='{}'", original);
                let s_src_to = format!("src='{}'", rel_path);
                rewritten = rewritten.replace(&s_src_from, &s_src_to);
            }
        }
    }

    Ok((links, rewritten))
}

/// Extract the document title from HTML.
/// Returns `None` if there is no `<title>` element or if the title is empty/whitespace.
pub fn extract_title(html_bytes: &[u8]) -> Option<String> {
    let html = String::from_utf8_lossy(html_bytes);
    let document = Html::parse_document(&html);
    let title_sel = Selector::parse("title").unwrap();
    document.select(&title_sel).next().and_then(|el| {
        let text = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
        if text.is_empty() { None } else { Some(text) }
    })
}

/// Heuristically detect major UI regions from an HTML document.
/// Returns a list of region names (e.g. `["header", "nav", "hero", "footer"]`).
pub fn detect_regions(html_bytes: &[u8]) -> Vec<String> {
    let html = String::from_utf8_lossy(html_bytes);
    let document = Html::parse_document(&html);
    let mut regions = Vec::new();

    // Check for semantic HTML5 elements.
    for tag in &[
        "header", "nav", "main", "footer", "aside", "section", "article",
    ] {
        let sel = Selector::parse(tag).unwrap();
        if document.select(&sel).next().is_some() {
            regions.push((*tag).to_string());
        }
    }

    // Heuristic: look for common class/id names that imply component regions.
    let landmark_sel = Selector::parse("[class], [id]").unwrap();
    let heuristics = ["hero", "feature", "cta", "pricing", "testimonial", "banner"];
    let mut seen: std::collections::HashSet<String> = regions.iter().cloned().collect();

    for el in document.select(&landmark_sel) {
        let attrs = [el.value().attr("class"), el.value().attr("id")];
        for attr in attrs.into_iter().flatten() {
            for h in &heuristics {
                if attr.to_lowercase().contains(h) && seen.insert((*h).to_string()) {
                    regions.push((*h).to_string());
                }
            }
        }
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_correctly() {
        let html = r#"<html><head><title>  My Page Title  </title></head><body></body></html>"#;
        assert_eq!(
            extract_title(html.as_bytes()),
            Some("My Page Title".to_string())
        );
    }

    #[test]
    fn no_title_tag_returns_none() {
        let html = r#"<html><head></head><body></body></html>"#;
        assert_eq!(extract_title(html.as_bytes()), None);
    }

    #[test]
    fn empty_title_returns_none() {
        let html = r#"<html><head><title></title></head><body></body></html>"#;
        assert_eq!(extract_title(html.as_bytes()), None);
    }

    #[test]
    fn whitespace_only_title_returns_none() {
        let html = r#"<html><head><title>   </title></head><body></body></html>"#;
        assert_eq!(extract_title(html.as_bytes()), None);
    }

    #[test]
    fn extracts_anchor_links() {
        let html = r#"<html><body>
            <a href="/about">About</a>
            <a href="https://other.com/page">Other</a>
        </body></html>"#;
        let base = Url::parse("https://example.com/").unwrap();
        let (links, _) = extract_and_rewrite(html.as_bytes(), &base, |_| None).unwrap();
        assert_eq!(links.page_links.len(), 2);
    }

    #[test]
    fn detects_semantic_regions() {
        let html = r#"<html><body>
            <header>H</header>
            <nav>N</nav>
            <main><section class="hero-section">Hero</section></main>
            <footer>F</footer>
        </body></html>"#;
        let regions = detect_regions(html.as_bytes());
        assert!(regions.contains(&"header".to_string()));
        assert!(regions.contains(&"footer".to_string()));
        assert!(regions.contains(&"hero".to_string()));
    }

    #[test]
    fn rewrites_urls_correctly() {
        let html = r#"<html><body>
            <link href="/assets/app-71RbDVbK.css" rel="stylesheet">
            <script src="/assets/index-B8uUuc7f.js"></script>
            <a href="/about#team">Team</a>
        </body></html>"#;
        let base = Url::parse("https://example.com/docs/").unwrap();
        let (links, rewritten) = extract_and_rewrite(html.as_bytes(), &base, |url| {
            if url.path().ends_with(".css") {
                Some("../assets/app-71RbDVbK.css".to_string())
            } else if url.path().ends_with(".js") {
                Some("../assets/index-B8uUuc7f.js".to_string())
            } else if url.path().ends_with("/about") {
                Some("../about/index.html".to_string())
            } else {
                None
            }
        })
        .unwrap();

        assert_eq!(links.asset_links.len(), 2);
        assert_eq!(links.page_links.len(), 1);

        assert!(rewritten.contains(r#"href="../assets/app-71RbDVbK.css""#));
        assert!(rewritten.contains(r#"src="../assets/index-B8uUuc7f.js""#));
        assert!(rewritten.contains(r#"href="../about/index.html#team""#));
    }
}
