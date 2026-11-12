use std::cell::{Cell, RefCell};

use crate::gpui_call::InGpui;

/// One callback GPUI registered with the platform.
///
/// GPUI may register a replacement, or reach the platform again, while its
/// callback runs, so the callback is taken out of the slot for the call
/// instead of being borrowed through it.
pub(super) struct Slot<F: ?Sized> {
    callback: RefCell<Option<Box<F>>>,
    running: Cell<bool>,
}

/// What [`Slot::try_call`] found.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Called<R> {
    /// The callback ran and returned this.
    Done(R),
    /// GPUI registered no callback.
    Unregistered,
    /// The callback is running further up the stack.
    Running,
}

impl<F: ?Sized> Default for Slot<F> {
    fn default() -> Self {
        Self {
            callback: RefCell::new(None),
            running: Cell::new(false),
        }
    }
}

impl<F: ?Sized> Slot<F> {
    pub(super) fn set(&self, callback: Box<F>) {
        self.callback.replace(Some(callback));
    }

    /// Calls the registered callback through `call` and returns its result,
    /// or `None` when nothing is registered or the callback is already
    /// running further up the stack. A callback registered during the call
    /// replaces this one.
    pub(super) fn call<R>(&self, call: impl FnOnce(&mut F) -> R) -> Option<R> {
        match self.try_call(call) {
            Called::Done(result) => Some(result),
            Called::Unregistered | Called::Running => None,
        }
    }

    /// [`Slot::call`], telling a missing callback from a running one.
    pub(super) fn try_call<R>(&self, call: impl FnOnce(&mut F) -> R) -> Called<R> {
        let Some(mut callback) = self.callback.borrow_mut().take() else {
            return if self.running.get() {
                Called::Running
            } else {
                Called::Unregistered
            };
        };
        self.running.set(true);
        let _in_gpui = InGpui::enter();
        let result = call(&mut callback);
        self.running.set(false);
        let mut slot = self.callback.borrow_mut();
        if slot.is_none() {
            *slot = Some(callback);
        }
        Called::Done(result)
    }
}

#[cfg(test)]
mod tests;
