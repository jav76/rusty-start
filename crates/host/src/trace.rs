//! Opt-in latency tracing for debug builds and builds run with `RUSTY_START_TRACE_OPEN` set. Logs
//! the time from a search change to the frame that shows it. Every function here must run on the UI
//! thread.

use std::cell::Cell;
use std::sync::LazyLock;
use std::time::Instant;

static ENABLED: LazyLock<bool> =
    LazyLock::new(|| cfg!(debug_assertions) || std::env::var_os("RUSTY_START_TRACE_OPEN").is_some());

thread_local! {
    static SEARCH_CHANGED_AT: Cell<Option<Instant>> = const { Cell::new(None) };
}

pub fn enabled() -> bool {
    *ENABLED
}

/// Starts timing a query the user typed. Call before the rows are rebuilt.
pub fn search_changed() {
    if enabled() {
        SEARCH_CHANGED_AT.set(Some(Instant::now()));
    }
}

/// Drops a pending measurement, for searches the host resets itself while showing or hiding.
pub fn forget_search() {
    SEARCH_CHANGED_AT.set(None);
}

pub fn frame_rendered() {
    if let Some(changed_at) = SEARCH_CHANGED_AT.take() {
        eprintln!("search frame {:.1} ms after the keystroke", changed_at.elapsed().as_secs_f64() * 1000.0);
    }
}
