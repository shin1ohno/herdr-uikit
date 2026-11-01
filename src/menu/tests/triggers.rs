use objc2_ui_kit::UIKeyModifierFlags;

use super::*;

const COMMAND: UIKeyModifierFlags = UIKeyModifierFlags::Command;

fn keys(flags: UIKeyModifierFlags) -> EventSource {
    EventSource::Keys { flags, serial: 7 }
}

#[test]
fn a_command_performed_during_a_key_event_with_its_modifiers_came_from_the_keyboard() {
    assert_eq!(trigger(COMMAND, keys(COMMAND), None), Trigger::Keyboard);
    // Caps lock and the keypad flag do not decide a shortcut.
    let extra = COMMAND | UIKeyModifierFlags::AlphaShift | UIKeyModifierFlags::NumericPad;
    assert_eq!(trigger(COMMAND, keys(extra), None), Trigger::Keyboard);
}

#[test]
fn a_command_performed_during_a_touch_or_pointer_event_came_from_the_menu() {
    assert_eq!(trigger(COMMAND, EventSource::Other, None), Trigger::Pointer);
}

#[test]
fn a_key_event_without_the_shortcuts_modifiers_picked_the_item_in_the_menu() {
    // Return in an open menu.
    assert_eq!(
        trigger(COMMAND, keys(UIKeyModifierFlags::empty()), None),
        Trigger::Pointer
    );
    assert_eq!(
        trigger(COMMAND, keys(UIKeyModifierFlags::Control), None),
        Trigger::Pointer
    );
    assert_eq!(
        trigger(COMMAND, keys(COMMAND | UIKeyModifierFlags::Alternate), None),
        Trigger::Pointer
    );
}

#[test]
fn with_no_event_being_delivered_the_keystroke_goes_to_gpui() {
    // Stale or unknown state must never skip the keymap.
    assert_eq!(
        trigger(COMMAND, EventSource::Unknown, None),
        Trigger::Keyboard
    );
}

#[test]
fn shift_must_be_held_only_when_the_shortcut_has_it() {
    let redo = COMMAND | UIKeyModifierFlags::Shift;
    assert_eq!(trigger(redo, keys(COMMAND), None), Trigger::Pointer);
    assert_eq!(trigger(redo, keys(redo), None), Trigger::Keyboard);
    // `cmd-?` is typed with shift held but bound without it.
    assert_eq!(trigger(COMMAND, keys(redo), None), Trigger::Keyboard);
}

#[test]
fn a_press_gpui_already_declined_is_not_offered_again() {
    for source in [keys(COMMAND), EventSource::Unknown, EventSource::Other] {
        assert_eq!(
            trigger(COMMAND, source, Some(Route::Forwarded)),
            Trigger::Declined
        );
    }
}

#[test]
fn a_press_gpui_already_handled_is_not_dispatched_again() {
    for source in [keys(COMMAND), EventSource::Unknown] {
        assert_eq!(
            trigger(COMMAND, source, Some(Route::Gpui)),
            Trigger::Handled
        );
    }
}
