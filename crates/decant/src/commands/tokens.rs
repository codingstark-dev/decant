//! `decant tokens <DIR>` — re-run design-token extraction on an existing capture.

use color_eyre::eyre::{Context as _, Result, bail};

use decant_extract::{css::extract_tokens, tokens::DesignTokens};

use crate::args::TokensArgs;

pub async fn run(args: TokensArgs) -> Result<()> {
    if !args.dir.exists() {
        bail!("directory does not exist: {}", args.dir.display());
    }

    let output_path = args.dir.join("design-tokens.json");
    if output_path.exists() && !args.force {
        bail!("design-tokens.json already exists. Use --force to overwrite.");
    }

    println!("decant tokens ▶  scanning {}", args.dir.display());

    let mut aggregate = DesignTokens {
        schema_version: "1.0".into(),
        ..Default::default()
    };
    let mut css_count = 0usize;

    // Walk the capture directory for .css files.
    let mut dir = tokio::fs::read_dir(&args.dir)
        .await
        .with_context(|| format!("cannot read directory: {}", args.dir.display()))?;

    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("css") {
            let bytes = tokio::fs::read(&path).await?;
            if let Ok(tokens) = extract_tokens(&bytes) {
                aggregate.merge(&tokens);
                css_count += 1;
            }
        }
    }

    let json = serde_json::to_string_pretty(&aggregate)?;
    tokio::fs::write(&output_path, json).await?;

    println!("✓  Scanned {css_count} CSS files → design-tokens.json");

    Ok(())
}
