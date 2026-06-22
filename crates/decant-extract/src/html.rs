//! HTML parsing — link extraction and path rewriting.
//!
//! Given an HTML document and its source URL, this module:
//! # noqa: SIZE_OK - keeps parsing, discovery, and rewriting together so selector
//! semantics and rewrite fixtures stay in one extraction boundary until split tests
//! are introduced.
//! 1. Finds all navigable links (`<a href>`, `<link href>`, `<script src>`, `<img src>`, etc.).
//! 2. Rewrites absolute/root-relative references to relative local paths so the
//!    mirrored site works offline.
//! 3. Returns the list of discovered URLs for the frontier.

#[path = "html_rewrite.rs"]
mod html_rewrite;

use scraper::{Html, Selector};
use url::Url;

use html_rewrite::{
    collect_asset_url, is_non_asset_link_rel, parse_srcset_urls, rewrite_comma_url_list,
    rewrite_srcset, strip_integrity_attrs,
};

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
    let mut srcset_refs = Vec::new();
    let mut comma_list_refs = Vec::new();

    // Collect page-level <a href> links.
    let a_sel = parse_selector("a[href]")?;
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

    let asset_sel = parse_selector(
        "link[href], script[src], img[src], source[src], video[src], video[poster], [srcset], [data-poster-url], [data-video-urls]",
    )?;
    for el in document.select(&asset_sel) {
        let is_ignored_link = el.value().name() == "link"
            && el.value().attr("rel").is_some_and(is_non_asset_link_rel);
        if is_ignored_link {
            continue;
        }

        let href = el.value().attr("href").or_else(|| el.value().attr("src"));
        if let Some(href) = href {
            collect_asset_url(href, base_url, &mut collected_refs, &mut links.asset_links);
        }
        if let Some(poster) = el.value().attr("poster") {
            collect_asset_url(
                poster,
                base_url,
                &mut collected_refs,
                &mut links.asset_links,
            );
        }
        if let Some(data_poster) = el.value().attr("data-poster-url") {
            collect_asset_url(
                data_poster,
                base_url,
                &mut collected_refs,
                &mut links.asset_links,
            );
        }
        if let Some(srcset) = el.value().attr("srcset") {
            srcset_refs.push(srcset.to_string());
            for candidate in parse_srcset_urls(srcset) {
                if let Ok(abs) = base_url.join(candidate) {
                    if abs.scheme() == "http" || abs.scheme() == "https" {
                        links.asset_links.push(abs);
                    }
                }
            }
        }
        if let Some(video_urls) = el.value().attr("data-video-urls") {
            comma_list_refs.push(video_urls.to_string());
            for candidate in video_urls
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                if let Ok(abs) = base_url.join(candidate) {
                    if abs.scheme() == "http" || abs.scheme() == "https" {
                        links.asset_links.push(abs);
                    }
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

                let d_poster_from = format!("poster=\"{}\"", original);
                let d_poster_to = format!("poster=\"{}\"", rel_path);
                rewritten = rewritten.replace(&d_poster_from, &d_poster_to);

                let s_poster_from = format!("poster='{}'", original);
                let s_poster_to = format!("poster='{}'", rel_path);
                rewritten = rewritten.replace(&s_poster_from, &s_poster_to);

                let d_data_poster_from = format!("data-poster-url=\"{}\"", original);
                let d_data_poster_to = format!("data-poster-url=\"{}\"", rel_path);
                rewritten = rewritten.replace(&d_data_poster_from, &d_data_poster_to);

                let s_data_poster_from = format!("data-poster-url='{}'", original);
                let s_data_poster_to = format!("data-poster-url='{}'", rel_path);
                rewritten = rewritten.replace(&s_data_poster_from, &s_data_poster_to);
            }
        }
    }

    for original_srcset in srcset_refs {
        let rewritten_srcset = rewrite_srcset(&original_srcset, base_url, &map_url_to_rel_path)
            .unwrap_or_else(|| original_srcset.clone());
        rewritten = rewritten.replace(
            &format!("srcset=\"{original_srcset}\""),
            &format!("srcset=\"{rewritten_srcset}\""),
        );
        rewritten = rewritten.replace(
            &format!("srcset='{original_srcset}'"),
            &format!("srcset='{rewritten_srcset}'"),
        );
    }

    for original_list in comma_list_refs {
        let rewritten_list = rewrite_comma_url_list(&original_list, base_url, &map_url_to_rel_path)
            .unwrap_or_else(|| original_list.clone());
        rewritten = rewritten.replace(
            &format!("data-video-urls=\"{original_list}\""),
            &format!("data-video-urls=\"{rewritten_list}\""),
        );
        rewritten = rewritten.replace(
            &format!("data-video-urls='{original_list}'"),
            &format!("data-video-urls='{rewritten_list}'"),
        );
    }

    links.page_links.sort();
    links.page_links.dedup();
    links.asset_links.sort();
    links.asset_links.dedup();

    Ok((links, strip_integrity_attrs(&rewritten)))
}

/// Extract the document title from HTML.
/// Returns `None` if there is no `<title>` element or if the title is empty/whitespace.
pub fn extract_title(html_bytes: &[u8]) -> Option<String> {
    let html = String::from_utf8_lossy(html_bytes);
    let document = Html::parse_document(&html);
    let title_sel = Selector::parse("title").ok()?;
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
        if Selector::parse(tag)
            .ok()
            .and_then(|sel| document.select(&sel).next())
            .is_some()
        {
            regions.push((*tag).to_string());
        }
    }

    // Heuristic: look for common class/id names that imply component regions.
    let Some(landmark_sel) = Selector::parse("[class], [id]").ok() else {
        return regions;
    };
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

fn parse_selector(selector: &str) -> Result<Selector, ExtractError> {
    Selector::parse(selector).map_err(|e| ExtractError::HtmlParse(e.to_string()))
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
