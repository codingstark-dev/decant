use url::Url;

pub(super) fn collect_asset_url(
    raw: &str,
    base_url: &Url,
    collected_refs: &mut std::collections::HashSet<String>,
    asset_links: &mut Vec<Url>,
) {
    collected_refs.insert(raw.to_string());
    if let Ok(abs) = base_url.join(raw) {
        if abs.scheme() == "http" || abs.scheme() == "https" {
            asset_links.push(abs);
        }
    }
}

pub(super) fn parse_srcset_urls(srcset: &str) -> impl Iterator<Item = &str> {
    srcset.split(',').filter_map(|candidate| {
        candidate
            .split_whitespace()
            .next()
            .filter(|url| !url.is_empty())
    })
}

pub(super) fn rewrite_srcset<F>(
    srcset: &str,
    base_url: &Url,
    map_url_to_rel_path: &F,
) -> Option<String>
where
    F: Fn(&Url) -> Option<String>,
{
    rewrite_joined_url_list(srcset, base_url, map_url_to_rel_path, |candidate| {
        candidate
            .find(char::is_whitespace)
            .map(|index| candidate.split_at(index))
    })
}

pub(super) fn rewrite_comma_url_list<F>(
    list: &str,
    base_url: &Url,
    map_url_to_rel_path: &F,
) -> Option<String>
where
    F: Fn(&Url) -> Option<String>,
{
    rewrite_joined_url_list(list, base_url, map_url_to_rel_path, |_| None)
}

fn rewrite_joined_url_list<F, S>(
    list: &str,
    base_url: &Url,
    map_url_to_rel_path: &F,
    split_suffix: S,
) -> Option<String>
where
    F: Fn(&Url) -> Option<String>,
    S: Fn(&str) -> Option<(&str, &str)>,
{
    let mut changed = false;
    let rewritten = list
        .split(',')
        .map(|candidate| {
            let leading_len = candidate.len() - candidate.trim_start().len();
            let trailing_len = candidate.len() - candidate.trim_end().len();
            let leading = &candidate[..leading_len];
            let trailing = &candidate[candidate.len() - trailing_len..];
            let trimmed = candidate.trim();
            if trimmed.is_empty() {
                return candidate.to_string();
            }

            let (url_part, suffix) = split_suffix(trimmed).unwrap_or((trimmed, ""));
            let Ok(abs_url) = base_url.join(url_part) else {
                return candidate.to_string();
            };
            let Some(rel_path) = map_url_to_rel_path(&abs_url) else {
                return candidate.to_string();
            };

            changed = true;
            format!("{leading}{rel_path}{suffix}{trailing}")
        })
        .collect::<Vec<_>>()
        .join(",");

    changed.then_some(rewritten)
}

pub(super) fn strip_integrity_attrs(html: &str) -> String {
    let mut rewritten = html.to_string();
    for quote in ['"', '\''] {
        while let Some(start) = rewritten.find(&format!(" integrity={quote}")) {
            let value_start = start + " integrity=".len() + 1;
            let Some(value_end_offset) = rewritten[value_start..].find(quote) else {
                break;
            };
            let end = value_start + value_end_offset + 1;
            rewritten.replace_range(start..end, "");
        }
    }
    rewritten
}

pub(super) fn is_non_asset_link_rel(rel: &str) -> bool {
    rel.split_whitespace().any(|part| {
        matches!(
            part.to_ascii_lowercase().as_str(),
            "preconnect" | "dns-prefetch" | "canonical" | "alternate"
        )
    })
}
