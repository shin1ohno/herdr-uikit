//! Whether GPUI is running on this thread, inside a callback the platform
//! called or a task the dispatcher ran. GPUI holds its app borrowed there,
//! so a UIKit call that would reach GPUI's app synchronously from inside
//! (`validateCommand:` during a focus change GPUI made) must not, or GPUI's
//! `RefCell` panics in an `extern "C"` method and aborts.

use std::cell::{Cell, RefCell};

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    static AFTER: RefCell<Vec<Box<dyn FnOnce()>>> = const { RefCell::new(Vec::new()) };
    static DRAINING: Cell<bool> = const { Cell::new(false) };
}

/// Marks GPUI as running until dropped. Nests.
pub(crate) struct InGpui(());

impl InGpui {
    pub(crate) fn enter() -> Self {
        DEPTH.set(DEPTH.get().saturating_add(1));
        Self(())
    }
}

impl Drop for InGpui {
    fn drop(&mut self) {
        DEPTH.set(DEPTH.get().saturating_sub(1));
        if !std::thread::panicking() {
            drain();
        }
    }
}

/// Whether a callback or task of GPUI's is on this thread's stack.
pub(crate) fn is_running() -> bool {
    DEPTH.get() > 0
}

/// Run native work after the current GPUI call returns, without a timer.
/// Only opt-in retirement uses this; one pending operation per live window.
pub(crate) fn after_update(f: impl FnOnce() + 'static) {
    AFTER.with_borrow_mut(|queue| queue.push(Box::new(f)));
    drain();
}

fn drain() {
    if is_running() || DRAINING.replace(true) {
        return;
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            DRAINING.set(false);
        }
    }
    let _reset = Reset;
    loop {
        let batch = AFTER.with_borrow_mut(std::mem::take);
        if batch.is_empty() {
            break;
        }
        for action in batch {
            action();
        }
    }
}

#[cfg(test)]
mod tests;
