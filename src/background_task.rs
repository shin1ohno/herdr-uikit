//! Time to finish work after the app leaves the screen. UIKit suspends an
//! app soon after its last scene enters the background, and may end it later
//! without telling it, unless the app holds a background task. The system
//! grants a task about 30 seconds and calls its expiration handler when the
//! time is up; the task must end by then, or the system ends the app.

#![allow(
    unsafe_code,
    reason = "reads UIKit's exported UIBackgroundTaskInvalid constant"
)]

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;
use objc2_foundation::NSString;
use objc2_ui_kit::{UIApplication, UIBackgroundTaskIdentifier, UIBackgroundTaskInvalid};

/// Keeps the app running in the background until dropped or until the
/// system's time runs out, whichever comes first; the task ends exactly once.
/// Begin it on the main thread; it may be dropped on any thread.
#[derive(Debug)]
pub struct BackgroundTask {
    claim: Arc<Claim<UIBackgroundTaskIdentifier>>,
}

impl BackgroundTask {
    /// Asks the system for background time, named `name` in its diagnostics.
    /// `None`, logged, off the main thread or when the system refuses, as it
    /// does when the app has no background time left.
    pub fn begin(name: &str) -> Option<Self> {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!(name, "not beginning a background task off the main thread");
            return None;
        };
        let application = UIApplication::sharedApplication(mtm);
        let task_name = NSString::from_str(name);
        let claim = Claim::begin(
            |expiry| {
                // UIKit calls the handler on the main thread when the time
                // is up; the task must have ended when it returns.
                let handler = RcBlock::new(move || {
                    tracing::warn!("a background task ran out of time");
                    expiry.end(end_now);
                });
                let identifier = application.beginBackgroundTaskWithName_expirationHandler(
                    Some(&*task_name),
                    Some(&*handler),
                );
                // SAFETY: UIKit exports the constant, and it never changes.
                let invalid = unsafe { UIBackgroundTaskInvalid };
                (identifier != invalid).then_some(identifier)
            },
            end_now,
        );
        if claim.is_none() {
            tracing::warn!(name, "the system gave no background time");
        }
        claim.map(|claim| Self { claim })
    }
}

impl Drop for BackgroundTask {
    fn drop(&mut self) {
        if MainThreadMarker::new().is_some() {
            self.claim.end(end_now);
            return;
        }
        // The task is ended on the main thread, where the expiration handler
        // also runs, so whichever of the two comes first ends it.
        let claim = Arc::clone(&self.claim);
        DispatchQueue::main().exec_async(move || claim.end(end_now));
    }
}

fn end_now(identifier: UIBackgroundTaskIdentifier) {
    match MainThreadMarker::new() {
        Some(mtm) => UIApplication::sharedApplication(mtm).endBackgroundTask(identifier),
        None => tracing::error!("a background task was ended off the main thread"),
    }
}

/// Where a begun task is.
#[derive(Clone, Copy, Debug)]
enum State<I> {
    /// The system has not returned the task's identifier yet.
    Starting,
    Running(I),
    Ended,
}

/// One system task, ended exactly once by whichever of its owner and its
/// expiration handler gets there first.
#[derive(Debug)]
struct Claim<I> {
    state: Mutex<State<I>>,
}

impl<I: Copy> Claim<I> {
    /// Begins a task through `begin`, which is handed the claim for the
    /// task's expiration handler and returns the task's identifier, or
    /// `None` when the system refused. A task that expired before `begin`
    /// returned is ended with `end` at once, and `None` returned.
    fn begin(begin: impl FnOnce(Arc<Self>) -> Option<I>, end: impl FnOnce(I)) -> Option<Arc<Self>> {
        let claim = Arc::new(Self {
            state: Mutex::new(State::Starting),
        });
        let identifier = begin(Arc::clone(&claim))?;
        let mut state = claim.lock();
        match *state {
            State::Starting => {
                *state = State::Running(identifier);
                drop(state);
                Some(claim)
            }
            State::Running(_) | State::Ended => {
                *state = State::Ended;
                drop(state);
                end(identifier);
                None
            }
        }
    }

    /// Ends the task with `end`, unless it already ended.
    fn end(&self, end: impl FnOnce(I)) {
        let previous = std::mem::replace(&mut *self.lock(), State::Ended);
        if let State::Running(identifier) = previous {
            end(identifier);
        }
    }

    /// The state is a plain value, consistent even when a holder panicked.
    fn lock(&self) -> MutexGuard<'_, State<I>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests;
