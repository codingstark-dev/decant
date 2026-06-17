//! `decant` — CLI entry point.
//!
//! # Error handling
//!
//! Uses [`color_eyre`] for rich error reporting in the terminal.
//! `color_eyre::install()` **must** be called before `ratatui::run()` so that
//! the terminal is restored before the eyre panic hook prints its report
//! (ratatui 0.30.1 skill requirement).
//!
//! # Logging
//!
//! Structured logging via `tracing` + `tracing-subscriber`. Set `RUST_LOG`
//! to control verbosity:
//! ```bash
//! RUST_LOG=decant_core=debug decant clone https://example.com
//! ```

mod args;
mod commands;

use args::{Cli, Commands};
use clap::Parser as _;
use color_eyre::eyre::Result;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    // ── 1. Install color-eyre FIRST — before any terminal/ratatui init ────────
    // Per ratatui 0.30.1 skill: eyre hooks must be registered before ratatui
    // init so the terminal is restored before its report prints on panic.
    color_eyre::install()?;

    // ── 2. Structured logging (honours RUST_LOG env var) ─────────────────────
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_target(false)
        .compact()
        .init();

    // ── 3. Parse CLI args and dispatch ───────────────────────────────────────
    let cli = Cli::parse();

    match cli.command {
        Commands::Clone(args) => commands::clone::run(args).await,
        Commands::Tokens(args) => commands::tokens::run(args).await,
        Commands::Serve(args) => commands::serve::run(args).await,
    }
}
