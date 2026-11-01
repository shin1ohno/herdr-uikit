//! Bound retries for resources that can remain unavailable across many frames.

use std::time::{Duration, Instant};

const RETRY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct Retry {
    last_failure: Option<Instant>,
}

impl Retry {
    pub(super) fn allows(&self, now: Instant) -> bool {
        self.last_failure
            .is_none_or(|failed| now.saturating_duration_since(failed) >= RETRY_INTERVAL)
    }

    pub(super) fn failed(&mut self, now: Instant) {
        self.last_failure = Some(now);
    }

    pub(super) fn succeeded(&mut self) {
        self.last_failure = None;
    }
}

#[cfg(test)]
#[path = "tests/retry.rs"]
mod tests;
