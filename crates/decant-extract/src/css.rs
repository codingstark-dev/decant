//! CSS parsing and design-token extraction using `lightningcss`.
//!
//! # noqa: SIZE_OK - CSS URL rewriting and token extraction share parser state;
//! split only after a dedicated parser/rewrite fixture suite is in place.
//!
//! Given raw CSS bytes, this module parses the stylesheet and extracts:
//! - Colors (hex, rgb, hsl, named)
//! - Font families and size scale
//! - Spacing values (margin, padding, gap)
//! - Breakpoints (from `@media` queries)
//! - Border radii
//! - Box shadows

use crate::ExtractError;
use crate::tokens::{ColorTokens, DesignTokens, TypographyTokens};
use url::Url;

/// Parse `css_bytes` and return a [`DesignTokens`] snapshot.
/// Multiple snapshots can be merged via [`DesignTokens::merge`].
pub fn extract_tokens(css_bytes: &[u8]) -> Result<DesignTokens, ExtractError> {
    // lightningcss requires a string.
    let css = std::str::from_utf8(css_bytes).map_err(|e| ExtractError::CssParse(e.to_string()))?;

    // Use lightningcss to parse the stylesheet.
    // We will implement a full visitor once the lightningcss visitor API is stable.
    // For now, use regex-free heuristic extraction on the raw CSS string.
    let mut tokens = DesignTokens {
        schema_version: "1.0".into(),
        source: String::new(),
        captured_at: None,
        ..Default::default()
    };

    extract_colors(css, &mut tokens.colors);
    extract_typography(css, &mut tokens.typography);
    extract_spacing(css, &mut tokens.spacing);
    extract_breakpoints(css, &mut tokens.breakpoints);
    extract_radii(css, &mut tokens.radii);
    extract_shadows(css, &mut tokens.shadows);

    Ok(tokens)
}

/// Extract hex color values (#rgb, #rrggbb, #rrggbbaa).
fn extract_colors(css: &str, colors: &mut ColorTokens) {
    // Simple hex color extraction — a full implementation would also handle rgb()/hsl().
    let mut i = 0;
    let bytes = css.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_hexdigit()) && end - start < 8 {
                end += 1;
            }
            let len = end - start;
            if matches!(len, 3 | 4 | 6 | 8) {
                let hex = format!("#{}", &css[start..end]);
                if !colors.swatches.contains(&hex) {
                    colors.swatches.push(hex);
                }
            }
        }
        i += 1;
    }
}

/// Extract font-family declarations.
fn extract_typography(css: &str, typo: &mut TypographyTokens) {
    // Look for `font-family:` declarations.
    for line in css.lines() {
        let line = line.trim();
        if let Some(val) = line
            .strip_prefix("font-family:")
            .or_else(|| line.strip_prefix("font-family :"))
        {
            let family = val.trim().trim_end_matches(';').trim().to_string();
            if !family.is_empty() && !typo.font_families.contains(&family) {
                typo.font_families.push(family);
            }
        }
    }
}

/// Extract spacing values (margin/padding/gap) in px.
fn extract_spacing(css: &str, spacing: &mut Vec<f32>) {
    for part in css.split(|c: char| c.is_whitespace() || c == ';' || c == ':' || c == '(') {
        if let Some(val) = part.strip_suffix("px") {
            if let Ok(n) = val.trim().parse::<f32>() {
                if n > 0.0 && n <= 256.0 && !spacing.contains(&n) {
                    spacing.push(n);
                }
            }
        }
    }
    spacing.sort_by(f32::total_cmp);
    spacing.dedup();
}

/// Extract breakpoints from `@media` queries.
fn extract_breakpoints(css: &str, breakpoints: &mut Vec<u32>) {
    for chunk in css.split("@media") {
        // Look for patterns like `min-width: 768px` or `max-width: 1024px`.
        for part in chunk.split("width:") {
            let trimmed = part.trim().trim_start_matches('(').trim();
            if let Some(val) = trimmed.split(|c: char| !c.is_ascii_digit()).next() {
                if let Ok(n) = val.parse::<u32>() {
                    if n >= 320 && n <= 3840 && !breakpoints.contains(&n) {
                        breakpoints.push(n);
                    }
                }
            }
        }
    }
    breakpoints.sort();
    breakpoints.dedup();
}

/// Extract border-radius values.
fn extract_radii(css: &str, radii: &mut Vec<String>) {
    for line in css.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("border-radius:") {
            let r = val.trim().trim_end_matches(';').trim().to_string();
            if !r.is_empty() && !radii.contains(&r) {
                radii.push(r);
            }
        }
    }
}

/// Extract box-shadow values.
fn extract_shadows(css: &str, shadows: &mut Vec<String>) {
    for line in css.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("box-shadow:") {
            let s = val.trim().trim_end_matches(';').trim().to_string();
            if !s.is_empty() && !shadows.contains(&s) {
                shadows.push(s);
            }
        }
    }
}

/// Extract all asset URLs (e.g., fonts, background images) referenced via `url(...)` in CSS,
/// and rewrite them to relative local paths using the provided mapping closure.
pub fn extract_and_rewrite_css<F>(
    css_bytes: &[u8],
    base_url: &Url,
    map_url_to_rel_path: F,
) -> Result<(Vec<Url>, String), ExtractError>
where
    F: Fn(&Url) -> Option<String>,
{
    let css = String::from_utf8_lossy(css_bytes);
    let mut rewritten = String::with_capacity(css.len());
    let mut discovered_urls = Vec::new();

    let mut start_idx = 0;
    while let Some(pos) = css[start_idx..].find("url(") {
        let url_start = start_idx + pos;
        // Push everything before "url("
        rewritten.push_str(&css[start_idx..url_start]);

        let content_start = url_start + 4; // after "url("

        if let Some(content_end) = find_url_function_end(&css, content_start) {
            let raw_val = &css[content_start..content_end];

            let trimmed = raw_val.trim();
            let mut val = trimmed;
            if (val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\''))
            {
                if val.len() >= 2 {
                    val = &val[1..val.len() - 1];
                }
            }
            let val = val.trim();

            let mut rewritten_url = None;
            if !val.is_empty() {
                if let Ok(abs_url) = base_url.join(val) {
                    if abs_url.scheme() == "http" || abs_url.scheme() == "https" {
                        discovered_urls.push(abs_url.clone());
                        if let Some(rel_path) = map_url_to_rel_path(&abs_url) {
                            rewritten_url = Some(rel_path);
                        }
                    }
                }
            }

            if let Some(rel_path) = rewritten_url {
                rewritten.push_str(&format!("url(\"{}\")", rel_path));
            } else {
                rewritten.push_str(&format!("url({})", raw_val));
            }

            start_idx = content_end + 1;
        } else {
            // No closing paren, push the rest and stop
            rewritten.push_str(&css[url_start..]);
            start_idx = css.len();
            break;
        }
    }

    if start_idx < css.len() {
        rewritten.push_str(&css[start_idx..]);
    }

    discovered_urls.sort();
    discovered_urls.dedup();

    Ok((discovered_urls, rewritten))
}

fn find_url_function_end(css: &str, content_start: usize) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;

    for (offset, ch) in css[content_start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        match quote {
            Some(q) if ch == q => quote = None,
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == ')' => return Some(content_start + offset),
            Some(_) | None => {}
        }
    }

    None
}

#[cfg(test)]
#[path = "css_tests.rs"]
mod tests;
