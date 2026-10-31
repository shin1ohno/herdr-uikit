use std::cell::RefCell;

/// One callback GPUI registered with the platform.
///
/// GPUI may register a replacement, or reach the platform again, while its
/// callback runs, so the callback is taken out of the slot for the call
/// instead of being borrowed through it.
pub(super) struct Slot<F: ?Sized>(RefCell<Option<Box<F>>>);

impl<F: ?Sized> Default for Slot<F> {
    fn default() -> Self {
        Self(RefCell::new(None))
    }
}

impl<F: ?Sized> Slot<F> {
    pub(super) fn set(&self, callback: Box<F>) {
        self.0.replace(Some(callback));
    }

    /// Calls the registered callback through `call` and returns its result,
    /// or `None` when nothing is registered or the callback is already
    /// running further up the stack. A callback registered during the call
    /// replaces this one.
    pub(super) fn call<R>(&self, call: impl FnOnce(&mut F) -> R) -> Option<R> {
        let mut callback = self.0.borrow_mut().take()?;
        let result = call(&mut callback);
        let mut slot = self.0.borrow_mut();
        if slot.is_none() {
            *slot = Some(callback);
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests;
