//! URL frontier — tracks pending, in-flight, done, and errored URLs.
//!
//! The frontier is the single source of truth for crawl state.
//! All access is protected by an `Arc<Mutex<_>>` so it is safe to share
//! across tokio tasks.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use url::Url;

/// The kind of crawl target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TargetKind {
    /// A web page (follows depth restrictions and can contain links).
    Page,
    /// A static asset (e.g. CSS, JS, image) which is always fetched.
    Asset,
}

/// A target item to crawl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlItem {
    /// The URL to fetch.
    pub url: Url,
    /// Whether it's a page or an asset.
    pub kind: TargetKind,
    /// The hop depth of this target from the seed URL.
    pub depth: usize,
}

/// The state a URL can be in during a crawl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlStatus {
    /// The URL is queued and waiting to be fetched.
    Pending,
    /// The URL is currently being fetched by a worker task.
    InFlight,
    /// The URL was fetched successfully.
    Done,
    /// The URL fetch failed; contains the error description.
    Error(String),
}

/// Thread-safe URL frontier shared across all fetch tasks.
#[derive(Debug, Clone)]
pub struct Frontier(Arc<Mutex<Inner>>);

#[derive(Debug, Default)]
struct Inner {
    /// FIFO queue of items waiting to be fetched.
    pending: VecDeque<CrawlItem>,
    /// URLs currently being fetched.
    in_flight: HashSet<String>,
    /// Successfully fetched URLs.
    done: HashSet<String>,
    /// URLs that errored, with the error message.
    errors: Vec<(String, String)>,
    /// All seen URLs (to avoid re-queuing).
    seen: HashSet<String>,
}

impl Frontier {
    /// Create a new frontier seeded with one or more URLs.
    pub fn new(seeds: impl IntoIterator<Item = Url>) -> Self {
        let mut inner = Inner::default();
        for url in seeds {
            let key = normalize_key(&url);
            if inner.seen.insert(key) {
                inner.pending.push_back(CrawlItem {
                    url,
                    kind: TargetKind::Page,
                    depth: 0,
                });
            }
        }
        Self(Arc::new(Mutex::new(inner)))
    }

    /// Enqueue a crawl item if the URL has not been seen before.
    /// Returns `true` if the item was newly added.
    pub fn enqueue(&self, item: CrawlItem) -> bool {
        let mut inner = self.0.lock().expect("frontier lock poisoned");
        let key = normalize_key(&item.url);
        if inner.seen.insert(key) {
            inner.pending.push_back(item);
            true
        } else {
            false
        }
    }

    /// Pop the next pending crawl item and mark its URL as in-flight.
    pub fn pop(&self) -> Option<CrawlItem> {
        let mut inner = self.0.lock().expect("frontier lock poisoned");
        let item = inner.pending.pop_front()?;
        inner.in_flight.insert(normalize_key(&item.url));
        Some(item)
    }

    /// Mark a URL as successfully done.
    pub fn mark_done(&self, url: &Url) {
        let mut inner = self.0.lock().expect("frontier lock poisoned");
        let key = normalize_key(url);
        inner.in_flight.remove(&key);
        inner.done.insert(key);
    }

    /// Mark a URL as errored.
    pub fn mark_error(&self, url: &Url, msg: impl Into<String>) {
        let mut inner = self.0.lock().expect("frontier lock poisoned");
        let key = normalize_key(url);
        inner.in_flight.remove(&key);
        inner.errors.push((key, msg.into()));
    }

    /// Snapshot counts for the TUI / progress display.
    pub fn counts(&self) -> FrontierCounts {
        let inner = self.0.lock().expect("frontier lock poisoned");
        FrontierCounts {
            pending: inner.pending.len(),
            in_flight: inner.in_flight.len(),
            done: inner.done.len(),
            errors: inner.errors.len(),
        }
    }

    /// Returns `true` when there are no pending or in-flight URLs.
    pub fn is_complete(&self) -> bool {
        let inner = self.0.lock().expect("frontier lock poisoned");
        inner.pending.is_empty() && inner.in_flight.is_empty()
    }
}

/// A point-in-time snapshot of frontier counts.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrontierCounts {
    /// Number of URLs waiting to be fetched.
    pub pending: usize,
    /// Number of URLs currently being fetched.
    pub in_flight: usize,
    /// Number of URLs successfully fetched.
    pub done: usize,
    /// Number of URLs that failed with an error.
    pub errors: usize,
}

/// Normalize a URL to a canonical string key (strip fragment, trailing slash, etc.).
fn normalize_key(url: &Url) -> String {
    let mut u = url.clone();
    u.set_fragment(None);
    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn enqueue_dedup() {
        let f = Frontier::new([u("https://example.com/")]);
        let duplicate = CrawlItem {
            url: u("https://example.com/"),
            kind: TargetKind::Page,
            depth: 0,
        };
        let new_item = CrawlItem {
            url: u("https://example.com/about"),
            kind: TargetKind::Page,
            depth: 1,
        };
        assert!(!f.enqueue(duplicate), "duplicate should not be added");
        assert!(f.enqueue(new_item));
        let counts = f.counts();
        assert_eq!(counts.pending, 2);
    }

    #[test]
    fn pop_marks_in_flight() {
        let f = Frontier::new([u("https://example.com/")]);
        let item = f.pop().unwrap();
        assert_eq!(item.url, u("https://example.com/"));
        assert_eq!(f.counts().in_flight, 1);
        f.mark_done(&item.url);
        assert_eq!(f.counts().done, 1);
        assert_eq!(f.counts().in_flight, 0);
    }

    #[test]
    fn is_complete_when_empty() {
        let f = Frontier::new([u("https://example.com/")]);
        assert!(!f.is_complete());
        let item = f.pop().unwrap();
        f.mark_done(&item.url);
        assert!(f.is_complete());
    }

    #[test]
    fn mark_error_removes_from_in_flight() {
        let f = Frontier::new([u("https://example.com/")]);
        let item = f.pop().unwrap();
        assert_eq!(f.counts().in_flight, 1);
        f.mark_error(&item.url, "connection refused");
        let counts = f.counts();
        assert_eq!(counts.in_flight, 0);
        assert_eq!(counts.errors, 1);
        assert!(f.is_complete());
    }

    #[test]
    fn frontier_handles_fragment_dedup() {
        // URLs differing only in fragment should be deduplicated
        let f = Frontier::new([u("https://example.com/page")]);
        let with_fragment = CrawlItem {
            url: u("https://example.com/page#section"),
            kind: TargetKind::Page,
            depth: 0,
        };
        // Should NOT be added because normalize_key strips fragments
        assert!(!f.enqueue(with_fragment));
        assert_eq!(f.counts().pending, 1);
    }

    #[test]
    fn frontier_multiple_seeds() {
        let f = Frontier::new([
            u("https://example.com/"),
            u("https://example.com/about"),
            u("https://example.com/"), // duplicate
        ]);
        assert_eq!(f.counts().pending, 2);
    }
}
