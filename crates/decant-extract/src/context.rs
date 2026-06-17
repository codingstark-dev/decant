//! `context.md` renderer — a token-budgeted LLM primer.
//!
//! Renders a short (~1–2 k token) Markdown summary of the captured site
//! intended to be pasted verbatim into an LLM prompt before diving into
//! raw HTML/CSS files.

use crate::manifest::Manifest;
use crate::tokens::DesignTokens;

/// Render `context.md` content from a finalized manifest and design tokens.
pub fn render_context(manifest: &Manifest, tokens: &DesignTokens) -> String {
    let mut md = String::with_capacity(2048);

    // ── Overview ───────────────────────────────────────────────────────────────
    md.push_str("# decant capture — context.md\n\n");
    md.push_str("> Read this file first. It is a token-budgeted primer.\n");
    md.push_str("> Then consult `design-tokens.json` and `manifest.json` for full detail.\n\n");

    md.push_str("## Overview\n\n");
    md.push_str(&format!("- **Seed URL**: {}\n", manifest.seed));
    if let Some(ts) = manifest.captured_at {
        md.push_str(&format!(
            "- **Captured at**: {}\n",
            ts.format("%Y-%m-%d %H:%M:%S UTC")
        ));
    }
    md.push_str(&format!("- **Render mode**: {}\n", manifest.render_mode));
    md.push_str(&format!("- **Pages captured**: {}\n", manifest.total_pages));
    md.push_str(&format!(
        "- **Assets captured**: {}\n",
        manifest.total_assets
    ));
    md.push_str(&format!(
        "- **Total size**: {:.1} MB\n\n",
        manifest.total_bytes as f64 / 1_048_576.0
    ));

    // ── Pages ─────────────────────────────────────────────────────────────────
    md.push_str("## Pages\n\n");
    for page in &manifest.pages {
        let title = page.title.as_deref().unwrap_or("(no title)");
        md.push_str(&format!("- **{}** → `{}`\n", page.url, page.file));
        md.push_str(&format!("  - Title: {title}\n"));
        if !page.regions.is_empty() {
            md.push_str(&format!("  - Regions: {}\n", page.regions.join(", ")));
        }
    }
    md.push('\n');

    // ── Screenshots ────────────────────────────────────────────────────────────
    let screenshots: Vec<_> = manifest
        .assets
        .iter()
        .filter(|a| a.path.starts_with("screenshots/"))
        .collect();
    if !screenshots.is_empty() {
        md.push_str("## Captured Screenshots\n\n");
        for s in &screenshots {
            md.push_str(&format!("- `{}`\n", s.path));
        }
        md.push('\n');
    }

    // ── Key design tokens ──────────────────────────────────────────────────────
    md.push_str("## Key Design Tokens\n\n");

    // Colors
    if !tokens.colors.swatches.is_empty() {
        md.push_str("### Colors\n\n");
        // Show at most 8 swatches to stay token-lean.
        let swatches: Vec<_> = tokens.colors.swatches.iter().take(8).collect();
        md.push_str(&format!(
            "Palette: `{}`\n\n",
            swatches
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("` `")
        ));
        if !tokens.colors.by_usage.is_empty() {
            md.push_str("Semantic roles:\n");
            for (role, color) in &tokens.colors.by_usage {
                md.push_str(&format!("- `{role}`: `{color}`\n"));
            }
            md.push('\n');
        }
    }

    // Typography
    if !tokens.typography.font_families.is_empty() {
        md.push_str("### Typography\n\n");
        md.push_str(&format!(
            "- **Fonts**: {}\n",
            tokens.typography.font_families.join(", ")
        ));
        if !tokens.typography.scale.is_empty() {
            let scale: Vec<_> = tokens
                .typography
                .scale
                .iter()
                .map(|n| format!("{n}px"))
                .collect();
            md.push_str(&format!("- **Type scale**: {}\n", scale.join(", ")));
        }
        md.push('\n');
    }

    // Spacing
    if !tokens.spacing.is_empty() {
        let scale: Vec<_> = tokens
            .spacing
            .iter()
            .take(10)
            .map(|n| format!("{n}px"))
            .collect();
        md.push_str(&format!("### Spacing\n\n`{}`\n\n", scale.join("` `")));
    }

    // Breakpoints
    if !tokens.breakpoints.is_empty() {
        let bps: Vec<_> = tokens
            .breakpoints
            .iter()
            .map(|n| format!("{n}px"))
            .collect();
        md.push_str(&format!("### Breakpoints\n\n{}\n\n", bps.join(", ")));
    }

    // ── How to use this capture ────────────────────────────────────────────────
    md.push_str("## How to Use This Capture\n\n");
    md.push_str("1. **You are here** — this file orients you without burning context.\n");
    md.push_str(
        "2. **`design-tokens.json`** — full color palette, type scale, spacing, \
                 radii, shadows, breakpoints. Translate into your target stack \
                 (Tailwind config / CSS custom properties / design system tokens).\n",
    );
    md.push_str(
        "3. **`manifest.json`** — page tree and component region breakdown. \
                 Build one component per listed region.\n",
    );
    md.push_str(
        "4. **Raw HTML/CSS files** — open only when you need exact markup or \
                 a specific class name. Reproduce structure and styling approach, \
                 not class names verbatim.\n",
    );
    md.push_str(
        "5. **`screenshots/`** — visual ground truth. Compare your output \
                 against these images at matching breakpoints.\n\n",
    );
    md.push_str(
        "> ⚠️  Mirroring a site's layout/design for reference does not grant \
                 rights to its copyrighted text, images, or branding.\n",
    );

    md
}
