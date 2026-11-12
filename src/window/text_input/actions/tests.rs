use super::*;

use std::cell::Cell;

#[test]
fn standard_edit_selectors_dispatch_command_presses_and_releases() {
    let actions = [
        (sel!(copy:), "c"),
        (sel!(cut:), "x"),
        (sel!(paste:), "v"),
        (sel!(selectAll:), "a"),
    ];
    for (selector, key) in actions {
        let Some(action) = EditAction::from_selector(selector) else {
            panic!("standard editing selector was not recognized");
        };
        let mut events = Vec::new();
        dispatch_edit_action(
            action,
            (Modifiers::default(), Capslock::default()),
            |input| {
                events.push(input);
                true
            },
            || panic!("handled paste must use the app's paste policy"),
        );
        let [
            PlatformInput::KeyDown(down),
            PlatformInput::KeyUp(up),
            PlatformInput::ModifiersChanged(restored),
        ] = events.as_slice()
        else {
            panic!("editing must dispatch one press, one release, and restore modifiers");
        };
        assert_eq!(down.keystroke, up.keystroke);
        assert_eq!(down.keystroke.key, key);
        assert_eq!(
            down.keystroke.modifiers,
            Modifiers {
                platform: true,
                ..Modifiers::default()
            }
        );
        assert!(!down.is_held);
        assert!(!down.prefer_character_input);
        assert_eq!(restored.modifiers, Modifiers::default());
        assert_eq!(restored.capslock, Capslock::default());
    }
    assert_eq!(EditAction::from_selector(sel!(delete:)), None);
}

#[test]
fn only_an_unhandled_paste_reads_and_inserts_clipboard_text() {
    for action in [
        EditAction::Copy,
        EditAction::Cut,
        EditAction::Paste,
        EditAction::SelectAll,
    ] {
        let inserted = Cell::new(false);
        let released = Cell::new(false);
        let restored = Cell::new(false);
        let original = (
            Modifiers {
                alt: true,
                shift: true,
                ..Modifiers::default()
            },
            Capslock { on: true },
        );
        dispatch_edit_action(
            action,
            original,
            |input| {
                if matches!(input, PlatformInput::KeyUp(_)) {
                    released.set(true);
                }
                if let PlatformInput::ModifiersChanged(event) = input {
                    assert!(released.get());
                    assert_eq!((event.modifiers, event.capslock), original);
                    restored.set(true);
                }
                false
            },
            || {
                assert!(released.get(), "fallback runs after the key release");
                assert!(restored.get(), "fallback runs with the original modifiers");
                inserted.set(true);
            },
        );
        assert_eq!(inserted.get(), action == EditAction::Paste);
    }
}

use crate::{NativeEditAction, native_edit_action};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn check_native_origin(selector: Sel, action: NativeEditAction) {
    assert_eq!(native_edit_action(), None);
    // Repeat to expose a scope that leaks into the next selector invocation.
    for _ in 0..2 {
        let mut origins = Vec::new();
        dispatch_edit_selector(
            selector,
            (Modifiers::default(), Capslock::default()),
            |input| {
                origins.push((input, native_edit_action()));
                false
            },
            || assert_eq!(native_edit_action(), None, "fallback has no native origin"),
        );
        assert_eq!(origins.len(), 3);
        assert!(matches!(origins[0].0, PlatformInput::KeyDown(_)));
        assert!(matches!(origins[1].0, PlatformInput::KeyUp(_)));
        assert_eq!(origins[0].1, Some(action), "key-down origin");
        assert_eq!(origins[1].1, Some(action), "key-up origin");
        assert_eq!(origins[2].1, None, "modifier restoration is outside scope");
        assert_eq!(native_edit_action(), None);
    }
}

#[test]
fn native_origin_copy_selector() {
    check_native_origin(sel!(copy:), NativeEditAction::Copy);
}

#[test]
fn native_origin_cut_selector() {
    check_native_origin(sel!(cut:), NativeEditAction::Cut);
}

#[test]
fn native_origin_paste_selector() {
    check_native_origin(sel!(paste:), NativeEditAction::Paste);
}

#[test]
fn native_origin_select_all_selector() {
    check_native_origin(sel!(selectAll:), NativeEditAction::SelectAll);
}

#[test]
fn native_origin_physical_command_v_has_identical_stroke_but_no_origin() {
    use super::super::keymap::{KeyEvent, keystroke};
    use objc2_ui_kit::{UIKeyModifierFlags, UIKeyboardHIDUsage};

    let physical = keystroke(&KeyEvent {
        usage: UIKeyboardHIDUsage::KeyboardV,
        characters: "v",
        unmodified: "v",
        flags: UIKeyModifierFlags::Command,
    });
    let mut native = None;
    dispatch_edit_selector(
        sel!(paste:),
        (Modifiers::default(), Capslock::default()),
        |input| {
            if let PlatformInput::KeyDown(event) = input {
                native = Some(event.keystroke);
                assert_eq!(native_edit_action(), Some(NativeEditAction::Paste));
            }
            true
        },
        || panic!("handled paste must not fall back"),
    );
    assert_eq!(physical, native);
    let Some(physical) = physical else {
        panic!("Command-V must translate to a keystroke");
    };
    super::super::keys::dispatch_key_down(physical.clone(), false, |input| {
        assert!(matches!(input, PlatformInput::KeyDown(_)));
        assert_eq!(native_edit_action(), None);
        true
    });
    super::super::keys::dispatch_key_up(physical, |input| {
        assert!(matches!(input, PlatformInput::KeyUp(_)));
        assert_eq!(native_edit_action(), None);
        true
    });
    assert_eq!(native_edit_action(), None);
}

#[test]
fn native_origin_restores_after_consumer_panics_on_down_or_up() {
    for panic_on_down in [true, false] {
        let mut seen = Vec::new();
        let result = catch_unwind(AssertUnwindSafe(|| {
            dispatch_edit_selector(
                sel!(paste:),
                (Modifiers::default(), Capslock::default()),
                |input| {
                    seen.push(native_edit_action());
                    if matches!(input, PlatformInput::KeyDown(_)) == panic_on_down {
                        panic!("consumer panic");
                    }
                    true
                },
                || panic!("fallback must not run"),
            );
        }));
        assert!(result.is_err());
        assert_eq!(
            seen,
            vec![Some(NativeEditAction::Paste); if panic_on_down { 1 } else { 2 }]
        );
        assert_eq!(native_edit_action(), None, "unwind must clear origin");
        check_native_origin(sel!(copy:), NativeEditAction::Copy);
    }
}

#[test]
fn native_origin_nested_dispatch_restores_outer_even_after_unwind() {
    dispatch_edit_selector(
        sel!(copy:),
        (Modifiers::default(), Capslock::default()),
        |input| {
            if !matches!(input, PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_)) {
                assert_eq!(native_edit_action(), None);
                return true;
            }
            assert_eq!(native_edit_action(), Some(NativeEditAction::Copy));
            let physical = EditAction::Paste.keystroke();
            super::super::keys::dispatch_key_down(physical.clone(), false, |_| {
                assert_eq!(native_edit_action(), None, "reentrant physical down");
                true
            });
            super::super::keys::dispatch_key_up(physical, |_| {
                assert_eq!(native_edit_action(), None, "reentrant physical up");
                true
            });
            assert_eq!(native_edit_action(), Some(NativeEditAction::Copy));
            for unwind in [false, true] {
                let mut inner_seen = None;
                let result = catch_unwind(AssertUnwindSafe(|| {
                    dispatch_edit_selector(
                        sel!(cut:),
                        (Modifiers::default(), Capslock::default()),
                        |input| {
                            if matches!(input, PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_))
                            {
                                inner_seen = native_edit_action();
                                if unwind {
                                    panic!("nested consumer panic");
                                }
                            } else {
                                assert_eq!(native_edit_action(), Some(NativeEditAction::Copy));
                            }
                            true
                        },
                        || panic!("Cut has no paste fallback"),
                    );
                }));
                assert_eq!(result.is_err(), unwind);
                assert_eq!(inner_seen, Some(NativeEditAction::Cut));
                assert_eq!(native_edit_action(), Some(NativeEditAction::Copy));
            }
            true
        },
        || panic!("Copy has no paste fallback"),
    );
    assert_eq!(native_edit_action(), None);
}

#[test]
fn explicit_edit_policy_is_independent_of_keyboard_mode_and_rechecked() {
    use crate::NativeEditAvailability;
    for mode in [Some(Mode::Text), Some(Mode::KeyOnly)] {
        for selected in [false, true] {
            let policy = NativeEditAvailability {
                copy: selected,
                paste: true,
                ..Default::default()
            };
            assert!(editing_allowed(
                EditAction::Paste,
                Some(policy),
                true,
                mode,
                false
            ));
            assert_eq!(
                editing_allowed(EditAction::Copy, Some(policy), true, mode, false),
                selected
            );
            assert!(!editing_allowed(
                EditAction::Cut,
                Some(policy),
                true,
                mode,
                true
            ));
            assert!(!editing_allowed(
                EditAction::SelectAll,
                Some(policy),
                true,
                mode,
                true
            ));
            // Losing the window/responder or clearing policy must refuse a stale command.
            assert!(!editing_allowed(
                EditAction::Paste,
                Some(policy),
                false,
                mode,
                true
            ));
            assert!(!editing_allowed(
                EditAction::Paste,
                Some(NativeEditAvailability::default()),
                true,
                mode,
                true
            ));
        }
    }
    for mode in [None, Some(Mode::Text), Some(Mode::KeyOnly)] {
        for focused in [false, true] {
            assert_eq!(
                editing_allowed(EditAction::Paste, None, true, mode, focused),
                mode == Some(Mode::Text) && focused
            );
        }
    }
}

#[test]
fn both_responders_override_standard_edit_validation_and_execution() {
    use objc2::ClassType;
    for class in [
        GPUITextInputView::class(),
        crate::window::view::GpuiView::class(),
    ] {
        let methods = class.instance_methods();
        for selector in [
            sel!(canPerformAction:withSender:),
            sel!(validateCommand:),
            sel!(copy:),
            sel!(paste:),
            sel!(cut:),
            sel!(selectAll:),
        ] {
            assert!(
                methods.iter().any(|method| method.name() == selector),
                "{class:?} {selector:?}"
            );
        }
    }
}
