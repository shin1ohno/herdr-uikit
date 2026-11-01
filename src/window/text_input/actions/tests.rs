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
