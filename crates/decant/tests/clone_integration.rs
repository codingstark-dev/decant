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
