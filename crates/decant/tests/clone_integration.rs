#![deny(missing_docs)]
//! Integration tests for the decant CLI.

use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Integration test for cloning a local mock HTTP server.
#[tokio::test]
async fn test_local_clone() {
    // 1. Bind a mock HTTP server to an ephemeral port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // 2. Spawn the server task
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0; 2048];
                if let Ok(n) = socket.read(&mut buf).await {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    eprintln!("--- MOCK SERVER RECEIVED REQUEST ---\n{}", req);

                    let (body, mime) = if req.contains("GET /assets/style.css") {
                        ("@font-face { src: url(/assets/font.woff2); }", "text/css")
                    } else if req.contains("GET /assets/font.woff2") {
                        ("font-data", "font/woff2")
                    } else if req.contains("GET / ") || req.contains("GET /index.html") {
                        (
                            "<html><head><link rel=\"stylesheet\" href=\"/assets/style.css\"></head><body><h1>Hello World</h1></body></html>",
                            "text/html",
                        )
                    } else {
                        ("", "text/plain")
                    };

                    let status = if body.is_empty()
                        && !req.contains("GET /assets/style.css")
                        && !req.contains("GET /assets/font.woff2")
                        && !req.contains("GET / ")
                        && !req.contains("GET /index.html")
                    {
                        "404 Not Found"
                    } else {
                        "200 OK"
                    };

                    let response = format!(
                        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        status,
                        mime,
                        body.len(),
                        body
                    );

                    eprintln!("--- MOCK SERVER SENDING RESPONSE ---\n{}", response);
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                }
            });
        }
    });

    // 3. Execute the built `decant` binary against the local mock server (using async Command to avoid deadlock)
    let bin_path = env!("CARGO_BIN_EXE_decant");
    let temp_dir = tempfile::tempdir().unwrap();
    let output = tokio::process::Command::new(bin_path)
        .arg("clone")
        .arg(format!("http://127.0.0.1:{}/", addr.port()))
        .arg("--output")
        .arg(temp_dir.path())
        .arg("--tui")
        .arg("false")
        .arg("--ignore-robots")
        .output()
        .await
        .expect("failed to execute decant binary");

    assert!(
        output.status.success(),
        "decant failed to clone local server. stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // 4. Verify outputs
    let index_path = temp_dir.path().join("index.html");
    assert!(index_path.exists());
    let html = fs::read_to_string(index_path).unwrap();

    // Check that the root-relative path has been rewritten to relative
    assert!(html.contains("href=\"assets/style.css\""));
    assert!(!html.contains("href=\"/assets/style.css\""));

    // Check that style.css was downloaded and written, and its font url was rewritten
    let css_path = temp_dir.path().join("assets/style.css");
    assert!(css_path.exists());
    let css = fs::read_to_string(css_path).unwrap();
    assert!(css.contains("url(\"font.woff2\")"));
    assert!(!css.contains("url(/assets/font.woff2)"));

    // Check that font.woff2 was downloaded and written
    let font_path = temp_dir.path().join("assets/font.woff2");
    assert!(font_path.exists());
    let font = fs::read_to_string(font_path).unwrap();
    assert_eq!(font, "font-data");

    // Check that standard decant files were generated
    assert!(temp_dir.path().join("manifest.json").exists());
    assert!(temp_dir.path().join("design-tokens.json").exists());
    assert!(temp_dir.path().join("context.md").exists());
}

/// Integration test for cloning the real multigres.com site (ignored by default).
#[tokio::test]
#[ignore]
async fn test_multigres_clone() {
    let bin_path = env!("CARGO_BIN_EXE_decant");
    let temp_dir = tempfile::tempdir().unwrap();
    let output = tokio::process::Command::new(bin_path)
        .arg("clone")
        .arg("https://multigres.com/")
        .arg("--output")
        .arg(temp_dir.path())
        .arg("--tui")
        .arg("false")
        .arg("--ignore-robots")
        .output()
        .await
        .expect("failed to execute decant binary");

    assert!(
        output.status.success(),
        "decant failed to clone multigres.com. stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify index.html was downloaded and contains relative assets rather than root-relative ones
    let index_path = temp_dir.path().join("index.html");
    assert!(index_path.exists());
    let html = fs::read_to_string(index_path).unwrap();

    // It should have rewritten "/assets/app-..." or "/assets/index-..." to "assets/app-..." or "assets/index-..."
    assert!(html.contains("assets/app-") || html.contains("assets/index-"));
    assert!(!html.contains("href=\"/assets/app-"));
    assert!(!html.contains("src=\"/assets/index-"));

    // Verify that the assets themselves were successfully downloaded
    let assets_dir = temp_dir.path().join("assets");
    assert!(assets_dir.exists());
    let files = fs::read_dir(assets_dir).unwrap();
    assert!(files.count() > 0, "assets directory should not be empty");

    // Verify manifests and context files
    assert!(temp_dir.path().join("manifest.json").exists());
    assert!(temp_dir.path().join("design-tokens.json").exists());
    assert!(temp_dir.path().join("context.md").exists());
}

#[cfg(feature = "render")]
#[tokio::test]
async fn runtime_captures_script_inserted_image_and_query_assets() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0; 4096];
                if let Ok(n) = socket.read(&mut buf).await {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or_default();
                    let (body, mime, status) = runtime_fixture_response(first_line);
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(&body).await;
                    let _ = socket.shutdown().await;
                }
            });
        }
    });

    let bin_path = env!("CARGO_BIN_EXE_decant");
    let temp_dir = tempfile::tempdir().unwrap();
    let output = tokio::process::Command::new(bin_path)
        .arg("clone")
        .arg(format!("http://127.0.0.1:{}/", addr.port()))
        .arg("--render")
        .arg("chrome")
        .arg("--runtime-capture")
        .arg("on")
        .arg("--output")
        .arg(temp_dir.path())
        .arg("--tui")
        .arg("false")
        .arg("--ignore-robots")
        .output()
        .await
        .expect("failed to execute decant binary");

    assert!(
        output.status.success(),
        "decant failed. stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("0 errors"),
        "expected zero errors. stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let paths = captured_relative_paths(temp_dir.path());
    assert!(
        paths
            .iter()
            .any(|path| path.starts_with("runtime/late-module.q-") && path.ends_with(".js")),
        "missing runtime module in {paths:?}"
    );
    assert!(
        paths
            .iter()
            .any(|path| path.starts_with("runtime/logo.q-") && path.ends_with(".png")),
        "missing query-suffixed runtime logo in {paths:?}"
    );
    let static_variants = paths
        .iter()
        .filter(|path| path.starts_with("runtime/static-pair.q-") && path.ends_with(".png"))
        .count();
    assert_eq!(
        static_variants, 2,
        "expected two query-distinct static variants"
    );
    assert!(
        !paths.iter().any(|path| path.contains("api")),
        "backend-like API response should not be written: {paths:?}"
    );
}

#[cfg(feature = "render")]
#[tokio::test]
async fn runtime_capture_off_skips_browser_observed_assets() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0; 4096];
                if let Ok(n) = socket.read(&mut buf).await {
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or_default();
                    let (body, mime, status) = runtime_fixture_response(first_line);
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.write_all(&body).await;
                    let _ = socket.shutdown().await;
                }
            });
        }
    });

    let bin_path = env!("CARGO_BIN_EXE_decant");
    let temp_dir = tempfile::tempdir().unwrap();
    let output = tokio::process::Command::new(bin_path)
        .arg("clone")
        .arg(format!("http://127.0.0.1:{}/", addr.port()))
        .arg("--render")
        .arg("chrome")
        .arg("--runtime-capture")
        .arg("off")
        .arg("--output")
        .arg(temp_dir.path())
        .arg("--tui")
        .arg("false")
        .arg("--ignore-robots")
        .output()
        .await
        .expect("failed to execute decant binary");

    assert!(
        output.status.success(),
        "decant failed. stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let paths = captured_relative_paths(temp_dir.path());
    assert!(
        paths
            .iter()
            .filter(|path| path.starts_with("runtime/static-pair.q-") && path.ends_with(".png"))
            .count()
            == 2,
        "static query assets should still be captured: {paths:?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.starts_with("runtime/late-module.q-")),
        "runtime module should not be captured when runtime capture is off: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.starts_with("runtime/logo.q-")),
        "rendered DOM image should still be captured from final HTML: {paths:?}"
    );
}

#[cfg(feature = "render")]
fn runtime_fixture_response(first_line: &str) -> (Vec<u8>, &'static str, &'static str) {
    if first_line.contains("GET /runtime/late-module.js?v=one ") {
        (
            br#"export function mount(){fetch('/api/state.json');const img=document.createElement('img');img.src='/runtime/logo.png?v=one';img.alt='runtime logo';document.body.appendChild(img);const p=document.createElement('p');p.id='runtime-ok';p.textContent='Runtime module mounted';document.body.appendChild(p);}"#
                .to_vec(),
            "text/javascript",
            "200 OK",
        )
    } else if first_line.contains("GET /runtime/logo.png?v=one ") {
        (b"runtime-logo-one".to_vec(), "image/png", "200 OK")
    } else if first_line.contains("GET /runtime/static-pair.png?v=one ") {
        (b"static-one".to_vec(), "image/png", "200 OK")
    } else if first_line.contains("GET /runtime/static-pair.png?v=two ") {
        (b"static-two".to_vec(), "image/png", "200 OK")
    } else if first_line.contains("GET /api/state.json ") {
        (br#"{"live":true}"#.to_vec(), "application/json", "200 OK")
    } else if first_line.contains("GET / ") || first_line.contains("GET /index.html ") {
        (
            br#"<html><body><h1>Runtime Fixture</h1><img src="/runtime/static-pair.png?v=one"><img src="/runtime/static-pair.png?v=two"><script type="module">setTimeout(()=>import('/runtime/late-module.js?v=one').then(m=>m.mount()),20)</script></body></html>"#
                .to_vec(),
            "text/html",
            "200 OK",
        )
    } else {
        (Vec::new(), "text/plain", "404 Not Found")
    }
}

#[cfg(feature = "render")]
fn captured_relative_paths(root: &std::path::Path) -> Vec<String> {
    fn visit(root: &std::path::Path, dir: &std::path::Path, paths: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, paths);
            } else {
                paths.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut paths = Vec::new();
    visit(root, root, &mut paths);
    paths
}
