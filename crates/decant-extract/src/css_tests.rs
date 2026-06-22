use super::*;
use url::Url;

#[test]
fn test_extract_and_rewrite_css() {
    let css = r#"
            @font-face {
                font-family: 'Geist';
                src: url(/assets/geist.woff2) format('woff2');
            }
            body {
                background-image: url('/img/bg.png');
            }
        "#;
    let base = Url::parse("https://example.com/assets/app.css").unwrap();
    let (urls, rewritten) = extract_and_rewrite_css(css.as_bytes(), &base, |url| {
        if url.path().ends_with(".woff2") {
            Some("geist.woff2".to_string())
        } else if url.path().ends_with(".png") {
            Some("../img/bg.png".to_string())
        } else {
            None
        }
    })
    .unwrap();

    assert_eq!(urls.len(), 2);
    assert!(urls.contains(&Url::parse("https://example.com/assets/geist.woff2").unwrap()));
    assert!(urls.contains(&Url::parse("https://example.com/img/bg.png").unwrap()));

    assert!(rewritten.contains("url(\"geist.woff2\")"));
    assert!(rewritten.contains("url(\"../img/bg.png\")"));
}

#[test]
fn rewrites_query_distinguished_css_urls() {
    let css = r#"
            @font-face {
                font-family: 'Geist';
                src: url("/assets/geist.woff2?v=1") format("woff2"),
                     url("/assets/geist.woff2?v=2") format("woff2");
            }
            body {
                background-image: url('/img/bg.png?v=1');
                mask-image: url(/img/bg.png?v=2);
            }
        "#;
    let base = Url::parse("https://example.com/assets/app.css").unwrap();
    let (urls, rewritten) = extract_and_rewrite_css(css.as_bytes(), &base, |url| {
        match (url.path(), url.query()) {
            ("/assets/geist.woff2", Some("v=1")) => Some("assets/geist.q-one.woff2".to_string()),
            ("/assets/geist.woff2", Some("v=2")) => Some("assets/geist.q-two.woff2".to_string()),
            ("/img/bg.png", Some("v=1")) => Some("img/bg.q-one.png".to_string()),
            ("/img/bg.png", Some("v=2")) => Some("img/bg.q-two.png".to_string()),
            _ => None,
        }
    })
    .unwrap();

    assert_eq!(urls.len(), 4);
    assert!(urls.contains(&Url::parse("https://example.com/assets/geist.woff2?v=1").unwrap()));
    assert!(urls.contains(&Url::parse("https://example.com/assets/geist.woff2?v=2").unwrap()));
    assert!(rewritten.contains(r#"url("assets/geist.q-one.woff2")"#));
    assert!(rewritten.contains(r#"url("assets/geist.q-two.woff2")"#));
    assert!(rewritten.contains(r#"url("img/bg.q-one.png")"#));
    assert!(rewritten.contains(r#"url("img/bg.q-two.png")"#));
}

#[test]
fn rewrites_quoted_css_url_when_filename_contains_parentheses() {
    let css = r#"
            .button {
                background-image: url("https://cdn.example.com/site/arrow-small-right (22).svg");
            }
        "#;
    let base = Url::parse("https://example.com/style.css").unwrap();
    let target = Url::parse("https://cdn.example.com/site/arrow-small-right%20(22).svg").unwrap();
    let (urls, rewritten) = extract_and_rewrite_css(css.as_bytes(), &base, |url| {
        if *url == target {
            Some("site/arrow-small-right-22.svg".to_string())
        } else {
            None
        }
    })
    .unwrap();

    assert_eq!(urls, vec![target]);
    assert!(rewritten.contains(r#"url("site/arrow-small-right-22.svg")"#));
    assert!(!rewritten.contains("%202.svg"));
}

#[test]
fn extracts_hex_colors() {
    let css = "body { color: #fff; background: #001122; border: 1px solid #abc; }";
    let tokens = extract_tokens(css.as_bytes()).unwrap();
    assert!(tokens.colors.swatches.contains(&"#fff".to_string()));
    assert!(tokens.colors.swatches.contains(&"#001122".to_string()));
    assert!(tokens.colors.swatches.contains(&"#abc".to_string()));
}

#[test]
fn extracts_font_families() {
    let css = "body {\n    font-family: 'Inter', sans-serif;\n}";
    let tokens = extract_tokens(css.as_bytes()).unwrap();
    assert!(!tokens.typography.font_families.is_empty());
}

#[test]
fn extracts_breakpoints() {
    let css = "@media (min-width: 768px) { } @media (max-width: 1024px) { }";
    let tokens = extract_tokens(css.as_bytes()).unwrap();
    assert!(tokens.breakpoints.contains(&768));
    assert!(tokens.breakpoints.contains(&1024));
}

#[test]
fn extract_rewrite_css_no_urls() {
    // CSS with no url() references should pass through unchanged
    let css = "body { color: red; font-size: 16px; }";
    let base = Url::parse("https://example.com/style.css").unwrap();
    let (urls, rewritten) = extract_and_rewrite_css(css.as_bytes(), &base, |_| None).unwrap();
    assert!(urls.is_empty());
    assert_eq!(rewritten, css);
}

#[test]
fn extract_rewrite_css_data_uri() {
    // data: URIs should be left alone (not treated as asset references)
    let css = r#"body { background: url("data:image/png;base64,abc=="); }"#;
    let base = Url::parse("https://example.com/style.css").unwrap();
    let (urls, _) = extract_and_rewrite_css(css.as_bytes(), &base, |_| None).unwrap();
    assert!(
        urls.is_empty(),
        "data: URIs should not be collected as URLs"
    );
}
