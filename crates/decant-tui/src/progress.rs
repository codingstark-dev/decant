//! indicatif-backed progress display for non-TTY environments (CI, piped output).

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::state::AppState;

/// Lightweight progress reporter for non-TTY environments.
pub struct ProgressReporter {
    // Kept alive so the bars remain attached; never accessed after construction.
    _multi: MultiProgress,
    bar: ProgressBar,
    state: AppState,
}

impl ProgressReporter {
    /// Create and display the progress bar. `total` is optional — if `None`, a spinner is used.
    pub fn new(state: AppState) -> Self {
        let multi = MultiProgress::new();

        let bar = multi.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.cyan} [{elapsed_precise}] {msg} ({pos} done, {per_sec})",
            )
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );

        Self {
            _multi: multi,
            bar,
            state,
        }
    }

    /// Tick the progress bar. Call this in a loop (e.g. every 100 ms).
    pub fn tick(&self) {
        let (metrics, _, finished, status) = self.state.snapshot();
        let msg = if let Some(s) = status {
            s
        } else {
            format!(
                "{} done | {} in-flight | {} pending | {} errors | {:.1} MB",
                metrics.done,
                metrics.in_flight,
                metrics.pending,
                metrics.errors,
                metrics.bytes_total as f64 / 1_048_576.0,
            )
        };
        self.bar.set_message(msg);
        self.bar.set_position(metrics.done as u64);
        self.bar.tick();
        if finished {
            self.bar.finish_with_message("✓ crawl complete");
        }
    }
}
