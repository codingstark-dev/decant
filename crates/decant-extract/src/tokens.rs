//! Design token types and JSON serialization.
//!
//! [`DesignTokens`] is the schema for `design-tokens.json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The top-level design token file written to `design-tokens.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesignTokens {
    /// Schema version string (e.g. `"1.0"`)
    pub schema_version: String,
    /// The seed URL of the site these tokens were extracted from.
    pub source: String,
    /// Timestamp when the tokens were captured.
    pub captured_at: Option<DateTime<Utc>>,
    /// Color tokens extracted from stylesheets.
    pub colors: ColorTokens,
    /// Typography tokens extracted from stylesheets.
    pub typography: TypographyTokens,
    /// Spacing scale in px (deduplicated, sorted ascending).
    pub spacing: Vec<f32>,
    /// Breakpoints in px (deduplicated, sorted ascending).
    pub breakpoints: Vec<u32>,
    /// Border-radius values in px / keyword.
    pub radii: Vec<String>,
    /// Box-shadow declarations (raw CSS, deduplicated).
    pub shadows: Vec<String>,
    /// z-index values found in stylesheets.
    pub z_indices: Vec<i32>,
}

/// Color-related tokens.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ColorTokens {
    /// All unique color values found, sorted by frequency (most common first).
    pub swatches: Vec<String>,
    /// Semantic role → color value mapping (heuristically assigned).
    pub by_usage: HashMap<String, String>,
}

/// Typography-related tokens.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TypographyTokens {
    /// Font-family stacks found across all stylesheets.
    pub font_families: Vec<String>,
    /// Font-size scale in px (deduplicated, sorted ascending).
    pub scale: Vec<f32>,
    /// Line-height values (ratio or px, deduplicated).
    pub line_heights: Vec<String>,
    /// Font-weight values found.
    pub font_weights: Vec<u32>,
}

impl DesignTokens {
    /// Merge another set of tokens into `self` (used when aggregating across stylesheets).
    pub fn merge(&mut self, other: &DesignTokens) {
        // Colors: extend swatches, deduplicate.
        for swatch in &other.colors.swatches {
            if !self.colors.swatches.contains(swatch) {
                self.colors.swatches.push(swatch.clone());
            }
        }
        // Typography: extend font families.
        for ff in &other.typography.font_families {
            if !self.typography.font_families.contains(ff) {
                self.typography.font_families.push(ff.clone());
            }
        }
        // Spacing.
        for s in &other.spacing {
            if !self.spacing.contains(s) {
                self.spacing.push(*s);
            }
        }
        self.spacing.sort_by(f32::total_cmp);

        // Breakpoints.
        for b in &other.breakpoints {
            if !self.breakpoints.contains(b) {
                self.breakpoints.push(*b);
            }
        }
        self.breakpoints.sort();

        // Radii.
        for r in &other.radii {
            if !self.radii.contains(r) {
                self.radii.push(r.clone());
            }
        }

        // Shadows.
        for s in &other.shadows {
            if !self.shadows.contains(s) {
                self.shadows.push(s.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_deduplicates_swatches() {
        let mut base = DesignTokens::default();
        base.colors.swatches = vec!["#fff".into(), "#000".into()];

        let mut other = DesignTokens::default();
        other.colors.swatches = vec!["#000".into(), "#red".into()];

        base.merge(&other);
        assert_eq!(base.colors.swatches, vec!["#fff", "#000", "#red"]);
    }

    #[test]
    fn merge_sorts_spacing() {
        let mut base = DesignTokens {
            spacing: vec![16.0, 8.0],
            ..DesignTokens::default()
        };
        let other = DesignTokens {
            spacing: vec![4.0, 16.0, 32.0],
            ..DesignTokens::default()
        };
        base.merge(&other);
        // should be sorted and deduplicated
        assert_eq!(base.spacing, vec![4.0, 8.0, 16.0, 32.0]);
    }
}
