//! Host-shaped source tests, not UIKit or live-view identity evidence.
//! The generator includes the actual editing::tap_key, InputTarget ingress,
//! Window ingress/dispatch and callback extraction, plus native producer closure.
use super::*;
use crate::fixture_input::{GPUITextInputView, Window};
use crate::native_edit_observer as obs;
use std::{cell::Cell, rc::Rc};

#[cfg(feature = "observer-source-fixture")]
use std::cell::RefCell;
fn stroke() -> Keystroke {
    Keystroke {
        key: "backspace".into(),
        key_char: None,
        modifiers: Modifiers::default(),
    }
}
fn install(window: &Window, callback: impl FnMut(PlatformInput) -> DispatchEventResult + 'static) {
    *window.callbacks.input.borrow_mut() = Some(Box::new(callback));
}
fn assert_pair(drain: &obs::Drain, key: obs::ObservationKey, aborted: bool) {
    let events: Vec<_> = drain
        .records
        .iter()
        .flatten()
        .filter(|r| r.key == key)
        .collect();
    assert_eq!(events.len(), 2, "original invocation has exactly one pair");
    assert_eq!(events[0].event, obs::Event::Entry);
    assert_eq!(
        events[1].event,
        obs::Event::Outcome(if aborted {
            obs::Outcome::Aborted
        } else {
            obs::Outcome::Dispatched {
                key_down_handled: true,
                fallback: obs::Fallback::NotEligible,
            }
        })
    );
}

#[test]
fn generic_tap_keeps_legacy_origin_and_handling_off_and_on() {
    let b = Window::fixture_new();
    let view = GPUITextInputView::fixture_new(&b);
    let seen = Rc::new(Cell::new(0));
    let count = seen.clone();
    install(&b, move |input| {
        assert!(matches!(
            input,
            PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_)
        ));
        assert_eq!(crate::native_edit_action(), Some(NativeEditAction::Copy));
        count.set(count.get() + 1);
        DispatchEventResult { propagate: false }
    });
    let _legacy =
        crate::native_edit_action::NativeEditActionGuard::enter(Some(NativeEditAction::Copy));
    assert!(view.tap_key(stroke()));
    assert_eq!(seen.get(), 2);
}

#[cfg(feature = "observer-source-fixture")]
#[test]
fn native_a_synthetic_b_and_unknown_ingress_have_no_inherited_key() {
    obs::with_source_test_observer(|_, arm| {
        // Fixed numeric registration fixture only: no live registry assertion.
        let reg = Rc::new(obs::fixture_registration(arm));
        let a = Window::fixture_new();
        let b = Window::fixture_new();
        let av = GPUITextInputView::fixture_new(&a);
        let bv = Rc::new(GPUITextInputView::fixture_new(&b));
        let outer = Rc::new(Cell::new(None));
        let inner = Rc::new(Cell::new(None));
        let mode = Rc::new(Cell::new(false));
        let log = Rc::new(RefCell::new(Vec::new()));
        let (out_b, in_b, mode_b, log_b) =
            (outer.clone(), inner.clone(), mode.clone(), log.clone());
        install(&b, move |input| {
            let key = obs::current_native_observation_key();
            if matches!(input, PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_)) {
                if mode_b.get() {
                    let key = key.expect("nested native callback has explicit provenance");
                    assert_ne!(Some(key), out_b.get());
                    in_b.set(Some(key));
                    assert_eq!(crate::native_edit_action(), Some(NativeEditAction::Cut));
                } else {
                    assert_eq!(
                        key, None,
                        "B synthetic/unknown KeyDown and KeyUp must be unobserved"
                    );
                    assert_eq!(crate::native_edit_action(), Some(NativeEditAction::Copy));
                }
                log_b.borrow_mut().push((
                    mode_b.get(),
                    matches!(input, PlatformInput::KeyDown(_)),
                    key,
                ));
            } else {
                assert_eq!(key, None, "native modifier callback must be unobserved");
            }
            DispatchEventResult { propagate: false }
        });
        let (out_a, mode_a, reg_a, bv_a, b_a) = (
            outer.clone(),
            mode.clone(),
            reg.clone(),
            bv.clone(),
            b.clone(),
        );
        install(&a, move |input| {
            if matches!(input, PlatformInput::KeyDown(_)) {
                let key = obs::current_native_observation_key()
                    .expect("native producer positive control");
                out_a.set(Some(key));
                assert!(bv_a.tap_key(stroke()));
                assert_eq!(
                    obs::current_native_observation_key(),
                    Some(key),
                    "A resumes after B tap_key"
                );
                // Unknown direct Window ingress is conservative too, including key events.
                b_a.dispatch(PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: stroke(),
                    is_held: false,
                    prefer_character_input: false,
                }));
                b_a.dispatch(PlatformInput::KeyUp(KeyUpEvent {
                    keystroke: stroke(),
                }));
                assert_eq!(obs::current_native_observation_key(), Some(key));
                mode_a.set(true);
                bv_a.fixture_native(&reg_a, NativeEditAction::Cut);
                mode_a.set(false);
                assert_eq!(
                    obs::current_native_observation_key(),
                    Some(key),
                    "A resumes after nested native"
                );
                assert_eq!(crate::native_edit_action(), Some(NativeEditAction::Copy));
            } else if matches!(input, PlatformInput::KeyUp(_)) {
                assert_eq!(obs::current_native_observation_key(), out_a.get());
            } else {
                assert_eq!(
                    obs::current_native_observation_key(),
                    None,
                    "native modifier callback must be unobserved"
                );
            }
            DispatchEventResult { propagate: false }
        });
        let key = av.fixture_native(&reg, NativeEditAction::Copy).unwrap();
        assert_eq!(Some(key), outer.get());
        assert_ne!(inner.get(), outer.get());
        assert_eq!(obs::current_native_observation_key(), None);
        let log = log.borrow();
        assert_eq!(log.len(), 6);
        assert!(log[..4].iter().all(|(_, _, key)| key.is_none()));
        assert_eq!(
            (log[0].1, log[1].1),
            (true, false),
            "both actual tap_key callbacks observed"
        );
        let drain = obs::fixture_snapshot();
        assert_eq!(
            drain.len, 4,
            "synthetic keys must not create native invocation records"
        );
        assert_pair(&drain, key, false);
        assert_pair(&drain, inner.get().unwrap(), false);
        assert_eq!(
            (
                drain.status.outstanding,
                drain.status.reserved,
                drain.status.dropped_total
            ),
            (0, 0, 0)
        );
        assert!(!drain.status.sticky_invalid);
    });
}

#[cfg(feature = "observer-source-fixture")]
#[test]
fn synthetic_callback_unwind_restores_a_and_original_terminal_pair() {
    for panic_on_down in [false, true] {
        obs::with_source_test_observer(|_, arm| {
            let reg = Rc::new(obs::fixture_registration(arm));
            let a = Window::fixture_new();
            let b = Window::fixture_new();
            let av = GPUITextInputView::fixture_new(&a);
            let bv = GPUITextInputView::fixture_new(&b);
            install(&b, move |input| {
                assert_eq!(
                    obs::current_native_observation_key(),
                    None,
                    "B unwind callback must be unobserved"
                );
                if matches!(input, PlatformInput::KeyDown(_)) == panic_on_down {
                    panic!("fixed synthetic callback unwind");
                }
                DispatchEventResult { propagate: false }
            });
            install(&a, move |input| {
                if matches!(input, PlatformInput::KeyDown(_)) {
                    let key = obs::current_native_observation_key().unwrap();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        bv.tap_key(stroke())
                    }));
                    assert!(result.is_err());
                    assert_eq!(
                        obs::current_native_observation_key(),
                        Some(key),
                        "A scope restored after B callback unwind"
                    );
                    assert_eq!(crate::native_edit_action(), Some(NativeEditAction::Copy));
                }
                DispatchEventResult { propagate: false }
            });
            let key = av.fixture_native(&reg, NativeEditAction::Copy).unwrap();
            assert_eq!(obs::current_native_observation_key(), None);
            let drain = obs::fixture_snapshot();
            assert_eq!(drain.len, 2);
            assert_pair(&drain, key, false);
            assert!(!drain.status.sticky_invalid);
        });
    }
}

#[cfg(feature = "observer-source-fixture")]
#[test]
fn outer_callback_unwind_restores_key_and_aborts_exactly_one_pair() {
    obs::with_source_test_observer(|_, arm| {
        let reg = obs::fixture_registration(arm);
        let a = Window::fixture_new();
        let av = GPUITextInputView::fixture_new(&a);
        let key = Rc::new(Cell::new(None));
        let seen = key.clone();
        install(&a, move |_| {
            seen.set(obs::current_native_observation_key());
            panic!("fixed outer callback unwind");
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            av.fixture_native(&reg, NativeEditAction::Copy)
        }));
        assert!(result.is_err());
        assert_eq!(obs::current_native_observation_key(), None);
        assert_eq!(crate::native_edit_action(), None);
        let drain = obs::fixture_snapshot();
        assert_eq!(drain.len, 2);
        assert_pair(&drain, key.get().unwrap(), true);
        assert_eq!((drain.status.outstanding, drain.status.reserved), (0, 0));
        assert!(drain.status.sticky_invalid);
    });
}
