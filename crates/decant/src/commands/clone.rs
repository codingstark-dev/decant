//! `decant clone` — orchestrates the full website mirror pipeline.

#[path = "clone_repair.rs"]
mod repair;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::eyre::{Context as _, Result, bail};
#[cfg(feature = "render")]
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use url::Url;

use decant_core::{
    client::{DEFAULT_USER_AGENT, build_client},
    frontier::{CrawlItem, Frontier, TargetKind},
    rate::HostRateLimiter,
    robots::RobotsCache,
    writer::{url_to_path, write_file},
};
use decant_extract::{
    context::render_context,
    css::extract_tokens,
    extract_and_rewrite_css, extract_js_dependencies,
    html::{detect_regions, extract_and_rewrite},
    manifest::{Asset, ManifestBuilder, PageEntry},
    tokens::DesignTokens,
};
use decant_tui::{
    dashboard::{Dashboard, DashboardConfig},
    progress::ProgressReporter,
    state::AppState,
};

use crate::args::{CloneArgs, RuntimeCaptureMode};
use repair::{RepairHints, is_optional_missing_asset, page_manifest_url, relative_path};

struct CaptureAspects {
    html: bool,
    css: bool,
    js: bool,
    fonts: bool,
    images: bool,
    screenshots: bool,
    tokens: bool,
    context: bool,
}

impl CaptureAspects {
    fn from_vec(aspects: &[String]) -> Self {
        let mut this = Self {
            html: true,
            css: false,
            js: false,
            fonts: false,
            images: false,
            screenshots: false,
            tokens: false,
            context: false,
        };
        for a in aspects {
            match a.trim().to_lowercase().as_str() {
                "html" => this.html = true,
                "css" => this.css = true,
                "js" => this.js = true,
                "fonts" => this.fonts = true,
                "images" => this.images = true,
                "screenshots" => this.screenshots = true,
                "tokens" => this.tokens = true,
                "context" => this.context = true,
                _ => {}
            }
        }
        this
    }
}

pub async fn run(args: CloneArgs) -> Result<()> {
    // ── Parse and validate seed URL ───────────────────────────────────────────
    let seed_url =
        Url::parse(&args.url).with_context(|| format!("invalid seed URL: {}", args.url))?;

    // ── Determine output directory ────────────────────────────────────────────
    let output_dir: PathBuf = args
        .output
        .unwrap_or_else(|| PathBuf::from(seed_url.host_str().unwrap_or("capture")));
    tokio::fs::create_dir_all(&output_dir)
        .await
        .with_context(|| format!("cannot create output dir: {}", output_dir.display()))?;

    println!("decant ▶  {} → {}", seed_url, output_dir.display());
    println!(
        "  depth={} | concurrency={} | rate={} req/s | robots={}",
        args.depth,
        args.concurrency,
        args.rate_limit,
        if args.ignore_robots {
            "ignored"
        } else {
            "respected"
        }
    );

    // ── Parse capture aspects ─────────────────────────────────────────────────
    let aspects = Arc::new(CaptureAspects::from_vec(&args.capture));

    // ── Parse headers and cookies ─────────────────────────────────────────────
    let mut extra_headers = Vec::new();
    for h in &args.headers {
        if let Some((k, v)) = h.split_once(':') {
            extra_headers.push((k.trim().to_string(), v.trim().to_string()));
        } else {
            bail!(
                "invalid header argument `{}`. Must be in KEY:VALUE format",
                h
            );
        }
    }

    let mut cookies = Vec::new();
    if let Some(ref cookies_str) = args.cookies {
        cookies.extend(decant_core::cookie::parse_cookie_str(cookies_str));
    }
    if let Some(ref cookie_file_path) = args.cookie_file {
        let content = tokio::fs::read_to_string(cookie_file_path)
            .await
            .with_context(|| {
                format!("failed to read cookie file: {}", cookie_file_path.display())
            })?;
        cookies.extend(decant_core::cookie::parse_netscape_file(&content));
    }

    if !cookies.is_empty() {
        let cookie_val = cookies
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ");
        extra_headers.push(("cookie".to_string(), cookie_val));
    }

    // ── Resolve render backend and viewports ──────────────────────────────────
    let render_backend = if let Some(ref r) = args.render {
        let b = match r.trim().to_lowercase().as_str() {
            "chrome" => decant_render::BrowserBackend::Chrome,
            "lightpanda" => decant_render::BrowserBackend::Lightpanda,
            other => bail!(
                "Invalid render backend `{}`. Must be `chrome` or `lightpanda`.",
                other
            ),
        };
        Some(b)
    } else {
        None
    };

    let runtime_capture = match (args.runtime_capture, render_backend) {
        (
            RuntimeCaptureMode::On | RuntimeCaptureMode::Auto,
            Some(decant_render::BrowserBackend::Chrome),
        ) => true,
        (RuntimeCaptureMode::On, _) => {
            bail!("--runtime-capture on requires --render chrome")
        }
        (RuntimeCaptureMode::Auto | RuntimeCaptureMode::Off, _) => false,
    };

    println!(
        "  runtime-capture={}",
        if runtime_capture { "on" } else { "off" }
    );

    #[cfg(not(feature = "render"))]
    let browser: Option<Arc<decant_render::Browser>> = {
        if render_backend.is_some() {
            bail!(
                "--render requires the `render` Cargo feature.\n\
                 Reinstall with: cargo install decant --features render"
            );
        }
        None
    };

    #[cfg(feature = "render")]
    let browser = if let Some(backend) = render_backend {
        println!("🚀 Launching headless browser ({:?})...", backend);
        let b = decant_render::Browser::launch(backend)
            .await
            .with_context(|| format!("Failed to launch browser backend {:?}", backend))?;
        Some(Arc::new(b))
    } else {
        None
    };

    #[cfg(feature = "render")]
    let selected_viewports = {
        let mut vps = Vec::new();
        if !args.no_screenshots {
            if args.screenshots.is_empty() {
                vps.push(decant_render::MOBILE);
                vps.push(decant_render::TABLET);
                vps.push(decant_render::DESKTOP);
            } else {
                for name in &args.screenshots {
                    match name.trim().to_lowercase().as_str() {
                        "mobile" => vps.push(decant_render::MOBILE),
                        "tablet" | "tab" | "tabs" => vps.push(decant_render::TABLET),
                        "desktop" => vps.push(decant_render::DESKTOP),
                        other => bail!(
                            "Unknown viewport name `{}`. Supported: mobile, tablet, desktop",
                            other
                        ),
                    }
                }
            }
        }
        vps
    };

    // ── Build shared infrastructure ───────────────────────────────────────────
    let user_agent = args.user_agent.as_deref().unwrap_or(DEFAULT_USER_AGENT);
    let client = Arc::new(build_client(Some(user_agent), &extra_headers)?);
    let frontier = Frontier::new([seed_url.clone()]);
    let rate_limiter = HostRateLimiter::new(args.rate_limit);
    let robots = RobotsCache::new();
    let semaphore = Arc::new(Semaphore::new(args.concurrency));

    // ── Shared state for TUI / progress ──────────────────────────────────────
    let app_state = AppState::new();

    // ── Launch TUI or progress bar ────────────────────────────────────────────
    let use_tui = args.tui.unwrap_or_else(|| atty::is(atty::Stream::Stderr));

    let tui_state = app_state.clone();
    let tokio_handle = tokio::runtime::Handle::current();
    let tui_handle = if use_tui {
        let config = DashboardConfig {
            concurrency: args.concurrency,
            rate_limit: args.rate_limit,
            render_mode: args.render.is_some(),
            robots_active: !args.ignore_robots,
        };
        Some(std::thread::spawn(move || {
            let _guard = tokio_handle.enter();
            Dashboard::new(tui_state, config).run()
        }))
    } else {
        None
    };

    let progress = if !use_tui {
        Some(ProgressReporter::new(app_state.clone()))
    } else {
        None
    };

    // ── Manifest and token aggregation ────────────────────────────────────────
    let manifest_builder = Arc::new(tokio::sync::Mutex::new(ManifestBuilder::new()));
    let design_tokens = Arc::new(tokio::sync::Mutex::new(DesignTokens {
        schema_version: "1.0".into(),
        source: seed_url.to_string(),
        captured_at: Some(chrono::Utc::now()),
        ..Default::default()
    }));

    // ── Main crawl loop ───────────────────────────────────────────────────────
    let mut tasks = Vec::new();
    loop {
        // Check completion before popping.
        if frontier.is_complete() {
            break;
        }

        let Some(item) = frontier.pop() else {
            // Nothing ready right now — wait briefly for in-flight to finish.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            continue;
        };

        let url = item.url.clone();
        let current_depth = item.depth;

        // Depth check only applies to pages. Assets are always fetched.
        if item.kind == TargetKind::Page && current_depth > args.depth as usize {
            frontier.mark_done(&url);
            continue;
        }

        if item.kind == TargetKind::Page
            && args.same_origin
            && url.host_str() != seed_url.host_str()
        {
            frontier.mark_done(&url);
            continue;
        }

        // Clone everything needed for the fetch task.
        let client = Arc::clone(&client);
        let rate_limiter = rate_limiter.clone();
        let robots = robots.clone();
        let frontier = frontier.clone();
        let semaphore = Arc::clone(&semaphore);
        let output_dir = output_dir.clone();
        let app_state = app_state.clone();
        let manifest_b = Arc::clone(&manifest_builder);
        let tokens_agg = Arc::clone(&design_tokens);
        let ignore_robots = args.ignore_robots;
        let no_tokens = args.no_tokens;
        let no_manifest = args.no_manifest;
        let ua = user_agent.to_string();
        let seed = seed_url.clone();
        let allow_doms = args.allow_domains.clone();
        let browser = browser.clone();
        let cookies = cookies.clone();
        #[cfg(feature = "render")]
        let runtime_capture_mode = args.runtime_capture;
        #[cfg(feature = "render")]
        let capture_runtime_resources = runtime_capture;
        #[cfg(feature = "render")]
        let selected_viewports = selected_viewports.clone();
        let aspects = Arc::clone(&aspects);

        tasks.push(tokio::spawn(async move {
            let Ok(_permit) = semaphore.acquire().await else {
                frontier.mark_error(&url, "semaphore closed".to_string());
                return;
            };

            // robots.txt check.
            if !ignore_robots {
                match robots.is_allowed(&client, &url, &ua).await {
                    Ok(false) => {
                        tracing::info!("robots.txt disallows {url}");
                        frontier.mark_done(&url);
                        return;
                    }
                    Err(e) => tracing::warn!("robots.txt error for {url}: {e}"),
                    _ => {}
                }
            }

            // Rate limit.
            let host = url.host_str().unwrap_or("").to_string();
            if let Err(e) = rate_limiter.acquire(&host).await {
                tracing::error!("rate limiter error: {e}");
                frontier.mark_error(&url, e.to_string());
                return;
            }

            // Fetch / Render.
            let _ = &browser;
            let _ = &cookies;
            #[cfg(feature = "render")]
            let mut is_rendered = false;
            #[cfg(not(feature = "render"))]
            let is_rendered = false;
            let mut bytes = Vec::new();
            let mut content_type = String::new();
            #[cfg(feature = "render")]
            let mut observed_runtime_assets = Vec::new();

            #[cfg(feature = "render")]
            if let Some(ref b) = browser {
                if item.kind == TargetKind::Page {
                    tracing::debug!("Rendering page via browser: {url}");
                    match decant_render::render_page(
                        b,
                        &url,
                        &cookies,
                        1000,
                        capture_runtime_resources,
                    )
                    .await
                    {
                        Ok(rendered) => {
                            if capture_runtime_resources {
                                for resource in rendered.observed_resources {
                                    match Url::parse(&resource.url) {
                                        Ok(asset_url) => observed_runtime_assets.push(asset_url),
                                        Err(e) => tracing::debug!(
                                            "Skipping invalid observed runtime URL `{}`: {e}",
                                            resource.url
                                        ),
                                    }
                                }
                            }
                            if runtime_capture_mode == RuntimeCaptureMode::On
                                && !rendered.diagnostics.is_empty()
                            {
                                let message = rendered.diagnostics.join("; ");
                                tracing::error!(
                                    "Runtime capture required for {url}, but setup failed: {message}"
                                );
                                frontier.mark_error(&url, message);
                                return;
                            }
                            for diagnostic in rendered.diagnostics {
                                tracing::debug!(
                                    "Runtime capture diagnostic for {url}: {diagnostic}"
                                );
                            }
                            bytes = rendered.html.into_bytes();
                            content_type = "text/html; charset=utf-8".to_string();
                            is_rendered = true;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Browser render failed for {url}, falling back to fetch: {e}"
                            );
                        }
                    }
                }
            }

            if !is_rendered {
                let response = match decant_core::client::fetch(&client, &url).await {
                    Ok(r) => r,
                    Err(e) => {
                        if is_optional_missing_asset(&url) {
                            tracing::debug!("optional asset missing, skipping {url}: {e}");
                            frontier.mark_done(&url);
                            return;
                        }
                        tracing::error!("fetch error {url}: {e}");
                        frontier.mark_error(&url, e.to_string());
                        return;
                    }
                };

                content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                let resp_bytes = match response.bytes().await {
                    Ok(b) => b.to_vec(),
                    Err(e) => {
                        frontier.mark_error(&url, e.to_string());
                        return;
                    }
                };
                bytes = resp_bytes;
            }

            let is_html = content_type.contains("html") || item.kind == TargetKind::Page;
            let path_lower = url.path().to_ascii_lowercase();
            let is_css = content_type.contains("css") || path_lower.ends_with(".css");
            let is_js = content_type.contains("javascript")
                || content_type.contains("ecmascript")
                || path_lower.ends_with(".js")
                || path_lower.ends_with(".mjs");
            let is_font = content_type.contains("font")
                || path_lower.ends_with(".woff2")
                || path_lower.ends_with(".ttf")
                || path_lower.ends_with(".otf")
                || path_lower.ends_with(".woff");
            let is_image = content_type.contains("image")
                || path_lower.ends_with(".png")
                || path_lower.ends_with(".jpg")
                || path_lower.ends_with(".jpeg")
                || path_lower.ends_with(".gif")
                || path_lower.ends_with(".svg")
                || path_lower.ends_with(".webp")
                || path_lower.ends_with(".avif");

            // Filter by capture aspects.
            let should_save = if is_html {
                aspects.html
            } else if is_css {
                aspects.css
            } else if is_js {
                aspects.js
            } else if is_font {
                aspects.fonts
            } else if is_image {
                aspects.images
            } else {
                true
            };

            if !should_save {
                frontier.mark_done(&url);
                return;
            }

            let mut bytes_to_write = bytes.clone();

            if is_html {
                let current_url = url.clone();
                let map_fn = move |target_url: &Url| -> Option<String> {
                    let target_path = url_to_path(Path::new(""), target_url);
                    let page_path = url_to_path(Path::new(""), &current_url);
                    let page_dir = page_path.parent().unwrap_or(Path::new(""));
                    let rel_path = relative_path(page_dir, &target_path);
                    Some(rel_path.display().to_string())
                };

                if let Ok((links, rewritten_html)) = extract_and_rewrite(&bytes, &url, map_fn) {
                    bytes_to_write = rewritten_html.into_bytes();

                    // Enqueue new page links.
                    for link in &links.page_links {
                        let in_scope = link.host_str() == seed.host_str()
                            || allow_doms.iter().any(|d| link.host_str() == Some(d));
                        if in_scope {
                            frontier.enqueue(CrawlItem {
                                url: link.clone(),
                                kind: TargetKind::Page,
                                depth: current_depth + 1,
                            });
                        }
                    }
                    // Enqueue asset links.
                    for asset in &links.asset_links {
                        frontier.enqueue(CrawlItem {
                            url: asset.clone(),
                            kind: TargetKind::Asset,
                            depth: 0,
                        });
                    }
                    #[cfg(feature = "render")]
                    if capture_runtime_resources {
                        for asset in &observed_runtime_assets {
                            frontier.enqueue(CrawlItem {
                                url: asset.clone(),
                                kind: TargetKind::Asset,
                                depth: 0,
                            });
                        }
                    }
                }

                // Capture screenshots if needed.
                #[cfg(feature = "render")]
                if let Some(ref b) = browser {
                    if aspects.screenshots
                        && b.backend().supports_screenshots()
                        && !selected_viewports.is_empty()
                    {
                        tracing::info!("Capturing screenshots for {url}...");
                        match decant_render::capture_viewports(b, &url, &selected_viewports).await {
                            Ok(screenshots) => {
                                let slug = url.path().trim_matches('/').replace('/', "_");
                                let slug = if slug.is_empty() {
                                    "index".to_string()
                                } else {
                                    slug
                                };

                                for s in screenshots {
                                    let rel_path =
                                        format!("screenshots/{}/{}.png", slug, s.viewport.name);
                                    let full_path = output_dir.join(&rel_path);
                                    if let Some(parent) = full_path.parent() {
                                        let _ = tokio::fs::create_dir_all(parent).await;
                                    }
                                    if tokio::fs::write(&full_path, &s.png_bytes).await.is_ok() {
                                        let mut hasher = Sha256::new();
                                        hasher.update(&s.png_bytes);
                                        let hash = hex::encode(hasher.finalize());

                                        if !no_manifest {
                                            manifest_b.lock().await.add_asset(Asset {
                                                path: rel_path,
                                                mime_type: "image/png".to_string(),
                                                hash,
                                                bytes: s.png_bytes.len() as u64,
                                            });
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Screenshot capture failed for {url}: {e}");
                            }
                        }
                    }
                }

                if !no_manifest {
                    let local_path = url_to_path(&output_dir, &url);
                    let rel_path = local_path
                        .strip_prefix(&output_dir)
                        .unwrap_or(&local_path)
                        .display()
                        .to_string();
                    let regions = detect_regions(&bytes);
                    let title = decant_extract::html::extract_title(&bytes);
                    let page = PageEntry {
                        url: page_manifest_url(&url),
                        file: rel_path,
                        title,
                        description: None,
                        regions,
                        asset_refs: vec![],
                    };
                    manifest_b.lock().await.add_page(page);
                }
            }

            if is_css {
                let current_url = url.clone();
                let map_fn = move |target_url: &Url| -> Option<String> {
                    let target_path = url_to_path(Path::new(""), target_url);
                    let page_path = url_to_path(Path::new(""), &current_url);
                    let page_dir = page_path.parent().unwrap_or(Path::new(""));
                    let rel_path = relative_path(page_dir, &target_path);
                    Some(rel_path.display().to_string())
                };

                if let Ok((asset_urls, rewritten_css)) =
                    extract_and_rewrite_css(&bytes, &url, map_fn)
                {
                    bytes_to_write = rewritten_css.into_bytes();

                    // Enqueue the discovered asset links.
                    for asset in &asset_urls {
                        frontier.enqueue(CrawlItem {
                            url: asset.clone(),
                            kind: TargetKind::Asset,
                            depth: 0,
                        });
                    }
                }
            }

            if is_js {
                let js_urls = extract_js_dependencies(&bytes, &url);
                for chunk in js_urls {
                    frontier.enqueue(CrawlItem {
                        url: chunk,
                        kind: TargetKind::Asset,
                        depth: 0,
                    });
                }
            }

            let byte_count = bytes_to_write.len() as u64;
            let local_path = url_to_path(&output_dir, &url);

            // Write file to disk.
            let hash = match write_file(&local_path, &bytes_to_write).await {
                Ok(h) => h,
                Err(e) => {
                    tracing::error!("write error {}: {e}", local_path.display());
                    frontier.mark_error(&url, e.to_string());
                    return;
                }
            };

            // Extract design tokens from CSS.
            if is_css && aspects.tokens && !no_tokens {
                if let Ok(page_tokens) = extract_tokens(&bytes) {
                    tokens_agg.lock().await.merge(&page_tokens);
                }
            }

            // Record asset in manifest.
            if !no_manifest {
                let rel_path = local_path
                    .strip_prefix(&output_dir)
                    .unwrap_or(&local_path)
                    .display()
                    .to_string();
                let asset = Asset {
                    path: rel_path,
                    mime_type: content_type.clone(),
                    hash,
                    bytes: byte_count,
                };
                manifest_b.lock().await.add_asset(asset);
            }

            // Update TUI state.
            let counts = frontier.counts();
            app_state.update(
                counts.pending,
                counts.in_flight,
                counts.done,
                counts.errors,
                byte_count,
                Some(url.path().to_string()),
            );

            frontier.mark_done(&url);
        }));

        // Tick progress bar if not using TUI.
        if let Some(ref p) = progress {
            p.tick();
        }
    }

    // Wait for all in-flight tasks to drain.
    while !frontier.is_complete() {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Some(ref p) = progress {
            p.tick();
        }
    }

    for task in tasks {
        if let Err(e) = task.await {
            tracing::warn!("clone task failed to join: {e}");
        }
    }

    // Shutdown browser.
    #[cfg(feature = "render")]
    if let Some(b) = browser {
        if let Ok(unwrapped) = Arc::try_unwrap(b) {
            unwrapped.shutdown().await;
        }
    }

    app_state.set_status("writing outputs…");

    // ── Write design-tokens.json ──────────────────────────────────────────────
    if aspects.tokens && !args.no_tokens {
        let tokens = design_tokens.lock().await.clone();
        let path = output_dir.join("design-tokens.json");
        let json = serde_json::to_string_pretty(&tokens)?;
        tokio::fs::write(&path, json).await?;
        println!("✓  design-tokens.json written");
    }

    // ── Write manifest.json ───────────────────────────────────────────────────
    if !args.no_manifest {
        let builder = {
            let mut guard = manifest_builder.lock().await;
            std::mem::take(&mut *guard)
        };
        let render_mode = if args.render.is_some() {
            "rendered"
        } else {
            "static"
        };
        let manifest = builder.build(&seed_url.to_string(), render_mode);
        let path = output_dir.join("manifest.json");
        let json = serde_json::to_string_pretty(&manifest)?;
        tokio::fs::write(&path, json).await?;
        println!("✓  manifest.json written");

        // Write context.md.
        if aspects.context {
            let tokens = design_tokens.lock().await.clone();
            let context = render_context(&manifest, &tokens);
            tokio::fs::write(output_dir.join("context.md"), context).await?;
            println!("✓  context.md written");
        }
    }

    let counts = frontier.counts();
    let repair_hints = RepairHints::new(&seed_url, counts.done, &frontier.errors());
    let repair_hints_path = output_dir.join("repair-hints.json");
    let repair_hints_json = serde_json::to_string_pretty(&repair_hints)?;
    tokio::fs::write(&repair_hints_path, repair_hints_json).await?;
    println!("✓  repair-hints.json written");

    app_state.finish();

    // Rejoin TUI thread.
    if let Some(handle) = tui_handle {
        handle.join().ok();
    }

    println!(
        "\n✓  Done. {} pages/assets captured, {} errors.",
        counts.done, counts.errors
    );

    Ok(())
}

#[cfg(test)]
#[path = "clone_tests.rs"]
mod tests;
