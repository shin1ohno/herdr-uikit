//! Host tests for the selection display's state machine (issue #139):
//! `scripts/input_mode_mutations.py host` removes each guarded transition in
//! turn and requires the named test to fail.

#[path = "../src/window/text_input/selection_display/state.rs"]
mod state;

use state::{DisplayState, Sink, caret_key};
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    SetActivated(bool),
    NeedsUpdate,
}

#[derive(Default)]
struct Recorder(RefCell<Vec<Call>>);

impl Sink for Recorder {
    fn set_activated(&self, on: bool) {
        self.0.borrow_mut().push(Call::SetActivated(on));
    }
    fn set_needs_update(&self) {
        self.0.borrow_mut().push(Call::NeedsUpdate);
    }
}

impl Recorder {
    fn calls(&self) -> Vec<Call> {
        self.0.borrow().clone()
    }
    fn count(&self, call: Call) -> usize {
        self.0.borrow().iter().filter(|c| **c == call).count()
    }
}

fn active() -> (DisplayState, Recorder) {
    let mut state = DisplayState::default();
    let sink = Recorder::default();
    state.acquired(&sink);
    assert!(state.activated(), "precondition: interaction active");
    (state, sink)
}

#[test]
fn acquire_activates() {
    let (_, sink) = active();
    assert_eq!(sink.calls(), [Call::SetActivated(true), Call::NeedsUpdate]);
}

#[test]
fn resign_deactivates() {
    let (mut state, sink) = active();
    state.resigned(&sink);
    assert_eq!(sink.calls().last(), Some(&Call::SetActivated(false)));
    assert!(!state.activated());
}

#[test]
fn resign_when_inactive_is_silent() {
    let mut state = DisplayState::default();
    let sink = Recorder::default();
    assert!(!state.activated(), "precondition: interaction inactive");
    state.resigned(&sink);
    assert_eq!(sink.calls(), []);
}

#[test]
fn acquire_when_active_does_not_toggle() {
    let (mut state, sink) = active();
    state.acquired(&sink);
    assert_eq!(sink.count(Call::SetActivated(false)), 0);
    assert!(state.activated());
}

#[test]
fn same_caret_does_not_update() {
    let (mut state, sink) = active();
    let before = sink.count(Call::NeedsUpdate);
    let k = caret_key(300.0, 200.0, 2.0, 24.0);
    state.caret(&sink, k);
    state.caret(&sink, k);
    assert_eq!(sink.count(Call::NeedsUpdate) - before, 1);
}

#[test]
fn moved_caret_updates_once() {
    let (mut state, sink) = active();
    let before = sink.count(Call::NeedsUpdate);
    let k1 = caret_key(300.0, 200.0, 2.0, 24.0);
    let k2 = caret_key(309.0, 200.0, 2.0, 24.0);
    state.caret(&sink, k1);
    state.caret(&sink, k2);
    state.caret(&sink, k2);
    assert_eq!(sink.count(Call::NeedsUpdate) - before, 2);
}

#[test]
fn caret_while_inactive_updates_after_acquire() {
    let mut state = DisplayState::default();
    let sink = Recorder::default();
    assert!(!state.activated(), "precondition: interaction inactive");
    state.caret(&sink, caret_key(300.0, 200.0, 2.0, 24.0));
    state.acquired(&sink);
    assert_eq!(sink.calls().last(), Some(&Call::NeedsUpdate));
    assert!(state.activated());
}
