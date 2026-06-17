//! Shared application state consumed by both TUI backends.
//!
//! [`AppState`] is the single source of truth for live crawl metrics.
//! It is updated by `decant-core` fetch tasks (via [`AppState::update`])
//! and read by the TUI render loop (via [`AppState::snapshot`]).
//!
//! # Thread safety
//!
//! `AppState` is `Clone + Send + Sync` — the inner data is protected by
//! `Arc<Mutex<_>>`. Cloning is cheap (just increments the `Arc` reference count).
//!
//! # Design note
//!
//! Keeping state in a shared `Arc<Mutex>` rather than message-passing channels
//! is intentional: the render loop only reads a snapshot; it never owns the data.
//! This avoids backpressure issues when the crawl is faster than the 100 ms tick.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Instant,
};

/// Maximum number of recent URLs to keep in the display ring-buffer.
const MAX_RECENT: usize = 50;

/// Maximum number of throughput samples to retain for the sparkline.
const MAX_SAMPLES: usize = 30;

// ── Metrics ───────────────────────────────────────────────────────────────────

/// A point-in-time snapshot of crawl metrics.
///
/// Returned by [`AppState::snapshot`]; cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    /// URLs waiting to be fetched.
    pub pending: usize,
    /// URLs currently being fetched.
    pub in_flight: usize,
    /// Successfully fetched URLs.
    pub done: usize,
    /// URLs that produced an error.
    pub errors: usize,
    /// Total bytes written to disk since the crawl started.
    pub bytes_total: u64,
    /// Ring-buffer of `(timestamp, bytes_in_period)` for the sparkline.
    /// Limited to `MAX_SAMPLES` entries; oldest entries are dropped first.
    pub throughput_samples: Vec<(Instant, u64)>,
}

// ── AppState ──────────────────────────────────────────────────────────────────

/// Thread-safe shared crawl state.
///
/// # Examples
///
/// ```
/// use decant_tui::AppState;
///
/// let state = AppState::new();
/// state.update(10, 2, 5, 0, 1024, Some("/index.html".into()));
///
/// let (metrics, urls, finished, _status) = state.snapshot();
/// assert_eq!(metrics.pending, 10);
/// assert_eq!(urls.first().map(String::as_str), Some("/index.html"));
/// assert!(!finished);
/// ```
#[derive(Clone)]
pub struct AppState(Arc<Mutex<Inner>>);

// Implement Debug manually so the Arc indirection is transparent.
impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct Inner {
    metrics: Metrics,
    recent_urls: VecDeque<String>,
    finished: bool,
    status: Option<String>,
}

impl AppState {
    /// Create a new, empty `AppState`.
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Inner {
            metrics: Metrics::default(),
            recent_urls: VecDeque::with_capacity(MAX_RECENT),
            finished: false,
            status: None,
        })))
    }

    /// Push updated metrics from a fetch task.
    ///
    /// - `new_bytes` — bytes received in this update (added to `bytes_total`).
    /// - `url` — the URL just fetched, added to the recent-URL ring-buffer.
    ///
    /// Call this after every successful or failed fetch so the TUI stays live.
    pub fn update(
        &self,
        pending: usize,
        in_flight: usize,
        done: usize,
        errors: usize,
        new_bytes: u64,
        url: Option<String>,
    ) {
        let mut inner = self.lock();
        inner.metrics.pending = pending;
        inner.metrics.in_flight = in_flight;
        inner.metrics.done = done;
        inner.metrics.errors = errors;
        inner.metrics.bytes_total += new_bytes;

        // Rolling throughput window.
        inner
            .metrics
            .throughput_samples
            .push((Instant::now(), new_bytes));
        if inner.metrics.throughput_samples.len() > MAX_SAMPLES {
            inner.metrics.throughput_samples.remove(0);
        }

        // Recent-URL ring-buffer.
        if let Some(u) = url {
            if inner.recent_urls.len() >= MAX_RECENT {
                inner.recent_urls.pop_front();
            }
            inner.recent_urls.push_back(u);
        }
    }

    /// Mark the crawl as finished (stops the TUI loop on next tick).
    pub fn finish(&self) {
        self.lock().finished = true;
    }

    /// Set the one-line status message shown in the footer.
    ///
    /// Pass `""` to clear.
    pub fn set_status(&self, msg: impl Into<String>) {
        let msg = msg.into();
        self.lock().status = if msg.is_empty() { None } else { Some(msg) };
    }

    /// Take a point-in-time snapshot for rendering.
    ///
    /// Returns `(metrics, recent_urls, finished, status)`.
    /// Holding the lock only for the duration of the clone.
    pub fn snapshot(&self) -> (Metrics, Vec<String>, bool, Option<String>) {
        let inner = self.lock();
        (
            inner.metrics.clone(),
            inner.recent_urls.iter().cloned().collect(),
            inner.finished,
            inner.status.clone(),
        )
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0
            .lock()
            .expect("AppState Mutex poisoned — a fetch task panicked")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> AppState {
        AppState::new()
    }

    // ── update ────────────────────────────────────────────────────────────────

    #[test]
    fn update_accumulates_bytes_total() {
        let state = make_state();
        state.update(5, 1, 0, 0, 1024, None);
        state.update(4, 1, 1, 0, 512, None);

        let (metrics, _, _, _) = state.snapshot();
        assert_eq!(metrics.bytes_total, 1536);
    }

    #[test]
    fn update_overwrites_counters_not_accumulates() {
        let state = make_state();
        state.update(10, 2, 0, 0, 0, None);
        state.update(8, 3, 2, 0, 0, None);

        let (metrics, _, _, _) = state.snapshot();
        // Counters are set, not incremented.
        assert_eq!(metrics.pending, 8);
        assert_eq!(metrics.in_flight, 3);
        assert_eq!(metrics.done, 2);
    }

    #[test]
    fn update_appends_url_to_recent_ring() {
        let state = make_state();
        state.update(1, 0, 0, 0, 0, Some("/about".into()));

        let (_, urls, _, _) = state.snapshot();
        assert_eq!(urls.first().map(String::as_str), Some("/about"));
    }

    #[test]
    fn update_ring_buffer_evicts_oldest_when_full() {
        let state = make_state();
        for i in 0..=MAX_RECENT {
            state.update(0, 0, i, 0, 0, Some(format!("/page-{i}")));
        }
        let (_, urls, _, _) = state.snapshot();
        // Buffer must not exceed MAX_RECENT.
        assert_eq!(urls.len(), MAX_RECENT);
        // /page-0 should have been evicted.
        assert!(!urls.contains(&"/page-0".to_string()));
    }

    #[test]
    fn throughput_samples_capped_at_max() {
        let state = make_state();
        for _ in 0..=MAX_SAMPLES + 5 {
            state.update(0, 0, 0, 0, 100, None);
        }
        let (metrics, _, _, _) = state.snapshot();
        assert!(metrics.throughput_samples.len() <= MAX_SAMPLES);
    }

    // ── finish ────────────────────────────────────────────────────────────────

    #[test]
    fn finished_starts_false_and_set_by_finish() {
        let state = make_state();
        let (_, _, finished, _) = state.snapshot();
        assert!(!finished, "should not be finished initially");

        state.finish();
        let (_, _, finished, _) = state.snapshot();
        assert!(finished);
    }

    // ── set_status ────────────────────────────────────────────────────────────

    #[test]
    fn set_status_stores_and_clears_message() {
        let state = make_state();
        state.set_status("writing manifest…");

        let (_, _, _, status) = state.snapshot();
        assert_eq!(status.as_deref(), Some("writing manifest…"));

        state.set_status("");
        let (_, _, _, status) = state.snapshot();
        assert!(status.is_none(), "empty string should clear the status");
    }

    // ── Clone ─────────────────────────────────────────────────────────────────

    #[test]
    fn clone_shares_the_same_underlying_data() {
        let state = make_state();
        let clone = state.clone();

        state.update(0, 0, 42, 0, 0, None);
        let (metrics, _, _, _) = clone.snapshot();
        assert_eq!(metrics.done, 42, "clone must see updates from original");
    }

    // ── Debug ─────────────────────────────────────────────────────────────────

    #[test]
    fn debug_does_not_panic() {
        let state = make_state();
        let _repr = format!("{state:?}");
    }
}
