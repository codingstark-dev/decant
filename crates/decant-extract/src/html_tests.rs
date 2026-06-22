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

#[test]
fn rewrites_query_distinguished_asset_urls() {
    let html = r#"<html><head>
            <link href="/assets/app.css?v=1" rel="stylesheet" integrity="sha384-old">
            <link href="/assets/app.css?v=2" rel="stylesheet">
            <script src="/assets/app.js?v=1" integrity='sha384-script'></script>
        </head><body>
            <img src="/images/hero.png?v=1">
            <img srcset="/images/card.png?v=1 500w, /images/card.png?v=2 1000w" src="/images/card.png?v=1">
            <video poster="/media/poster.jpg?v=1">
                <source src="/media/clip.mp4?v=1">
            </video>
            <div data-poster-url="/media/bg.jpg?v=1" data-video-urls="/media/bg.mp4?v=1,/media/bg.mp4?v=2"></div>
        </body></html>"#;
    let base = Url::parse("https://example.com/").unwrap();
    let (links, rewritten) = extract_and_rewrite(html.as_bytes(), &base, |url| {
        match (url.path(), url.query()) {
            ("/assets/app.css", Some("v=1")) => Some("assets/app.q-one.css".to_string()),
            ("/assets/app.css", Some("v=2")) => Some("assets/app.q-two.css".to_string()),
            ("/assets/app.js", Some("v=1")) => Some("assets/app.q-one.js".to_string()),
            ("/images/hero.png", Some("v=1")) => Some("images/hero.q-one.png".to_string()),
            ("/images/card.png", Some("v=1")) => Some("images/card.q-one.png".to_string()),
            ("/images/card.png", Some("v=2")) => Some("images/card.q-two.png".to_string()),
            ("/media/poster.jpg", Some("v=1")) => Some("media/poster.q-one.jpg".to_string()),
            ("/media/clip.mp4", Some("v=1")) => Some("media/clip.q-one.mp4".to_string()),
            ("/media/bg.jpg", Some("v=1")) => Some("media/bg.q-one.jpg".to_string()),
            ("/media/bg.mp4", Some("v=1")) => Some("media/bg.q-one.mp4".to_string()),
            ("/media/bg.mp4", Some("v=2")) => Some("media/bg.q-two.mp4".to_string()),
            _ => None,
        }
    })
    .unwrap();

    assert_eq!(links.asset_links.len(), 11);
    assert!(
        links
            .asset_links
            .contains(&Url::parse("https://example.com/assets/app.css?v=1").unwrap())
    );
    assert!(
        links
            .asset_links
            .contains(&Url::parse("https://example.com/assets/app.css?v=2").unwrap())
    );
    assert!(rewritten.contains(r#"href="assets/app.q-one.css""#));
    assert!(rewritten.contains(r#"href="assets/app.q-two.css""#));
    assert!(rewritten.contains(r#"src="assets/app.q-one.js""#));
    assert!(rewritten.contains(r#"src="images/hero.q-one.png""#));
    assert!(
        rewritten.contains(r#"srcset="images/card.q-one.png 500w, images/card.q-two.png 1000w""#)
    );
    assert!(rewritten.contains(r#"poster="media/poster.q-one.jpg""#));
    assert!(rewritten.contains(r#"src="media/clip.q-one.mp4""#));
    assert!(rewritten.contains(r#"data-poster-url="media/bg.q-one.jpg""#));
    assert!(rewritten.contains(r#"data-video-urls="media/bg.q-one.mp4,media/bg.q-two.mp4""#));
    assert!(!rewritten.contains("integrity="));
}

#[test]
fn rewrites_webflow_cdn_assets_and_removes_integrity() {
    let html = r#"<html><head>
            <link href="https://cdn.prod.website-files.com/site/css/site.css" rel="stylesheet" integrity="sha384-old" crossorigin="anonymous">
            <script src="https://cdn.prod.website-files.com/site/js/webflow.js" integrity='sha384-script'></script>
        </head><body>
            <img src="https://cdn.prod.website-files.com/site/logo.svg">
            <img srcset="https://cdn.prod.website-files.com/site/a-500.avif 500w, https://cdn.prod.website-files.com/site/a.avif 1000w" src="https://cdn.prod.website-files.com/site/a.avif">
            <video poster="https://cdn.prod.website-files.com/site/poster.jpg">
                <source src="https://cdn.prod.website-files.com/site/clip.mp4">
            </video>
            <div data-poster-url="https://cdn.prod.website-files.com/site/bg.jpg" data-video-urls="https://cdn.prod.website-files.com/site/bg.mp4,https://cdn.prod.website-files.com/site/bg.webm"></div>
        </body></html>"#;
    let base = Url::parse("https://fullstack-studio.webflow.io/").unwrap();
    let (links, rewritten) = extract_and_rewrite(html.as_bytes(), &base, |url| {
        Some(url.path().trim_start_matches('/').to_string())
    })
    .unwrap();

    assert_eq!(links.asset_links.len(), 10);
    assert!(rewritten.contains(r#"href="site/css/site.css""#));
    assert!(rewritten.contains(r#"src="site/js/webflow.js""#));
    assert!(rewritten.contains(r#"srcset="site/a-500.avif 500w, site/a.avif 1000w""#));
    assert!(rewritten.contains(r#"poster="site/poster.jpg""#));
    assert!(rewritten.contains(r#"data-poster-url="site/bg.jpg""#));
    assert!(rewritten.contains(r#"data-video-urls="site/bg.mp4,site/bg.webm""#));
    assert!(!rewritten.contains("integrity="));
}
