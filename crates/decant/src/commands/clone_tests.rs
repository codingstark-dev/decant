use super::*;
use crate::commands::clone::repair::{RepairCategory, classify_repair_issue};

#[test]
fn test_relative_path_flat_to_nested() {
    let from = Path::new("");
    let to = Path::new("assets/app.css");
    assert_eq!(relative_path(from, to), PathBuf::from("assets/app.css"));
}

#[test]
fn test_relative_path_nested_to_sibling() {
    let from = Path::new("about");
    let to = Path::new("about/team/index.html");
    assert_eq!(relative_path(from, to), PathBuf::from("team/index.html"));
}

#[test]
fn test_relative_path_nested_to_root_sibling() {
    let from = Path::new("about/team");
    let to = Path::new("assets/app.css");
    assert_eq!(
        relative_path(from, to),
        PathBuf::from("../../assets/app.css")
    );
}

#[test]
fn test_relative_path_same_directory() {
    let from = Path::new("about");
    let to = Path::new("about/index.html");
    assert_eq!(relative_path(from, to), PathBuf::from("index.html"));
}

#[test]
fn page_manifest_url_preserves_query() {
    let url = Url::parse("https://example.com/search?q=decant&page=2#ignored").unwrap();
    assert_eq!(page_manifest_url(&url), "/search?q=decant&page=2");
}

#[test]
fn repair_issue_classifies_malformed_css_url() {
    assert!(matches!(
        classify_repair_issue(
            "https://cdn.example.com/css/%22https://cdn.example.com/icon%20(22",
            "HTTP status client error (403 Forbidden)",
        ),
        RepairCategory::MalformedCssUrl
    ));
}

#[test]
fn repair_issue_treats_google_identity_as_optional() {
    assert!(matches!(
        classify_repair_issue(
            "https://accounts.google.com/gsi/client",
            "HTTP status client error (403 Forbidden)",
        ),
        RepairCategory::ThirdPartyOptional
    ));
}

#[test]
fn repair_issue_classifies_first_party_blocked_asset() {
    assert!(matches!(
        classify_repair_issue(
            "https://example.com/assets/hero.jpg",
            "HTTP status client error (403 Forbidden)",
        ),
        RepairCategory::BlockedAsset
    ));
}
