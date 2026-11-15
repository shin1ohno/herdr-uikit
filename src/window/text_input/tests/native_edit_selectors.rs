//! Real Objective-C edit selectors on a controlled SDK text responder, driven
//! only through the public observer API: begin_run, arm, invoke_native_selector,
//! drain and disable. `observation_test_host` replaces the scene lookup alone.
//! The host has no live window, so the production menu-delivery gate refuses
//! every invocation before dispatch. This proves actual canonical entry, the
//! view's registration, the arm-time enable and the terminal refusal pair. It
//! does NOT prove dispatch, KeyDown/KeyUp key correlation, the Drawing
//! responder, paste publication, consumer callbacks or receiving.
//! `scripts/check-observer-selector-mutations.py` requires each named
//! assertion below to fail under its source mutant.
#![allow(unsafe_code, reason = "main-thread Mach-O initializer")]
#![allow(clippy::expect_used)]

use std::{
    rc::{Rc, Weak},
    sync::atomic::{AtomicU8, Ordering},
};

use gpui::TestAppContext;
use objc2::MainThreadMarker;

use crate::native_edit_observer::{
    self as obs, Action, Decision, Event, InvalidReason, ObserverError, Outcome, Phase,
    ResponderOrigin, SCHEMA_VERSION,
};
use crate::window::{input_target::InputTarget, observation_test_host, text_input::TextInputView};

const ACTIONS: [Action; 4] = [Action::Copy, Action::Cut, Action::Paste, Action::SelectAll];

static RESULT: AtomicU8 = AtomicU8::new(0);

// libtest runs #[test] bodies off the main thread, and every observer API
// requires the main thread. Mach-O initializers run on the main thread before
// the harness; retain only their scalar result.
// SAFETY: __mod_init_func contains C-ABI functions taking no arguments and
// returning nothing. This function has that signature and stores no UIKit
// object beyond its invocation.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CHECK: extern "C" fn() = check;

extern "C" fn check() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let result = std::panic::catch_unwind(|| objc2::rc::autoreleasepool(|_| exercise(mtm)));
    RESULT.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
}

fn send_all(handle: gpui::AnyWindowHandle) {
    for action in ACTIONS {
        obs::invoke_native_selector(handle, action, ResponderOrigin::Text)
            .expect("native-selector-sent");
    }
}

fn exercise(mtm: MainThreadMarker) {
    let mut app = TestAppContext::single();
    let handle = app
        .add_empty_window()
        .update(|window, _| window.window_handle());
    // The production window-less target: no input handler, no live window.
    let input = Rc::new(TextInputView::new(mtm, InputTarget::new(Weak::new())));
    let _hosted = observation_test_host::host(handle, &input);
    let run = obs::begin_run().expect("observer run starts");

    // Off control: identical real selectors before arm record nothing, so arm
    // (which refuses an undrained ring) still succeeds.
    send_all(handle);
    let armed = obs::arm(handle, run, Phase(1)).expect("native-selector-arm");
    assert_eq!(
        obs::invoke_native_selector(handle, Action::Unknown, ResponderOrigin::Text),
        Err(ObserverError::InvalidAction),
        "native-selector-unknown-rejected-before-send"
    );
    send_all(handle);
    let drained = obs::drain(handle, armed).expect("native-selector-drain");
    let status = drained.status;
    eprintln!(
        "SDK11 native selector drain len={} status={status:?}",
        drained.len
    );

    // Each assertion names the property one source mutant breaks.
    assert_eq!(
        status.invalid_reasons & InvalidReason::StaleIdentity.bit(),
        0,
        "native-selector-entry-uses-view-registration"
    );
    assert_eq!(
        drained.len,
        2 * ACTIONS.len(),
        "native-selector-armed-entry-recorded"
    );
    let mut previous = 0;
    for (i, action) in ACTIONS.into_iter().enumerate() {
        let entry = drained.records[2 * i].expect("entry record");
        let outcome = drained.records[2 * i + 1].expect("outcome record");
        assert_eq!(entry.schema_version, SCHEMA_VERSION);
        assert_eq!(entry.key.arm, armed, "native-selector-full-arm-key");
        assert!(
            entry.key.invocation > previous,
            "native-selector-fresh-invocation"
        );
        previous = entry.key.invocation;
        assert_eq!(
            (
                entry.action,
                entry.responder,
                entry.parent,
                entry.decision,
                entry.event
            ),
            (action, ResponderOrigin::Text, None, None, Event::Entry),
            "native-selector-canonical-entry"
        );
        assert_eq!(outcome.key, entry.key, "native-selector-pair-key");
        assert_eq!(
            (outcome.action, outcome.decision, outcome.event),
            (
                action,
                Some(Decision::RefusedMenuDelivery),
                Event::Outcome(Outcome::Refused)
            ),
            "native-selector-terminal-refusal"
        );
    }
    assert_eq!(
        (
            status.invalid_reasons,
            status.dropped_total,
            status.sticky_invalid
        ),
        (0, 0, false),
        "native-selector-run-valid"
    );
    assert_eq!(
        (status.outstanding, status.reserved, status.queued),
        (0, 0, 8),
        "native-selector-no-outstanding"
    );
    assert!(status.enabled);

    // Disabled control: the same real selector after disable records nothing.
    let off = obs::disable(run).expect("native-selector-disable");
    assert!(
        !off.enabled && !off.sticky_invalid,
        "native-selector-disable-clean"
    );
    obs::invoke_native_selector(handle, Action::Copy, ResponderOrigin::Text)
        .expect("native-selector-sent");
    let after = obs::drain(handle, armed).expect("native-selector-drain");
    assert_eq!(after.len, 0, "native-selector-disabled-records-nothing");
    assert!(
        !after.status.sticky_invalid,
        "native-selector-disabled-stays-valid"
    );
}

#[test]
fn native_edit_selectors_record_actual_entry_and_terminal_pairs() {
    assert_eq!(
        RESULT.load(Ordering::Acquire),
        1,
        "native edit selector initializer failed; inspect its named assertion"
    );
}
