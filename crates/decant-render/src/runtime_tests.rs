use super::*;

#[test]
fn accepts_mjs_script_when_type_is_missing() {
    // Given: a browser-observed module request without a CDP resource type.
    let request = ObservedRequest {
        url: "https://example.test/assets/app.mjs?v=1".to_string(),
        method: "GET".to_string(),
        kind: None,
    };

    // When: the runtime policy evaluates the request.
    let accepted = accepts_request(&request);

    // Then: Decant keeps it as a static script candidate.
    assert!(accepted);
    assert_eq!(classify_url(&request.url), ObservedResourceKind::Script);
}

#[test]
fn rejects_fetch_and_xhr_api_like_traffic() {
    // Given: API-like requests labeled as runtime data traffic by Chrome.
    let xhr = ObservedRequest {
        url: "https://example.test/api/users".to_string(),
        method: "GET".to_string(),
        kind: None,
    };
    let post = ObservedRequest {
        url: "https://example.test/assets/app.js".to_string(),
        method: "POST".to_string(),
        kind: Some(ObservedResourceKind::Script),
    };

    // When: the runtime policy evaluates the requests.
    let accepted_xhr = accepts_request(&xhr);
    let accepted_post = accepts_request(&post);

    // Then: Decant rejects data/API traffic and non-GET requests.
    assert!(!accepted_xhr);
    assert!(!accepted_post);
}

#[test]
fn collector_merges_response_metadata_by_url() {
    // Given: a static request followed by Chrome's response metadata.
    let mut collector = RuntimeResourceCollector::default();
    collector.record_request(
        "1".to_string(),
        ObservedRequest {
            url: "https://example.test/assets/site.css".to_string(),
            method: "GET".to_string(),
            kind: Some(ObservedResourceKind::Stylesheet),
        },
    );

    // When: the response arrives for the same request.
    collector.record_response(
        "1",
        ObservedResponse {
            url: "https://example.test/assets/site.css".to_string(),
            kind: ObservedResourceKind::Stylesheet,
            status: Some(200),
            mime_type: Some("text/css".to_string()),
        },
    );

    // Then: the public resource carries enough metadata for later fetching.
    assert_eq!(
        collector.finish(),
        vec![ObservedResource {
            url: "https://example.test/assets/site.css".to_string(),
            method: "GET".to_string(),
            kind: ObservedResourceKind::Stylesheet,
            status: Some(200),
            mime_type: Some("text/css".to_string()),
        }]
    );
}

#[test]
fn collector_handles_response_before_request() {
    let mut collector = RuntimeResourceCollector::default();
    collector.record_response(
        "late-request",
        ObservedResponse {
            url: "https://example.test/runtime/late-module.js?v=one".to_string(),
            kind: ObservedResourceKind::Script,
            status: Some(200),
            mime_type: Some("text/javascript".to_string()),
        },
    );
    collector.record_request(
        "late-request".to_string(),
        ObservedRequest {
            url: "https://example.test/runtime/late-module.js?v=one".to_string(),
            method: "GET".to_string(),
            kind: Some(ObservedResourceKind::Script),
        },
    );

    let resources = collector.finish();
    assert_eq!(resources.len(), 1);
    assert_eq!(
        resources[0].url,
        "https://example.test/runtime/late-module.js?v=one"
    );
    assert_eq!(resources[0].status, Some(200));
}
