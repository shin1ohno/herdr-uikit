//! Whether GPUI is running on this thread, inside a callback the platform
//! called or a task the dispatcher ran. GPUI holds its app borrowed there,
//! so a UIKit call that would reach GPUI's app synchronously from inside
//! (`validateCommand:` during a focus change GPUI made) must not, or GPUI's
//! `RefCell` panics in an `extern "C"` method and aborts.

use std::cell::Cell;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
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
    }
}

/// Whether a callback or task of GPUI's is on this thread's stack.
pub(crate) fn is_running() -> bool {
    DEPTH.get() > 0
}

#[cfg(test)]
mod tests;
