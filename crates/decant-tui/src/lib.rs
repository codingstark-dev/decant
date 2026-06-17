//! `decant-tui` — ratatui 0.30 live dashboard and indicatif fallback.
//!
//! # Architecture
//!
//! Follows the Elm Architecture (TEA):
//! ```text
//! AppState (Model) → Message → update() → view()
//! ```
//!
//! - **TTY present** → [`dashboard`] runs a full ratatui 0.30 layout with
//!   async [`crossterm::event::EventStream`].
//! - **Non-TTY** (CI, piped) → [`progress`] renders indicatif bars.
//!
//! The [`AppState`] is the single source of truth, updated by `decant-core`
//! fetch tasks and read by both backends.
#![deny(missing_docs)]

pub mod dashboard;
pub mod error;
pub mod progress;
pub mod state;

pub use error::TuiError;
pub use state::AppState;
