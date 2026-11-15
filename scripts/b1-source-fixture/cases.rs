use crate::{NativeRetirementRequestError as Error, NativeRetirementRequestStatus as Status, *};
use native_operation::{NativeOperation, Phase, dispatcher_task, event};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn origin(w: &window::Window) -> NativeInputOrigin {
    w.text_input.captured_native().native_target().origin()
}
fn request(
    w: &window::Window,
    prepare: impl FnOnce(PreparedNativeRetirement) -> PrepareDecision + 'static,
) -> NativeRetirementRequest {
    request_native_edit_retirement(w.handle, origin(w), prepare).expect("fixture accepted request")
}
fn released(w: &window::Window) {
    assert!(
        w.state.borrow().native_request_pending.is_none(),
        "slot-release-exactly-once"
    );
    assert!(!w.text_input.replacing.get(), "slot-release-exactly-once");
}

#[test]
fn origin_capacity_exhaustion_and_refusals_do_not_reserve() {
    let a = window::fixture(10, 0);
    let b = window::fixture(11, 0);
    assert_ne!(origin(&a), origin(&b), "window-identity-not-numeric-client");
    dispatcher_task(|| {
        assert_eq!(
            request_native_edit_retirement(a.handle, origin(&b), |_| PrepareDecision::Decline)
                .unwrap_err(),
            Error::StaleOrigin,
            "request-window-origin-and-capacity"
        );
        let receipt = request(&a, |_| PrepareDecision::Decline);
        assert_eq!(
            request_native_edit_retirement(a.handle, origin(&a), |_| PrepareDecision::Decline)
                .unwrap_err(),
            Error::InProgress,
            "same-window-one-slot"
        );
        assert_eq!(
            request_native_edit_retirement(b.handle, origin(&b), |_| PrepareDecision::Decline)
                .unwrap_err(),
            Error::NewCapacityBusy,
            "cross-window-new-capacity-one"
        );
        assert_eq!(
            a.state.borrow().native_request_serial,
            receipt.key().serial()
        );
        assert_eq!(b.state.borrow().native_request_serial, 0);
    });
    released(&a);
    released(&b);
    for (client, serial) in [(u64::MAX, 0), (0, u64::MAX)] {
        let w = window::fixture(12, client);
        w.state.borrow_mut().native_request_serial = serial;
        dispatcher_task(|| {
            assert_eq!(
                request_native_edit_retirement(w.handle, origin(&w), |_| PrepareDecision::Decline)
                    .unwrap_err(),
                Error::IdentityExhausted,
                "identity-exhaustion-no-reservation"
            )
        });
        assert_eq!(w.state.borrow().native_request_serial, serial);
        released(&w);
        assert!(w.text_input.captured_native().native_target().client.live());
    }
}

use window::state::press_ledger::{self, KeyWitness, PressFact, PressPhase};

fn press(w: &window::Window, phase: PressPhase, usage: Option<isize>, identity: usize) {
    press_ledger::apply(
        &mut w.state.borrow_mut().press_ledger,
        phase,
        [PressFact::new(usage, identity, ())],
        std::time::Instant::now(),
    );
}
fn refused_as_hardware(w: &window::Window, label: &str) {
    dispatcher_task(|| {
        assert_eq!(
            request_native_edit_retirement(w.handle, origin(w), |_| PrepareDecision::Decline).err(),
            Some(Error::UnsupportedHardwareContext),
            "{label}"
        )
    });
    assert_eq!(w.state.borrow().native_request_serial, 0, "{label}");
    released(w);
}

// S10: a held press refuses a task request without reserving.
#[test]
fn held_press_refuses_request() {
    let w = window::fixture(13, 0);
    press(&w, PressPhase::Began, Some(4), 0);
    refused_as_hardware(&w, "held-press-refused");
    let hold = crate::window::Window::native_hardware_hold(w.handle, MainThreadMarker).unwrap();
    assert_eq!(
        (hold.keyed, hold.unkeyed, hold.held()),
        (1, 0, true),
        "hold-observed"
    );
    press(&w, PressPhase::Began, None, 0x1000);
    refused_as_hardware(&w, "held-press-refused");
}

// S11: once every press ended, hardware history alone admits the request.
#[test]
fn released_window_admits_request() {
    let w = window::fixture(14, 0);
    // Production sets this at all 13 hardware sites; it is diagnostic only.
    w.state.borrow_mut().native_hardware_observed = true;
    press(&w, PressPhase::Began, Some(4), 0);
    press(&w, PressPhase::Ended, Some(4), 0);
    let hold = crate::window::Window::native_hardware_hold(w.handle, MainThreadMarker).unwrap();
    assert!(
        !hold.held() && hold.observed_ever,
        "released-hardware-observed"
    );
    dispatcher_task(|| {
        assert!(
            request_native_edit_retirement(w.handle, origin(&w), |_| PrepareDecision::Decline)
                .is_ok(),
            "released-hardware-admitted"
        )
    });
    released(&w);
}

// S7 / G2: the real `hardware_held` reads the key state when the ledger is
// empty, for each witness field.
#[test]
fn key_state_witness_refuses_request_with_empty_ledger() {
    let none = KeyWitness::default();
    for (witness, label) in [
        (
            KeyWitness { presses: 1, ..none },
            "request-key-state-presses",
        ),
        (
            KeyWitness {
                repeat: true,
                ..none
            },
            "request-key-state-repeat",
        ),
        (
            KeyWitness {
                menu_press: true,
                ..none
            },
            "request-key-state-menu",
        ),
        (
            KeyWitness {
                modifiers: true,
                ..none
            },
            "request-key-state-modifiers",
        ),
    ] {
        let w = window::fixture(15, 0);
        w.text_input.captured_native().witness.set(witness);
        refused_as_hardware(&w, label);
        let hold = crate::window::Window::native_hardware_hold(w.handle, MainThreadMarker).unwrap();
        assert!(hold.key_state_only && hold.held(), "{label}");
    }
}

// G7: the Edit continuation path is refused the same way while a press is held.
#[test]
fn held_press_refuses_edit_request() {
    let w = window::fixture(16, 0);
    press(&w, PressPhase::Began, Some(4), 0);
    let old = w.text_input.captured_native();
    dispatcher_task(|| {
        event(false, || {
            NativeOperation::edit(|op| {
                op.phase(origin(&w), Phase::EditDispatch, || {
                    let _client = old.native_target().client.enter().unwrap();
                    assert_eq!(
                        request_native_edit_retirement(w.handle, origin(&w), |_| {
                            PrepareDecision::Decline
                        })
                        .err(),
                        Some(Error::UnsupportedHardwareContext),
                        "held-press-refuses-edit"
                    );
                })
            })
        })
    });
    assert_eq!(
        w.state.borrow().native_request_serial,
        0,
        "held-press-refuses-edit"
    );
    released(&w);
    // Released, the same Edit continuation is admitted.
    press(&w, PressPhase::Ended, Some(4), 0);
    dispatcher_task(|| {
        event(false, || {
            NativeOperation::edit(|op| {
                op.phase(origin(&w), Phase::EditDispatch, || {
                    let _client = old.native_target().client.enter().unwrap();
                    assert!(
                        request_native_edit_retirement(w.handle, origin(&w), |_| {
                            PrepareDecision::Decline
                        })
                        .is_ok(),
                        "released-edit-admitted"
                    );
                })
            })
        })
    });
    released(&w);
}

#[test]
fn whole_tail_terminal_publication_and_receipt_drop_are_ordered() {
    let a = window::fixture(20, 0);
    let other = window::fixture(21, 0);
    let old = a.text_input.captured_native();
    let events = Rc::new(RefCell::new(Vec::new()));
    let shared = Rc::new(RefCell::new(None::<NativeRetirementRequest>));
    dispatcher_task(|| {
        event(false, || {
            NativeOperation::edit(|op| {
                op.phase(origin(&a), Phase::EditDispatch, || {
                    let _loan = native_operation::loan();
                    let _client = old.native_target().client.enter().unwrap();
                    let seen = events.clone();
                    let state = shared.clone();
                    let weak = Rc::downgrade(&a);
                    let other = Rc::downgrade(&other);
                    let receipt = request(&a, move |prepared| {
                        assert_eq!(
                            *seen.borrow(),
                            ["down", "up", "modifiers", "fallback", "snapshot"],
                            "edit-whole-tail-before-prepare"
                        );
                        let w = weak.upgrade().unwrap();
                        assert!(
                            !w.text_input.captured_native().native_target().client.live(),
                            "old-fenced-before-prepare"
                        );
                        assert_eq!(state.borrow().as_ref().unwrap().key(), &prepared.key);
                        assert_eq!(
                            state.borrow().as_ref().unwrap().mark_owner_cancelled(),
                            NativeRetirementCancellation::TooLate
                        );
                        PrepareDecision::Proceed(Box::new(move |outcome| {
                            assert_eq!(
                                state.borrow().as_ref().unwrap().status(),
                                outcome.status,
                                "terminal-published-before-callback"
                            );
                            assert_eq!(
                                request_native_edit_retirement(w.handle, origin(&w), |_| {
                                    PrepareDecision::Decline
                                })
                                .unwrap_err(),
                                Error::InProgress
                            );
                            let b = other.upgrade().unwrap();
                            assert_eq!(
                                request_native_edit_retirement(b.handle, origin(&b), |_| {
                                    PrepareDecision::Decline
                                })
                                .unwrap_err(),
                                Error::NewCapacityBusy
                            );
                            seen.borrow_mut().push("completed");
                        }))
                    });
                    *shared.borrow_mut() = Some(receipt.clone());
                    drop(receipt);
                    events.borrow_mut().push("down");
                    assert!(old.native_target().client.live(), "queued-tail-live");
                });
                for (phase, name) in [
                    (Phase::EditDispatch, "up"),
                    (Phase::EditDispatch, "modifiers"),
                    (Phase::PasteFallback, "fallback"),
                    (Phase::ObserveSnapshot, "snapshot"),
                ] {
                    op.phase(origin(&a), phase, || events.borrow_mut().push(name));
                }
            })
        })
    });
    assert_eq!(
        events.borrow().last(),
        Some(&"completed"),
        "receipt-clone-does-not-duplicate"
    );
    assert_eq!(a.text_input.constructed.get(), 1);
    assert!(
        old.native_target().client.enter().is_none(),
        "old-native-origin-no-leak"
    );
    released(&a);
}

#[test]
fn cancellation_decline_and_explicit_retry_never_reopen_old() {
    let a = window::fixture(30, 0);
    let old = a.text_input.captured_native();
    let receipt = dispatcher_task(|| {
        let r = request(&a, |p| {
            assert!(p.owner_cancelled, "cancel-ticket-terminal-order");
            // Even an erroneous owner Proceed is coerced to Decline.
            PrepareDecision::Proceed(Box::new(|_| panic!("cancelled owner callback")))
        });
        assert_eq!(
            r.mark_owner_cancelled(),
            NativeRetirementCancellation::Marked
        );
        assert_eq!(
            r.mark_owner_cancelled(),
            NativeRetirementCancellation::Marked
        );
        r
    });
    assert_eq!(receipt.status(), Status::Declined { old_fenced: true });
    assert_eq!(
        receipt.mark_owner_cancelled(),
        NativeRetirementCancellation::AlreadyTerminal
    );
    assert_eq!(a.text_input.constructed.get(), 0);
    released(&a);
    let retry = dispatcher_task(|| {
        let r = request(&a, |_| PrepareDecision::Proceed(Box::new(|_| {})));
        assert_eq!(
            r.status(),
            Status::Queued {
                old_fenced_at_submission: true
            },
            "queued-retry-preserves-existing-fence"
        );
        assert!(old.native_target().client.enter().is_none());
        r
    });
    assert_ne!(receipt.key(), retry.key());
    assert_eq!(a.text_input.constructed.get(), 1);
    assert!(matches!(retry.status(), Status::Replaced { .. }));
    released(&a);
}

#[test]
fn live_and_fenced_interrupt_close_and_refused_legacy_cross_product() {
    for fenced in [false, true] {
        for cause in 0..4 {
            let a = window::fixture(40, 0);
            let b = window::fixture(41, 0);
            let old = a.text_input.captured_native();
            if fenced {
                dispatcher_task(|| {
                    request(&a, |_| PrepareDecision::Decline);
                });
            }
            let receipt = dispatcher_task(|| {
                let r = request(&a, |_| panic!("interrupted-request-never-prepares"));
                if cause == 0 {
                    let observed = r.clone();
                    let weak = Rc::downgrade(&a);
                    retire_text_input_session(b.handle, move |_| {
                    assert!(matches!(observed.status(), Status::InterruptedBeforeFence { .. }), "new-terminal-precedes-legacy-drain");
                    assert!(matches!(observed.status(), Status::InterruptedBeforeFence { old_fenced_at_submission, .. } if old_fenced_at_submission == fenced), "queued-retry-preserves-existing-fence");
                    released(&weak.upgrade().unwrap());
                }).unwrap();
                } else if cause == 1 {
                    let _native = native_operation::refused();
                } else if cause == 2 {
                    a.state.borrow_mut().closed = true;
                } else {
                    assert_eq!(
                        retire_text_input_session(a.handle, |_| panic!("refused-legacy-callback"))
                            .unwrap_err(),
                        TextInputRetirementError::InProgress
                    );
                    assert_eq!(
                        r.status(),
                        Status::Queued {
                            old_fenced_at_submission: fenced
                        },
                        "refused-legacy-noninterference"
                    );
                    let _native = native_operation::refused();
                }
                r
            });
            let observed = match receipt.status() {
                Status::WindowClosed { old_fenced } if cause == 2 => old_fenced,
                Status::InterruptedBeforeFence {
                    old_fenced_at_submission,
                    ..
                } if cause != 2 => old_fenced_at_submission,
                status => panic!("unexpected terminal {status:?}"),
            };
            assert_eq!(observed, fenced, "queued-retry-preserves-existing-fence");
            assert_eq!(
                old.native_target().client.enter().is_none(),
                fenced,
                "prior-token-fence-unchanged"
            );
            assert_eq!(a.text_input.constructed.get(), 0);
            released(&a);
            if cause != 2 {
                let next = dispatcher_task(|| request(&a, |_| PrepareDecision::Decline));
                assert_ne!(next.key(), receipt.key(), "explicit-next-ticket-is-fresh");
                released(&a);
            }
        }
    }
}

#[test]
fn legacy_during_prepare_and_completion_waits_for_release_and_can_refill() {
    let a = window::fixture(50, 0);
    let b = window::fixture(51, 0);
    let c = window::fixture(52, 0);
    let b_handle = b.handle;
    let c_handle = c.handle;
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    let weak = Rc::downgrade(&a);
    dispatcher_task(|| {
        request(&a, move |_| {
            observed.borrow_mut().push("prepare");
            let inside = observed.clone();
            retire_text_input_session(b_handle, move |_| {
                let w = weak.upgrade().unwrap();
                released(&w);
                assert_eq!(
                    request_native_edit_retirement(w.handle, origin(&w), |_| {
                        PrepareDecision::Decline
                    })
                    .unwrap_err(),
                    Error::LegacyDispatchActive,
                    "legacy-drain-does-not-refill-new"
                );
                inside.borrow_mut().push("legacy-b");
                let next = inside.clone();
                retire_text_input_session(c_handle, move |_| next.borrow_mut().push("legacy-c"))
                    .unwrap();
            })
            .unwrap();
            assert_eq!(
                *observed.borrow(),
                ["prepare"],
                "legacy-held-through-prepare"
            );
            PrepareDecision::Proceed(Box::new(move |_| observed.borrow_mut().push("terminal")))
        })
    });
    assert_eq!(
        *events.borrow(),
        ["prepare", "terminal", "legacy-b", "legacy-c"],
        "mixed-api-order-and-new-batch-one"
    );
    released(&a);
}

#[test]
fn close_during_prepare_or_acquire_and_false_readiness_have_terminal_callback() {
    for phase in 0..3 {
        let a = window::fixture(60, 0);
        let called = Rc::new(Cell::new(0));
        let weak = Rc::downgrade(&a);
        let completed = called.clone();
        if phase == 1 {
            let weak = weak.clone();
            *a.text_input.acquire.borrow_mut() = Some(Box::new(move || {
                weak.upgrade().unwrap().state.borrow_mut().closed = true
            }));
        }
        a.text_input.ready.set(false);
        let receipt = dispatcher_task(|| {
            request(&a, move |_| {
                if phase == 0 {
                    weak.upgrade().unwrap().state.borrow_mut().closed = true;
                }
                PrepareDecision::Proceed(Box::new(move |outcome| {
                    if phase < 2 {
                        assert_eq!(outcome.status, Status::WindowClosed { old_fenced: true });
                    } else {
                        assert!(
                            matches!(
                                outcome.status,
                                Status::Replaced {
                                    first_responder: false,
                                    ..
                                }
                            ),
                            "false-readiness-is-not-ready"
                        );
                    }
                    completed.set(completed.get() + 1);
                }))
            })
        });
        assert_eq!(called.get(), 1, "terminal-callback-once-after-proceed");
        assert!(!matches!(
            receipt.status(),
            Status::Queued { .. } | Status::Preparing
        ));
        released(&a);
    }
}

#[test]
fn weak_window_drop_before_tail_releases_global_capacity_and_keeps_prior_fence() {
    for fenced in [false, true] {
        let a = window::fixture(70, 0);
        let old = a.text_input.captured_native();
        if fenced {
            dispatcher_task(|| {
                request(&a, |_| PrepareDecision::Decline);
            });
        }
        let receipt = dispatcher_task(move || {
            let r = request(&a, |_| panic!("dead-window-prepare"));
            drop(a);
            r
        });
        assert_eq!(
            receipt.status(),
            Status::WindowClosed { old_fenced: fenced },
            "queued-retry-preserves-existing-fence"
        );
        assert_eq!(old.native_target().client.live(), !fenced);
        let b = window::fixture(71, 0);
        dispatcher_task(|| {
            request(&b, |_| PrepareDecision::Decline);
        });
        released(&b);
    }
}
