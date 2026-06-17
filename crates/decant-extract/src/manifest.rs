//! Manifest builder — page tree, component regions, and asset catalog.
//!
//! After the crawl completes, `decant` calls [`ManifestBuilder`] to
//! assemble the final `manifest.json` from data accumulated during the crawl.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single captured page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageEntry {
    /// Relative URL path (e.g. `/`, `/about`).
    pub url: String,
    /// Local file path relative to the output directory (e.g. `index.html`).
    pub file: String,
    /// `<title>` text, if any.
    pub title: Option<String>,
    /// `<meta name="description">` content, if any.
    pub description: Option<String>,
    /// Heuristically detected UI regions (header, nav, hero, …).
    pub regions: Vec<String>,
    /// Relative paths of assets referenced by this page.
    pub asset_refs: Vec<String>,
}

/// A single captured asset (image, font, CSS, JS, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    /// Relative path inside the output directory.
    pub path: String,
    /// MIME type (guessed from extension).
    pub mime_type: String,
    /// `sha256:<hex>` content hash.
    pub hash: String,
    /// Size in bytes.
    pub bytes: u64,
}

/// The top-level manifest written to `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Schema version string (e.g. `"1.0"`).
    pub schema_version: String,
    /// Seed URL used to start the crawl.
    pub seed: String,
    /// Timestamp when the crawl completed.
    pub captured_at: Option<DateTime<Utc>>,
    /// `"static"` or `"rendered"`.
    pub render_mode: String,
    /// All pages captured during the crawl.
    pub pages: Vec<PageEntry>,
    /// All assets captured during the crawl.
    pub assets: Vec<Asset>,
    /// Union of all detected component regions across all pages.
    pub component_regions: Vec<String>,
    /// Total number of pages captured.
    pub total_pages: usize,
    /// Total number of assets captured.
    pub total_assets: usize,
    /// Combined byte size of all assets.
    pub total_bytes: u64,
}

/// Incrementally builds a [`Manifest`] during a crawl.
#[derive(Debug, Default)]
pub struct ManifestBuilder {
    /// Pages recorded so far.
    pub pages: Vec<PageEntry>,
    /// Assets recorded so far.
    pub assets: Vec<Asset>,
}

impl ManifestBuilder {
    /// Create a new, empty [`ManifestBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a fetched page.
    pub fn add_page(&mut self, page: PageEntry) {
        self.pages.push(page);
    }

    /// Record a fetched asset.
    pub fn add_asset(&mut self, asset: Asset) {
        // Deduplicate by path.
        if !self.assets.iter().any(|a| a.path == asset.path) {
            self.assets.push(asset);
        }
    }

    /// Finalize into a [`Manifest`].
    pub fn build(self, seed: &str, render_mode: &str) -> Manifest {
        let total_bytes = self.assets.iter().map(|a| a.bytes).sum();
        let total_pages = self.pages.len();
        let total_assets = self.assets.len();

        // Collect all unique regions across pages.
        let mut component_regions: Vec<String> = Vec::new();
        for page in &self.pages {
            for region in &page.regions {
                if !component_regions.contains(region) {
                    component_regions.push(region.clone());
                }
            }
        }

        Manifest {
            schema_version: "1.0".into(),
            seed: seed.to_string(),
            captured_at: Some(Utc::now()),
            render_mode: render_mode.to_string(),
            pages: self.pages,
            assets: self.assets,
            component_regions,
            total_pages,
            total_assets,
            total_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_builder_deduplicates_assets() {
        let mut builder = ManifestBuilder::new();
        let asset = Asset {
            path: "assets/style.css".into(),
            mime_type: "text/css".into(),
            hash: "sha256:abc".into(),
            bytes: 1024,
        };
        builder.add_asset(asset.clone());
        builder.add_asset(asset);
        let manifest = builder.build("https://example.com/", "static");
        assert_eq!(manifest.total_assets, 1);
    }

    #[test]
    fn manifest_builder_aggregates_regions() {
        let mut builder = ManifestBuilder::new();
        builder.add_page(PageEntry {
            url: "/".into(),
            file: "index.html".into(),
            title: None,
            description: None,
            regions: vec!["header".into(), "nav".into()],
            asset_refs: vec![],
        });
        builder.add_page(PageEntry {
            url: "/about".into(),
            file: "about/index.html".into(),
            title: None,
            description: None,
            regions: vec!["header".into(), "footer".into()],
            asset_refs: vec![],
        });
        let manifest = builder.build("https://example.com/", "static");
        assert!(manifest.component_regions.contains(&"header".to_string()));
        assert!(manifest.component_regions.contains(&"nav".to_string()));
        assert!(manifest.component_regions.contains(&"footer".to_string()));
        // header appears in both pages but should only appear once in component_regions
        assert_eq!(
            manifest
                .component_regions
                .iter()
                .filter(|r| r.as_str() == "header")
                .count(),
            1
        );
    }

    #[test]
    fn manifest_totals_are_correct() {
        let mut builder = ManifestBuilder::new();
        builder.add_page(PageEntry {
            url: "/".into(),
            file: "index.html".into(),
            title: Some("Home".into()),
            description: None,
            regions: vec![],
            asset_refs: vec![],
        });
        builder.add_asset(Asset {
            path: "a.css".into(),
            mime_type: "text/css".into(),
            hash: "sha256:1".into(),
            bytes: 500,
        });
        builder.add_asset(Asset {
            path: "b.js".into(),
            mime_type: "application/js".into(),
            hash: "sha256:2".into(),
            bytes: 1500,
        });
        let manifest = builder.build("https://example.com/", "static");
        assert_eq!(manifest.total_pages, 1);
        assert_eq!(manifest.total_assets, 2);
        assert_eq!(manifest.total_bytes, 2000);
    }
}
