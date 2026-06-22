//! CLI argument definitions — all `clap` derive structs live here.

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// decant — mirror a website and extract its design system for AI agents.
#[derive(Parser, Debug)]
#[command(
    name = "decant",
    version,
    author,
    about = "Mirror a website's HTML/CSS/JS/assets and extract a machine-readable \
             design system (design-tokens.json, manifest.json, context.md) \
             so AI agents can faithfully reproduce the UI.",
    long_about = None,
    after_help = r#"EXAMPLES:
  # Clone a website statically (fastest)
  decant clone https://example.com --output ./example

  # Clone with headless-browser rendering (Chrome)
  decant clone https://example.com --render chrome

  # Serve a cloned site for local preview
  decant serve ./example --port 8080

  # Serve with script-stripping (prevents client-side hydration crashes)
  decant serve ./example --noscript

  # Extract design tokens from an existing directory
  decant tokens ./example"#,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Clone (mirror) a website to a local directory.
    Clone(CloneArgs),
    /// Re-run design-token extraction on an existing capture directory.
    Tokens(TokensArgs),
    /// Serve a captured site locally for preview.
    Serve(ServeArgs),
    /// Compare live and local screenshots and write AI repair guidance.
    Verify(VerifyArgs),
}

/// Arguments for `decant clone <URL>`.
#[derive(Args, Debug, Clone)]
pub struct CloneArgs {
    /// Seed URL to start cloning from.
    pub url: String,

    /// Output directory (default: `./<hostname>`).
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Maximum link-follow depth from the seed (0 = single page only).
    #[arg(short, long, default_value_t = 0)]
    pub depth: u32,

    /// Enable headless-browser rendering ("chrome" or "lightpanda").
    /// Requires the `render` Cargo feature: `cargo install decant --features render`.
    #[arg(long)]
    pub render: Option<String>,

    /// Capture browser-observed runtime resources: auto, on, or off.
    /// `auto` enables this only for `--render chrome`.
    #[arg(long, value_enum, default_value_t = RuntimeCaptureMode::Auto)]
    pub runtime_capture: RuntimeCaptureMode,

    /// Inline cookie string: "name=val; name2=val2".
    #[arg(long)]
    pub cookies: Option<String>,

    /// Path to a Netscape cookie jar file.
    #[arg(long)]
    pub cookie_file: Option<PathBuf>,

    /// Extra HTTP header in "KEY:VALUE" format. Can be specified multiple times.
    #[arg(long = "header", number_of_values = 1)]
    pub headers: Vec<String>,

    /// Restrict crawl to the seed's origin only (default: on).
    #[arg(long, default_value_t = true)]
    pub same_origin: bool,

    /// Comma-separated additional domains to follow (e.g. CDN/font hosts).
    #[arg(long, value_delimiter = ',')]
    pub allow_domains: Vec<String>,

    /// Maximum number of parallel HTTP requests.
    #[arg(long, default_value_t = 16)]
    pub concurrency: usize,

    /// Per-host request rate limit in requests per second.
    #[arg(long, default_value_t = 4)]
    pub rate_limit: u32,

    /// Viewports for screenshots (comma-separated: mobile, tablet, desktop).
    /// Default: mobile,tablet,desktop (when rendering via chrome).
    #[arg(long, value_delimiter = ',')]
    pub screenshots: Vec<String>,

    /// Disable screenshot capture even if render mode is chrome.
    #[arg(long, default_value_t = false)]
    pub no_screenshots: bool,

    /// Comma-separated list of aspects to capture: html, css, js, fonts, images, screenshots, tokens, context.
    /// Default: html,css,js,fonts,images,tokens,context
    #[arg(
        long,
        default_value = "html,css,js,fonts,images,tokens,context",
        value_delimiter = ','
    )]
    pub capture: Vec<String>,

    /// Skip design-token extraction (no design-tokens.json).
    #[arg(long, default_value_t = false)]
    pub no_tokens: bool,

    /// Skip manifest.json / context.md generation.
    #[arg(long, default_value_t = false)]
    pub no_manifest: bool,

    /// Honor robots.txt (default: on).
    #[arg(long, default_value_t = true)]
    pub respect_robots: bool,

    /// Explicitly disable robots.txt enforcement.
    #[arg(long, default_value_t = false, conflicts_with = "respect_robots")]
    pub ignore_robots: bool,

    /// Force TUI on (true) or off (false). Default: auto-detect TTY.
    #[arg(long)]
    pub tui: Option<bool>,

    /// Override the default identifying User-Agent string.
    #[arg(long)]
    pub user_agent: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum RuntimeCaptureMode {
    Auto,
    On,
    Off,
}

/// Arguments for `decant verify <LIVE_URL> <LOCAL_URL>`.
#[derive(Args, Debug, Clone)]
pub struct VerifyArgs {
    /// Original live URL to compare against.
    pub live_url: String,

    /// Local preview URL, usually from `decant serve`.
    pub local_url: String,

    /// JSON report path.
    #[arg(short, long, default_value = "verify-report.json")]
    pub output: PathBuf,

    /// Directory where live/local PNG screenshots are written.
    #[arg(long, default_value = "verify-screenshots")]
    pub screenshots_dir: PathBuf,

    /// Viewports to compare (comma-separated: mobile, tablet, desktop).
    #[arg(long, value_delimiter = ',', default_value = "desktop")]
    pub viewports: Vec<String>,

    /// Minimum similarity score from 0.0 to 1.0.
    #[arg(long, default_value_t = 0.92)]
    pub threshold: f32,
}

/// Arguments for `decant tokens <DIR>`.
#[derive(Args, Debug, Clone)]
pub struct TokensArgs {
    /// Path to an existing decant capture directory.
    pub dir: PathBuf,

    /// Overwrite existing design-tokens.json if present.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

/// Arguments for `decant serve <DIR>`.
#[derive(Args, Debug, Clone)]
pub struct ServeArgs {
    /// Path to an existing decant capture directory.
    pub dir: PathBuf,

    /// Port to listen on.
    #[arg(short, long, default_value_t = 8080)]
    pub port: u16,

    /// Address to bind to.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Strip all script tags from HTML files on-the-fly to prevent client-side hydration crashes.
    #[arg(long, default_value_t = false)]
    pub noscript: bool,
}
