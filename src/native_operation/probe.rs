//! Optional receiving hook for the actual raw ingress, not an admission bypass.
use std::cell::RefCell;

type Observer = (&'static str, Box<dyn FnOnce()>);
thread_local! { static NEXT: RefCell<Option<Observer>> = const { RefCell::new(None) }; }

pub(crate) fn register(label: &'static str, callback: impl FnOnce() + 'static) -> bool {
    if objc2::MainThreadMarker::new().is_none() {
        return false;
    }
    NEXT.with_borrow_mut(|next| {
        if next.is_some() {
            return false;
        }
        *next = Some((label, Box::new(callback)));
        true
    })
}

pub(super) fn observe(label: &'static str) {
    let callback = NEXT.with_borrow_mut(|next| {
        if next
            .as_ref()
            .is_some_and(|(expected, _)| *expected == label)
        {
            next.take()
        } else {
            None
        }
    });
    if let Some((_, callback)) = callback {
        callback();
    }
}
