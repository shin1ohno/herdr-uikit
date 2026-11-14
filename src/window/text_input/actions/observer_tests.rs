//! Source dispatch regressions; these are not actual native callback receipts.
use super::*;
use std::cell::Cell;

#[test]
fn decision_reasons_preserve_short_circuit_priority() {
    use observer::Decision::*;
    let disallow = crate::NativeEditAvailability::default();
    let allow = crate::NativeEditAvailability {
        paste: true,
        ..disallow
    };
    for active in [false, true] {
        for focused in [false, true] {
            for mode in [None, Some(Mode::Text), Some(Mode::KeyOnly)] {
                assert_eq!(
                    editing_decision(EditAction::Paste, Some(disallow), active, mode, focused)
                        .observed(),
                    if active {
                        RefusedPolicyDisallowed
                    } else {
                        RefusedInactiveOrNoResponder
                    }
                );
                assert_eq!(
                    editing_decision(EditAction::Paste, Some(allow), active, mode, focused)
                        .observed(),
                    if active {
                        AllowedExplicitPolicy
                    } else {
                        RefusedInactiveOrNoResponder
                    }
                );
                let default = editing_decision(EditAction::Paste, None, active, mode, focused);
                assert_eq!(
                    default.observed(),
                    if mode != Some(Mode::Text) {
                        RefusedNoTextMode
                    } else if !focused {
                        RefusedNoTextFocus
                    } else {
                        AllowedDefaultText
                    }
                );
            }
        }
    }
}
#[test]
fn all_actions_keep_keydown_handled_distinct_from_keyup_and_fallback() {
    for action in [
        EditAction::Copy,
        EditAction::Cut,
        EditAction::Paste,
        EditAction::SelectAll,
    ] {
        for handled in [false, true] {
            observer::with_source_test_observer(|registration, _| {
                // Deliberately a source fixture entry; never a native proof.
                let inv = observer::InvocationGuard::enter(
                    registration,
                    match action {
                        EditAction::Copy => observer::Action::Copy,
                        EditAction::Cut => observer::Action::Cut,
                        EditAction::Paste => observer::Action::Paste,
                        EditAction::SelectAll => observer::Action::SelectAll,
                    },
                );
                let key = inv.key();
                let fallback = Cell::new(false);
                let steps = Cell::new(0);
                let summary = dispatch_edit_action_inner(
                    action,
                    (Modifiers::default(), Capslock::default()),
                    |input| {
                        match input {
                            PlatformInput::KeyDown(_) => {
                                assert_eq!(steps.replace(1), 0);
                                assert_eq!(observer::current_native_observation_key(), key);
                                assert_eq!(crate::native_edit_action(), Some(action));
                                handled
                            }
                            PlatformInput::KeyUp(_) => {
                                assert_eq!(steps.replace(2), 1);
                                assert_eq!(observer::current_native_observation_key(), key);
                                !handled // Must not replace the KeyDown return value.
                            }
                            PlatformInput::ModifiersChanged(_) => {
                                assert_eq!(steps.replace(3), 2);
                                assert_eq!(observer::current_native_observation_key(), None);
                                !handled
                            }
                            _ => panic!("unexpected dispatch kind"),
                        }
                    },
                    || {
                        assert_eq!(steps.get(), 3);
                        assert_eq!(observer::current_native_observation_key(), None);
                        fallback.set(true);
                    },
                    key,
                );
                assert_eq!(summary.handled, handled);
                assert_eq!(
                    summary.fallback_invoked,
                    action == EditAction::Paste && !handled
                );
                assert_eq!(summary.fallback_invoked, fallback.get());
                inv.complete(
                    observer::Decision::AllowedExplicitPolicy,
                    observer::Outcome::Dispatched {
                        key_down_handled: summary.handled,
                        fallback: if summary.fallback_invoked {
                            observer::Fallback::Invoked
                        } else {
                            observer::Fallback::NotEligible
                        },
                    },
                );
            });
        }
    }
}
#[test]
fn panic_in_keydown_or_fallback_does_not_become_success() {
    for panic_in_fallback in [false, true] {
        observer::with_source_test_observer(|registration, arm| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let inv = observer::InvocationGuard::enter(registration, observer::Action::Paste);
                let _ = dispatch_edit_action_inner(
                    EditAction::Paste,
                    (Modifiers::default(), Capslock::default()),
                    |_| {
                        assert!(panic_in_fallback, "fixed keydown unwind fixture");
                        false
                    },
                    || panic!("fixed fallback unwind fixture"),
                    inv.key(),
                );
                inv.complete(
                    observer::Decision::AllowedDefaultText,
                    observer::Outcome::Refused,
                );
            }));
            assert!(result.is_err());
            // run_status requires a real main-thread native harness; state-only
            // abort/reservation assertions live in runtime_tests.
            let _ = arm;
            assert_eq!(observer::current_native_observation_key(), None);
        });
    }
}
#[test]
fn rejected_menu_delivery_dispatches_nothing_and_completes_as_refused() {
    use super::super::super::edit_menu::state::Delivery;
    observer::with_source_test_observer(|registration, _| {
        // Deliberately a source fixture entry; never a native proof.
        let inv = observer::InvocationGuard::enter(registration, observer::Action::Paste);
        let touched = Cell::new(false);
        let checked = dispatch_checked_selector_inner(
            sel!(paste:),
            Delivery::Reject,
            true,
            (Modifiers::default(), Capslock::default()),
            |_| {
                touched.set(true);
                false
            },
            || touched.set(true),
            inv.key(),
        );
        assert!(matches!(checked, CheckedDispatch::RejectedDelivery));
        assert!(!touched.get());
        inv.complete(
            observer::Decision::RefusedMenuDelivery,
            observer::Outcome::Refused,
        );
    });
}
