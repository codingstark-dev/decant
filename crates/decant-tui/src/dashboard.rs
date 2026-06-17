//! Ratatui **0.30.1** live dashboard — full-panel layout with async event handling.
//!
//! # Architecture (Elm / TEA)
//!
//! ```text
//! AppState (Model)
//!      │
//!      ▼ snapshot()  ←──────────────────────────────────┐
//! render(frame)                                          │
//!      │                                                 │
//!      ▼                                           decant-core
//! crossterm EventStream ──► Message ──► run_loop        │
//!                                          │             │
//!                                          └── state.update()
//! ```
//!
//! # Layout
//!
//! ```text
//! ┌─── Queue ────────────────────┬─── Throughput ───────────────────┐
//! │ ✓ Done         142           │  KB/s ▁▃▅▇█▇▅▃▁                 │
//! │ ⏳ In-flight      8          │                                   │
//! │ ⏸ Pending        15         │  Total : 18.4 MB                  │
//! │ ✗ Error           3         │                                   │
//! │                              │                                   │
//! │  Recent URLs:                │                                   │
//! │  › /about                    │                                   │
//! │  › /assets/app.css           │                                   │
//! └──────────────────────────────┴───────────────────────────────────┘
//!  q quit  conc:16  rate:4/s  render:off  robots:on  crawling…
//! ```
//!
//! # What's new in 0.30.1 used here
//!
//! - `ratatui::run(|terminal| …)` — init + panic-hook + restore in one call.
//! - `Block::shadow(Shadow::…)` — drop-shadow on the throughput panel.
//! - `Layout::vertical/horizontal().areas()` — destructuring layout.
//! - `Stylize` trait — `.bold()`, `.cyan()`, `.dim()`, etc. on `&str` / `String`.
//! - `HorizontalAlignment` (was `Alignment` pre-0.29).
//! - `Flex::SpaceEvenly` (was `SpaceAround` pre-0.29).
//!
//! # Usage
//!
//! ```no_run
//! use decant_tui::{AppState, dashboard::{Dashboard, DashboardConfig}};
//!
//! let state = AppState::new();
//! let config = DashboardConfig {
//!     concurrency: 16,
//!     rate_limit: 4,
//!     render_mode: false,
//!     robots_active: true,
//! };
//!
//! // Must run on a thread that owns the tokio runtime handle.
//! std::thread::spawn(move || Dashboard::new(state, config).run().ok());
//! ```

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
use futures::StreamExt as _;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Offset, Rect},
    style::Stylize as _,
    text::Line,
    widgets::{Block, List, ListItem, Paragraph, Shadow, Sparkline},
};
use tokio::time::interval;

use crate::{AppState, TuiError, state::Metrics};

// ── Public types ──────────────────────────────────────────────────────────────

/// Static configuration for the dashboard, set at startup.
///
/// All fields are cheap to clone and read-only during the crawl.
///
/// # Examples
///
/// ```
/// use decant_tui::dashboard::DashboardConfig;
///
/// let cfg = DashboardConfig {
///     concurrency: 16,
///     rate_limit: 4,
///     render_mode: false,
///     robots_active: true,
/// };
/// assert_eq!(cfg.concurrency, 16);
/// ```
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// Maximum number of simultaneous HTTP requests.
    pub concurrency: usize,
    /// Per-host rate limit in requests per second.
    pub rate_limit: u32,
    /// Whether headless-Chrome rendering (`--render`) is active.
    pub render_mode: bool,
    /// Whether robots.txt enforcement is active.
    pub robots_active: bool,
}

// ── Private TEA message type ──────────────────────────────────────────────────

/// Internal Elm-Architecture message. The only way the event loop transitions.
#[derive(Debug)]
enum Message {
    /// 100 ms timer tick — triggers a redraw.
    Tick,
    /// User pressed `q` or `Ctrl-C`.
    Quit,
}

// ── Dashboard ─────────────────────────────────────────────────────────────────

/// The ratatui 0.30.1 dashboard.
///
/// Run via [`Dashboard::run`] on a thread that has a tokio runtime handle
/// (i.e. inside a `std::thread::spawn` called from within `#[tokio::main]`).
pub struct Dashboard {
    state: AppState,
    config: DashboardConfig,
}

impl Dashboard {
    /// Create a new dashboard bound to a shared [`AppState`].
    pub fn new(state: AppState, config: DashboardConfig) -> Self {
        Self { state, config }
    }

    /// Run the dashboard, blocking until the crawl finishes or the user quits.
    ///
    /// Uses `ratatui::run()` (0.30.1) which:
    /// 1. Calls `ratatui::init()` — sets raw mode, enters alternate screen.
    /// 2. Installs a panic hook that restores the terminal before panicking.
    /// 3. Calls the provided closure with the terminal handle.
    /// 4. Calls `ratatui::restore()` on return (even on `Err`).
    ///
    /// # Errors
    ///
    /// Returns [`TuiError::Io`] on any terminal I/O failure.
    pub fn run(self) -> Result<(), TuiError> {
        let state = self.state;
        let config = self.config;

        // ratatui::run() passes &mut DefaultTerminal to the closure,
        // installs a panic hook, and calls ratatui::restore() on exit.
        ratatui::run(|terminal| {
            tokio::runtime::Handle::current()
                .block_on(event_loop(terminal, state, config))
                .map_err(|e| std::io::Error::other(e.to_string()))
        })?;

        Ok(())
    }
}

// ── Async event loop (TEA update) ─────────────────────────────────────────────

/// Async event loop: polls crossterm events + 100 ms tick, dispatches [`Message`].
async fn event_loop(
    terminal: &mut DefaultTerminal,
    state: AppState,
    config: DashboardConfig,
) -> Result<(), TuiError> {
    let mut events = EventStream::new();
    // Tick every 100 ms — smooth sparkline, not too much CPU.
    let mut ticker = interval(Duration::from_millis(100));

    loop {
        let msg = tokio::select! {
            _ = ticker.tick() => Message::Tick,

            Some(Ok(event)) = events.next() => map_event(&event),
        };

        match msg {
            Message::Quit => break,
            Message::Tick => {
                terminal.draw(|f| view(f, &state, &config))?;

                let (_, _, finished, _) = state.snapshot();
                if finished {
                    // Hold the "complete" screen briefly before exiting.
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Map a raw crossterm [`Event`] to a [`Message`].
fn map_event(event: &Event) -> Message {
    match event {
        Event::Key(k) if k.code == KeyCode::Char('q') => Message::Quit,
        Event::Key(k)
            if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Message::Quit
        }
        _ => Message::Tick,
    }
}

// ── View (pure render functions) ──────────────────────────────────────────────

/// Root view — pure function from `AppState` to frame pixels.
///
/// Splitting render logic into small focused functions follows the
/// TEA principle that the view is a pure function of the model.
fn view(frame: &mut Frame, state: &AppState, config: &DashboardConfig) {
    let (metrics, recent_urls, finished, status) = state.snapshot();
    let area = frame.area();

    // ── Vertical split: [body | footer(1)] ────────────────────────────────────
    let [body, footer_area] =
        ratatui::layout::Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    // ── Horizontal split: [queue(50%) | throughput(fill)] ─────────────────────
    let [left, right] =
        ratatui::layout::Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)])
            .areas(body);

    view_queue(frame, left, &metrics, &recent_urls, finished);
    view_throughput(frame, right, &metrics);
    view_footer(frame, footer_area, config, status.as_deref());
}

/// Left panel — queue counters and recent URL ring-buffer.
fn view_queue(
    frame: &mut Frame,
    area: Rect,
    metrics: &Metrics,
    recent_urls: &[String],
    finished: bool,
) {
    // Title colour: green when done, cyan while running.
    let title = if finished {
        Line::from(" Queue ".bold().green())
    } else {
        Line::from(" Queue ".bold().cyan())
    };

    let mut items: Vec<ListItem> = vec![
        ListItem::new(Line::from(vec![
            "✓ Done      : ".green().bold().into(),
            metrics.done.to_string().into(),
        ])),
        ListItem::new(Line::from(vec![
            "⏳ In-flight : ".yellow().into(),
            metrics.in_flight.to_string().into(),
        ])),
        ListItem::new(Line::from(vec![
            "⏸ Pending   : ".blue().into(),
            metrics.pending.to_string().into(),
        ])),
        ListItem::new(Line::from(vec![
            "✗ Error     : ".red().into(),
            metrics.errors.to_string().into(),
        ])),
        ListItem::new(Line::raw("")),
        ListItem::new(Line::from(" Recent URLs:".dark_gray().italic())),
    ];

    for url in recent_urls.iter().rev().take(10) {
        // Truncate long paths so the panel never wraps.
        let display = truncate_url(url, 44);
        items.push(ListItem::new(Line::from(vec![
            "  › ".cyan().into(),
            display.dark_gray().into(),
        ])));
    }

    let block = Block::bordered()
        .title(title)
        .border_style(ratatui::style::Style::new().dark_gray());

    frame.render_widget(List::new(items).block(block), area);
}

/// Right panel — throughput sparkline + total bytes.
///
/// Uses `Block::shadow()` (new in ratatui 0.30.1) for a subtle depth effect.
fn view_throughput(frame: &mut Frame, area: Rect, metrics: &Metrics) {
    // 0.30.1: Block::shadow() with Offset for a two-cell drop-shadow.
    let block = Block::bordered()
        .title(Line::from(" Throughput ".bold().magenta()))
        .border_style(ratatui::style::Style::new().dark_gray())
        .shadow(Shadow::dark_shade().offset(Offset::new(1, 1)));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build sparkline data: KB per 100 ms sample — fits nicely in u64.
    let spark_data: Vec<u64> = metrics
        .throughput_samples
        .iter()
        .map(|(_, bytes)| bytes / 1024)
        .collect();

    // Split the inner area: sparkline on top, stats text below.
    let [spark_area, stats_area] =
        ratatui::layout::Layout::vertical([Constraint::Length(3), Constraint::Fill(1)])
            .areas(inner);

    if !spark_data.is_empty() {
        frame.render_widget(
            Sparkline::default()
                .data(&spark_data)
                .style(ratatui::style::Style::new().cyan()),
            spark_area,
        );
    }

    let total_mb = metrics.bytes_total as f64 / 1_048_576.0;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            "  Total : ".dim().into(),
            format!("{total_mb:.1} MB").bold().into(),
        ])),
        stats_area,
    );
}

/// Footer — one-line status bar with key-binding hints.
fn view_footer(frame: &mut Frame, area: Rect, config: &DashboardConfig, status: Option<&str>) {
    let render_str = if config.render_mode { "on " } else { "off" };
    let robots_str = if config.robots_active { "on " } else { "off" };
    let msg = status.unwrap_or("crawling…");

    // Key bindings block — pattern from ratatui skill style guide.
    let line = Line::from(vec![
        " q ".bold().on_dark_gray(),
        " quit  ".dim().into(),
        "│".dark_gray().into(),
        format!(" conc:{} ", config.concurrency).dark_gray().into(),
        format!(" rate:{}/s ", config.rate_limit).dark_gray().into(),
        format!(" render:{render_str} ").dark_gray().into(),
        format!(" robots:{robots_str} ").dark_gray().into(),
        "│".dark_gray().into(),
        format!(" {msg}").italic().dim().into(),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Truncate a URL to at most `max_chars`, adding a leading `…` if cut.
///
/// # Examples
///
/// ```
/// use decant_tui::dashboard::truncate_url;
///
/// assert_eq!(truncate_url("/short", 20), "/short");
/// assert_eq!(truncate_url("/a/very/long/path/that/exceeds/limit", 10).chars().count(), 10);
/// ```
pub fn truncate_url(url: &str, max_chars: usize) -> String {
    let char_count = url.chars().count();
    if char_count <= max_chars {
        url.to_owned()
    } else {
        if max_chars <= 1 {
            return "…".to_owned();
        }
        // Keep the tail — the interesting part of a URL path is usually the end.
        let skip_count = char_count - (max_chars - 1);
        let tail: String = url.chars().skip(skip_count).collect();
        format!("…{tail}")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    // ── DashboardConfig ───────────────────────────────────────────────────────

    #[test]
    fn config_is_clone_and_debug() {
        let cfg = DashboardConfig {
            concurrency: 8,
            rate_limit: 4,
            render_mode: false,
            robots_active: true,
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.concurrency, cfg.concurrency);
        // Ensure Debug doesn't panic.
        let _repr = format!("{cfg:?}");
    }

    // ── truncate_url ──────────────────────────────────────────────────────────

    #[rstest]
    #[case("/short", 20, "/short")]
    #[case("", 10, "")]
    fn truncate_url_returns_original_when_short(
        #[case] input: &str,
        #[case] max: usize,
        #[case] expected: &str,
    ) {
        assert_eq!(truncate_url(input, max), expected);
    }

    #[test]
    fn truncate_url_limits_length_and_adds_ellipsis() {
        let url = "/a/very/long/path/that/should/be/truncated";
        let result = truncate_url(url, 15);
        assert_eq!(
            result.chars().count(),
            15,
            "result must be exactly max_chars"
        );
        assert!(
            result.starts_with('…'),
            "truncated result must start with '…'"
        );
    }

    #[test]
    fn truncate_url_at_exact_boundary_is_unchanged() {
        let url = "/exactly/ten"; // 12 chars
        let result = truncate_url(url, url.len());
        assert_eq!(result, url);
    }

    // ── map_event ────────────────────────────────────────────────────────────

    #[test]
    fn map_event_q_key_produces_quit() {
        use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};
        let key_event = Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert!(matches!(map_event(&key_event), Message::Quit));
    }

    #[test]
    fn map_event_ctrl_c_produces_quit() {
        use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};
        let key_event = Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert!(matches!(map_event(&key_event), Message::Quit));
    }

    #[test]
    fn map_event_other_key_produces_tick() {
        use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};
        let key_event = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert!(matches!(map_event(&key_event), Message::Tick));
    }
}
