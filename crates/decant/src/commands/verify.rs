use color_eyre::eyre::{Result, bail};
#[cfg(feature = "render")]
use image::{DynamicImage, GenericImageView as _, ImageReader, imageops::FilterType};
#[cfg(feature = "render")]
use num_traits::ToPrimitive as _;
#[cfg(feature = "render")]
use serde::Serialize;
#[cfg(feature = "render")]
use std::path::{Path, PathBuf};
#[cfg(feature = "render")]
use url::Url;

use crate::args::VerifyArgs;

#[cfg(feature = "render")]
#[derive(Debug, Serialize)]
struct VerifyReport {
    status: VerifyStatus,
    threshold: f32,
    comparisons: Vec<ViewportComparison>,
    ai_next_steps: Vec<String>,
}

#[cfg(feature = "render")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum VerifyStatus {
    Match,
    NeedsRepair,
}

#[cfg(feature = "render")]
#[derive(Debug, Serialize)]
struct ViewportComparison {
    viewport: String,
    similarity: f32,
    passed: bool,
    live_screenshot: PathBuf,
    local_screenshot: PathBuf,
    live_size: ImageSize,
    local_size: ImageSize,
    repair_hints: Vec<String>,
}

#[cfg(feature = "render")]
#[derive(Debug, Serialize)]
struct ImageSize {
    width: u32,
    height: u32,
}

pub async fn run(args: VerifyArgs) -> Result<()> {
    #[cfg(not(feature = "render"))]
    {
        let _ = args;
        bail!(
            "decant verify requires the `render` feature. Reinstall with: cargo install decant --features render"
        );
    }

    #[cfg(feature = "render")]
    {
        run_with_render(args).await
    }
}

#[cfg(feature = "render")]
async fn run_with_render(args: VerifyArgs) -> Result<()> {
    validate_threshold(args.threshold)?;

    let live_url = Url::parse(&args.live_url)?;
    let local_url = Url::parse(&args.local_url)?;
    let viewports = parse_viewports(&args.viewports)?;

    tokio::fs::create_dir_all(&args.screenshots_dir).await?;

    let browser = decant_render::Browser::launch(decant_render::BrowserBackend::Chrome).await?;
    let capture_result = async {
        let live_shots = decant_render::capture_viewports(&browser, &live_url, &viewports).await?;
        let local_shots =
            decant_render::capture_viewports(&browser, &local_url, &viewports).await?;

        Ok::<_, color_eyre::Report>((live_shots, local_shots))
    }
    .await;
    browser.shutdown().await;
    let (live_shots, local_shots) = capture_result?;

    ensure_screenshot_counts_match(live_shots.len(), local_shots.len())?;

    let mut comparisons = Vec::new();
    for (live, local) in live_shots.iter().zip(local_shots.iter()) {
        let live_path = args
            .screenshots_dir
            .join(format!("{}-live.png", live.viewport.name));
        let local_path = args
            .screenshots_dir
            .join(format!("{}-local.png", local.viewport.name));

        tokio::fs::write(&live_path, &live.png_bytes).await?;
        tokio::fs::write(&local_path, &local.png_bytes).await?;

        let live_image = load_png(&live_path)?;
        let local_image = load_png(&local_path)?;
        let similarity = image_similarity(&live_image, &local_image);
        let passed = similarity >= args.threshold;

        comparisons.push(ViewportComparison {
            viewport: live.viewport.name.to_string(),
            similarity,
            passed,
            live_screenshot: live_path,
            local_screenshot: local_path,
            live_size: image_size(&live_image),
            local_size: image_size(&local_image),
            repair_hints: repair_hints(similarity, args.threshold),
        });
    }

    let status = if comparisons.iter().all(|c| c.passed) {
        VerifyStatus::Match
    } else {
        VerifyStatus::NeedsRepair
    };

    let report = VerifyReport {
        status,
        threshold: args.threshold,
        comparisons,
        ai_next_steps: ai_next_steps(),
    };

    let json = serde_json::to_string_pretty(&report)?;
    tokio::fs::write(&args.output, json).await?;
    println!("✓ verify report written to {}", args.output.display());

    Ok(())
}

#[cfg(feature = "render")]
fn parse_viewports(names: &[String]) -> Result<Vec<decant_render::Viewport>> {
    let mut viewports = Vec::new();
    for name in names {
        match name.trim().to_lowercase().as_str() {
            "mobile" => viewports.push(decant_render::MOBILE),
            "tablet" | "tab" => viewports.push(decant_render::TABLET),
            "desktop" => viewports.push(decant_render::DESKTOP),
            other => bail!("unknown viewport `{other}`. Supported: mobile, tablet, desktop"),
        }
    }
    Ok(viewports)
}

#[cfg(feature = "render")]
fn validate_threshold(threshold: f32) -> Result<()> {
    if (0.0..=1.0).contains(&threshold) {
        Ok(())
    } else {
        bail!("--threshold must be between 0.0 and 1.0")
    }
}

#[cfg(feature = "render")]
fn ensure_screenshot_counts_match(live_count: usize, local_count: usize) -> Result<()> {
    if live_count == local_count {
        Ok(())
    } else {
        bail!("live/local screenshot count mismatch: live={live_count}, local={local_count}")
    }
}

#[cfg(feature = "render")]
fn load_png(path: &Path) -> Result<DynamicImage> {
    Ok(ImageReader::open(path)?.decode()?)
}

#[cfg(feature = "render")]
fn image_size(image: &DynamicImage) -> ImageSize {
    let (width, height) = image.dimensions();
    ImageSize { width, height }
}

#[cfg(feature = "render")]
fn image_similarity(live: &DynamicImage, local: &DynamicImage) -> f32 {
    let live_rgba = live.to_rgba8();
    let local_rgba = if live.dimensions() == local.dimensions() {
        local.to_rgba8()
    } else {
        local
            .resize_exact(live.width(), live.height(), FilterType::Triangle)
            .to_rgba8()
    };

    let mut total_delta = 0_f64;
    let mut samples = 0_f64;

    for (live_pixel, local_pixel) in live_rgba.pixels().zip(local_rgba.pixels()) {
        for channel in 0..3 {
            let live_value = f64::from(live_pixel.0[channel]);
            let local_value = f64::from(local_pixel.0[channel]);
            total_delta += (live_value - local_value).abs();
            samples += 1.0;
        }
    }

    if samples == 0.0 {
        return 0.0;
    }

    (1.0 - (total_delta / samples / 255.0))
        .to_f32()
        .unwrap_or(0.0)
}

#[cfg(feature = "render")]
fn repair_hints(similarity: f32, threshold: f32) -> Vec<String> {
    if similarity >= threshold {
        return vec!["viewport passed; keep current clone output".to_string()];
    }

    vec![
        "inspect repair-hints.json for failed or blocked assets".to_string(),
        "rerun clone with --render chrome --runtime-capture on and include images,screenshots"
            .to_string(),
        "serve the capture with decant serve --noscript if hydration changes the static DOM"
            .to_string(),
    ]
}

#[cfg(feature = "render")]
fn ai_next_steps() -> Vec<String> {
    vec![
        "If status is match, use the cloned capture as the visual source of truth.".to_string(),
        "If status is needs_repair, compare the live/local PNGs and repair missing assets, layout CSS, or script-stripped preview behavior before accepting the clone.".to_string(),
    ]
}

#[cfg(all(test, feature = "render"))]
mod tests {
    use image::{DynamicImage, Rgba, RgbaImage};

    use super::{
        ensure_screenshot_counts_match, image_similarity, repair_hints, validate_threshold,
    };

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba(rgba)))
    }

    fn assert_similarity(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn image_similarity_is_exact_for_identical_pixels() {
        let live = solid(2, 2, [12, 34, 56, 255]);
        let local = solid(2, 2, [12, 34, 56, 255]);

        assert_similarity(image_similarity(&live, &local), 1.0);
    }

    #[test]
    fn image_similarity_detects_opposite_pixels() {
        let live = solid(2, 2, [0, 0, 0, 255]);
        let local = solid(2, 2, [255, 255, 255, 255]);

        assert_similarity(image_similarity(&live, &local), 0.0);
    }

    #[test]
    fn repair_hints_explain_failed_viewport() {
        let hints = repair_hints(0.5, 0.9);

        assert!(hints.iter().any(|hint| hint.contains("repair-hints.json")));
        assert!(
            hints
                .iter()
                .any(|hint| hint.contains("--runtime-capture on"))
        );
    }

    #[test]
    fn threshold_rejects_out_of_range_values() {
        assert!(validate_threshold(-0.1).is_err());
        assert!(validate_threshold(1.1).is_err());
        assert!(validate_threshold(0.92).is_ok());
    }

    #[test]
    fn screenshot_count_mismatch_is_an_error() {
        assert!(ensure_screenshot_counts_match(2, 1).is_err());
        assert!(ensure_screenshot_counts_match(2, 2).is_ok());
    }
}
