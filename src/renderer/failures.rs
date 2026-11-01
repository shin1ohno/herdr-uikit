//! Consecutive renderer failures retain one typed diagnostic per operation.

use super::Error;

#[derive(Default)]
pub(super) struct FailureLog {
    last: Option<Error>,
}

impl FailureLog {
    pub(super) fn record(&mut self, error: Error) -> Option<&Error> {
        // Scene counts can change every frame while the underlying resource
        // failure persists. Keep the complete first diagnostic for reporting.
        if self
            .last
            .as_ref()
            .is_some_and(|previous| cause(previous) == cause(&error))
        {
            return None;
        }
        self.last = Some(error);
        self.last.as_ref()
    }

    pub(super) fn clear(&mut self) {
        self.last = None;
    }
}

fn cause(mut error: &Error) -> &Error {
    while let Error::Frame { source, .. } = error {
        error = source;
    }
    error
}

#[cfg(test)]
#[path = "tests/failures.rs"]
mod tests;
