use std::path::{Path, PathBuf};

use serde::Serialize;
use url::Url;

#[derive(Serialize)]
pub(super) struct RepairHints {
    schema_version: &'static str,
    source: String,
    generated_at: chrono::DateTime<chrono::Utc>,
    status: RepairStatus,
    summary: RepairSummary,
    issues: Vec<RepairIssue>,
    ai_next_steps: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum RepairStatus {
    Clean,
    NeedsRepair,
}

#[derive(Serialize)]
struct RepairSummary {
    captured: usize,
    errors: usize,
}

#[derive(Serialize)]
struct RepairIssue {
    url: String,
    category: RepairCategory,
    message: String,
    suggested_action: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RepairCategory {
    BlockedAsset,
    MissingAsset,
    MalformedCssUrl,
    NetworkRetry,
    ThirdPartyOptional,
    UnknownFetchFailure,
}

impl RepairHints {
    pub(super) fn new(seed_url: &Url, captured: usize, errors: &[(String, String)]) -> Self {
        let issues = errors
            .iter()
            .map(|(url, message)| {
                let category = classify_repair_issue(url, message);
                RepairIssue {
                    url: url.clone(),
                    category,
                    message: message.clone(),
                    suggested_action: suggested_repair_action(url, message),
                }
            })
            .collect::<Vec<_>>();

        let status = if issues.is_empty() {
            RepairStatus::Clean
        } else {
            RepairStatus::NeedsRepair
        };

        Self {
            schema_version: "1.0",
            source: seed_url.to_string(),
            generated_at: chrono::Utc::now(),
            status,
            summary: RepairSummary {
                captured,
                errors: errors.len(),
            },
            issues,
            ai_next_steps: vec![
                "Open the live URL and local preview at the same viewport.",
                "Compare screenshots before changing generated files.",
                "If repair-hints.json lists malformed_css_url, update URL rewriting and rerun clone.",
                "If blocked third-party assets are analytics or identity scripts, ignore them only when the viewport remains visually identical.",
                "If required visual assets are blocked, rerun with cookies, headers, or a browser-authenticated capture.",
            ],
        }
    }
}

pub(super) fn classify_repair_issue(url: &str, message: &str) -> RepairCategory {
    let lowered = format!("{} {}", url, message).to_lowercase();
    if lowered.contains("%22") || lowered.contains("css/\"") {
        RepairCategory::MalformedCssUrl
    } else if lowered.contains("403 forbidden") || lowered.contains("401 unauthorized") {
        if is_optional_third_party(url) {
            RepairCategory::ThirdPartyOptional
        } else {
            RepairCategory::BlockedAsset
        }
    } else if lowered.contains("404 not found") {
        if is_optional_third_party(url) {
            RepairCategory::ThirdPartyOptional
        } else {
            RepairCategory::MissingAsset
        }
    } else if lowered.contains("error sending request")
        || lowered.contains("timed out")
        || lowered.contains("connection")
    {
        RepairCategory::NetworkRetry
    } else {
        RepairCategory::UnknownFetchFailure
    }
}

fn suggested_repair_action(url: &str, message: &str) -> &'static str {
    match classify_repair_issue(url, message) {
        RepairCategory::MalformedCssUrl => {
            "Fix CSS url(...) parsing for quoted URLs, spaces, parentheses, or escapes, then rerun clone."
        }
        RepairCategory::BlockedAsset => {
            "Rerun with cookies or required headers; if the asset is still blocked, preserve the browser-rendered DOM and capture a visual fallback."
        }
        RepairCategory::MissingAsset => {
            "Check whether the URL was resolved from a relative runtime string; repair URL normalization or runtime asset discovery."
        }
        RepairCategory::NetworkRetry => {
            "Rerun with lower concurrency and retry the failed asset; keep the issue open if visual comparison changes."
        }
        RepairCategory::ThirdPartyOptional => {
            "Treat as optional only after screenshot comparison proves the local clone remains visually equivalent."
        }
        RepairCategory::UnknownFetchFailure => {
            "Inspect the failed URL, response, and local screenshot before deciding whether to add a parser rule or auth input."
        }
    }
}

fn is_optional_third_party(url: &str) -> bool {
    const OPTIONAL_HOSTS: [&str; 10] = [
        "accounts.google.com",
        "analytics.google.com",
        "connect.facebook.net",
        "google-analytics.com",
        "googletagmanager.com",
        "snippet.growsumo.com",
        "app.cal.com",
        "pagead2.googlesyndication.com",
        "clarity.ms",
        "recaptcha.net",
    ];

    OPTIONAL_HOSTS.iter().any(|host| url.contains(host))
}

pub(super) fn relative_path(from_dir: &Path, to: &Path) -> PathBuf {
    let from_comps: Vec<_> = from_dir.components().collect();
    let to_comps: Vec<_> = to.components().collect();

    let mut common_prefix_len = 0;
    for (from, to) in from_comps.iter().zip(to_comps.iter()) {
        if from == to {
            common_prefix_len += 1;
        } else {
            break;
        }
    }

    let mut result = PathBuf::new();
    for _ in common_prefix_len..from_comps.len() {
        result.push("..");
    }
    for comp in &to_comps[common_prefix_len..] {
        result.push(comp);
    }

    result
}

pub(super) fn is_optional_missing_asset(url: &Url) -> bool {
    url.path().eq_ignore_ascii_case("/favicon.ico")
}

pub(super) fn page_manifest_url(url: &Url) -> String {
    match url.query() {
        Some(query) if !query.is_empty() => format!("{}?{query}", url.path()),
        _ => url.path().to_string(),
    }
}
