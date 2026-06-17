//! `decant-core` — fetch engine, URL frontier, rate limiter, robots.txt enforcement, and file writer.
//!
//! This is the heart of `decant`. All network I/O runs through this crate.
//! Library crates (`decant-extract`, `decant-render`) are pure-transform;
//! only `decant-core` touches the network or disk.

pub mod client;
pub mod cookie;
pub mod error;
pub mod frontier;
pub mod rate;
pub mod robots;
pub mod writer;

pub use cookie::Cookie;
pub use error::CoreError;
