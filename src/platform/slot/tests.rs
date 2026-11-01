use std::{cell::Cell, rc::Rc};

use super::*;

#[test]
fn an_empty_slot_calls_nothing() {
    let slot = Slot::<dyn FnMut() -> u32>::default();

    assert_eq!(slot.call(|callback| callback()), None);
}

#[test]
fn a_registered_callback_runs_and_stays_registered() {
    let calls = Rc::new(Cell::new(0));
    let slot = Slot::<dyn FnMut() -> u32>::default();
    let counter = Rc::clone(&calls);
    slot.set(Box::new(move || {
        counter.set(counter.get() + 1);
        counter.get()
    }));

    assert_eq!(slot.call(|callback| callback()), Some(1));
    assert_eq!(slot.call(|callback| callback()), Some(2));
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_callback_registered_during_the_call_replaces_the_running_one() {
    let slot = Rc::new(Slot::<dyn FnMut() -> &'static str>::default());
    let inner = Rc::clone(&slot);
    slot.set(Box::new(move || {
        inner.set(Box::new(|| "replacement"));
        "original"
    }));

    assert_eq!(slot.call(|callback| callback()), Some("original"));
    assert_eq!(slot.call(|callback| callback()), Some("replacement"));
}

#[test]
fn a_reentrant_call_finds_the_slot_empty_instead_of_panicking() {
    let slot = Rc::new(Slot::<dyn FnMut() -> Option<u32>>::default());
    let inner = Rc::clone(&slot);
    slot.set(Box::new(move || {
        inner.call(|callback| callback()).flatten()
    }));

    assert_eq!(slot.call(|callback| callback()), Some(None));
}

#[test]
fn try_call_tells_a_missing_callback_from_a_running_one() {
    let slot = Rc::new(Slot::<dyn FnMut() -> Called<u32>>::default());
    assert_eq!(slot.try_call(|callback| callback()), Called::Unregistered);

    let inner = Rc::clone(&slot);
    slot.set(Box::new(move || {
        match inner.try_call(|callback| callback()) {
            Called::Done(_) => Called::Done(1),
            Called::Unregistered => Called::Unregistered,
            Called::Running => Called::Running,
        }
    }));

    assert_eq!(
        slot.try_call(|callback| callback()),
        Called::Done(Called::Running)
    );
}

#[test]
fn gpui_counts_as_running_during_a_callback() {
    let slot = Slot::<dyn FnMut() -> bool>::default();
    slot.set(Box::new(crate::gpui_call::is_running));

    assert_eq!(slot.call(|callback| callback()), Some(true));
    assert!(!crate::gpui_call::is_running());
}
