//! `decant serve <DIR>` — serve a mirrored capture locally for preview.
//!
//! A minimal static file server (no dependencies beyond tokio + stdlib).
//! Binds to `127.0.0.1:<port>` and serves files from the capture directory.

use color_eyre::eyre::{Context as _, Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::args::ServeArgs;
use decant_core::writer::percent_decode;

pub async fn run(args: ServeArgs) -> Result<()> {
    if !args.dir.exists() {
        bail!("directory does not exist: {}", args.dir.display());
    }

    let addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("cannot bind to {addr}"))?;

    println!(
        "decant serve ▶  http://{addr}  (serving {})",
        args.dir.display()
    );
    println!("  Press Ctrl-C to stop.\n");

    let noscript = args.noscript;
    loop {
        let (mut stream, peer) = listener.accept().await?;
        let root = args.dir.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_request(&mut stream, &root, peer.to_string(), noscript).await {
                tracing::warn!("serve error: {e}");
            }
        });
    }
}

async fn handle_request(
    stream: &mut tokio::net::TcpStream,
    root: &std::path::Path,
    peer: String,
    noscript: bool,
) -> Result<()> {
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let request = std::str::from_utf8(&buf[..n]).unwrap_or("");

    // Parse the GET path from HTTP/1.x request line.
    let path = request
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/");

    let path = percent_decode(path.trim_start_matches('/'));
    let mut file_path = root.join(&path);

    // Default to index.html for directory requests.
    if file_path.is_dir() {
        file_path = file_path.join("index.html");
    }

    tracing::debug!("{peer} → {}", file_path.display());

    let (status, mut body, content_type) = if file_path.exists() && file_path.is_file() {
        let body = tokio::fs::read(&file_path).await.unwrap_or_default();
        let ct = mime_guess::from_path(&file_path)
            .first_raw()
            .unwrap_or("application/octet-stream");
        ("200 OK", body, ct.to_string())
    } else {
        (
            "404 Not Found",
            b"<h1>404 Not Found</h1>".to_vec(),
            "text/html".to_string(),
        )
    };

    if noscript && content_type == "text/html" {
        let html = String::from_utf8_lossy(&body);
        body = prepare_noscript_preview(&html).into_bytes();
    }

    let response = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(&body).await?;

    Ok(())
}

/// Strip all script tags (case-insensitive) from an HTML string.
pub fn strip_script_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '<' {
            let peek_next: String = chars.clone().take(7).collect();
            let peek_lower = peek_next.to_lowercase();
            if peek_lower.starts_with("script") {
                let char_after = peek_lower.chars().nth(6);
                if char_after.is_none_or(|ch| ch.is_whitespace() || ch == '>') {
                    // Skip characters until we find the closing "</script>" tag
                    let mut found_close = false;
                    while let Some(ch) = chars.next() {
                        if ch == '<' {
                            let close_chars: String = chars.clone().take(8).collect();
                            if close_chars.to_lowercase().starts_with("/script>") {
                                // Skip the "/script>" characters
                                for _ in 0..8 {
                                    chars.next();
                                }
                                found_close = true;
                                break;
                            }
                        }
                    }
                    if found_close {
                        continue;
                    }
                }
            }
        }
        result.push(c);
    }
    result
}

pub fn prepare_noscript_preview(html: &str) -> String {
    remove_inline_opacity(&strip_script_tags(html))
}

fn remove_inline_opacity(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(style_start) = rest.find("style=\"") {
        let (before_style, after_before) = rest.split_at(style_start);
        result.push_str(before_style);

        let after_prefix = &after_before["style=\"".len()..];
        let Some(style_end) = after_prefix.find('"') else {
            result.push_str(after_before);
            return result;
        };

        let style_value = &after_prefix[..style_end];
        let sanitized = remove_opacity_declarations(style_value);
        if !sanitized.is_empty() {
            result.push_str("style=\"");
            result.push_str(&sanitized);
            result.push('"');
        }
        rest = &after_prefix[style_end + 1..];
    }

    result.push_str(rest);
    result
}

fn remove_opacity_declarations(style_value: &str) -> String {
    style_value
        .split(';')
        .filter_map(|declaration| {
            let trimmed = declaration.trim();
            if trimmed.is_empty() || trimmed.to_ascii_lowercase().starts_with("opacity:") {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_script_tags() {
        let html = "<html><head><script src='a.js'></script></head><body><h1>Hello</h1><script>console.log('hi');</script></body></html>";
        let expected = "<html><head></head><body><h1>Hello</h1></body></html>";
        assert_eq!(strip_script_tags(html), expected);
    }

    #[test]
    fn test_strip_script_tags_case_insensitive_and_attributes() {
        let html = "<div><SCRIPT TYPE=\"module\" async src=\"b.js\">const x = 1;</SCRIPT></div>";
        let expected = "<div></div>";
        assert_eq!(strip_script_tags(html), expected);
    }

    #[test]
    fn prepare_noscript_preview_reveals_webflow_animation_content() {
        let html = "<section style=\"opacity: 0; transform: translate3d(0, 2px, 0);\" data-w-id=\"a\"><h2>Visible copy</h2><script>Webflow.require('ix2')</script></section>";
        let prepared = prepare_noscript_preview(html);

        assert!(prepared.contains("<h2>Visible copy</h2>"));
        assert!(prepared.contains("transform: translate3d(0, 2px, 0)"));
        assert!(!prepared.contains("<script>"));
        assert!(!prepared.contains("opacity:"));
    }
}
