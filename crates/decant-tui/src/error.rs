//! Error types for `decant-tui`.

use thiserror::Error;

/// Errors that can arise in the TUI layer.
#[derive(Debug, Error)]
pub enum TuiError {
    /// A crossterm I/O error.
    #[error("terminal I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The TUI event loop was interrupted unexpectedly.
    #[error("TUI loop interrupted: {0}")]
    Interrupted(String),
}
