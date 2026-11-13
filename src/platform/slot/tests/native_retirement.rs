//! Actual callback-slot restoration controls. No native/OS delivery claim.
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

use super::super::Slot;

struct Restored {
    slot: Weak<Slot<dyn FnMut()>>,
    dropped: Rc<Cell<bool>>,
}
impl Drop for Restored {
    fn drop(&mut self) {
        assert!(
            crate::native_operation::loan_count_for_test() > 0,
            "callback-loan-through-restore"
        );
        assert!(
            self.slot
                .upgrade()
                .is_some_and(|slot| slot.callback.borrow().is_some()),
            "newer-callback-restored-before-old-drop"
        );
        self.dropped.set(true);
    }
}

#[test]
fn new_callback_wins_while_loan_spans_old_callback_drop() {
    let slot: Rc<Slot<dyn FnMut()>> = Rc::new(Slot::default());
    let dropped = Rc::new(Cell::new(false));
    let count = Rc::new(Cell::new(0));
    let restored = Restored {
        slot: Rc::downgrade(&slot),
        dropped: dropped.clone(),
    };
    let weak = Rc::downgrade(&slot);
    let next_count = count.clone();
    slot.set(Box::new(move || {
        let _keep = &restored;
        // Assert in Restored::drop, after the newer callback is installed. An
        // earlier assertion would mask the restoration boundary and double-panic.
        if let Some(slot) = weak.upgrade() {
            let count = next_count.clone();
            slot.set(Box::new(move || count.set(count.get() + 1)));
        }
    }));
    slot.call(|callback| callback());
    assert!(dropped.get());
    assert_eq!(
        crate::native_operation::loan_count_for_test(),
        0,
        "callback-loan-released-after-restore"
    );
    slot.call(|callback| callback());
    assert_eq!(count.get(), 1, "newer-callback-wins");
    assert_eq!(crate::native_operation::loan_count_for_test(), 0);
}

#[test]
fn empty_slot_releases_loan_on_early_return() {
    let slot: Slot<dyn FnMut()> = Slot::default();
    assert!(slot.call(|callback| callback()).is_none());
    assert_eq!(
        crate::native_operation::loan_count_for_test(),
        0,
        "callback-loan-empty-cleanup"
    );
}
