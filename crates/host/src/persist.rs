//! Lets the host write every pending save before the session ends. Each background writer
//! registers a flush function at startup; a power action that ends the session, and the Windows
//! `WM_QUERYENDSESSION`, call `flush_all` from whatever thread they run on.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

/// Longest a single writer may take to confirm its flush, so a stuck disk cannot hold up sign-out.
pub const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

type Flusher = Box<dyn Fn() + Send + Sync>;

static FLUSHERS: Mutex<Vec<Flusher>> = Mutex::new(Vec::new());

/// Registers `flush`, which must return once the writer's pending change is on disk, or after
/// `FLUSH_TIMEOUT` at the latest.
pub fn register(flush: impl Fn() + Send + Sync + 'static) {
    FLUSHERS.lock().unwrap_or_else(PoisonError::into_inner).push(Box::new(flush));
}

/// Flushes every registered writer in turn. Safe to call from any thread.
pub fn flush_all() {
    let flushers = FLUSHERS.lock().unwrap_or_else(PoisonError::into_inner);
    for flush in flushers.iter() {
        flush();
    }
}
