use super::*;

const NONE: UIKeyModifierFlags = UIKeyModifierFlags::empty();

#[test]
fn plain_letter_types_itself() {
    assert_eq!(
        stroke(UIKeyboardHIDUsage::KeyboardS, "s", "s", NONE),
        expected("s", Some("s"), Modifiers::default())
    );
}

#[test]
fn shifted_letter_keeps_shift_and_lowercase_key() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardA,
            "A",
            "a",
            UIKeyModifierFlags::Shift
        ),
        expected("a", Some("A"), Modifiers::shift())
    );
}

#[test]
fn caps_lock_does_not_change_the_key() {
    let flags = UIKeyModifierFlags::AlphaShift;
    assert_eq!(
        stroke(UIKeyboardHIDUsage::KeyboardA, "A", "A", flags),
        expected("a", Some("A"), Modifiers::default())
    );
}

#[test]
fn shifted_symbol_is_its_own_key_without_shift() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::Keyboard1,
            "!",
            "1",
            UIKeyModifierFlags::Shift
        ),
        expected("!", Some("!"), Modifiers::default())
    );
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardSlash,
            "?",
            "/",
            UIKeyModifierFlags::Shift.union(UIKeyModifierFlags::Command)
        ),
        expected("?", None, Modifiers::command()),
        "cmd-shift-/ binds as cmd-?"
    );
}

#[test]
fn shifted_symbol_falls_back_to_us_layout_when_characters_are_unshifted() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardSlash,
            "/",
            "/",
            UIKeyModifierFlags::Shift.union(UIKeyModifierFlags::Command)
        ),
        expected("?", None, Modifiers::command())
    );
}

#[test]
fn option_types_its_character_but_keeps_the_key() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardS,
            "ß",
            "s",
            UIKeyModifierFlags::Alternate
        ),
        expected("s", Some("ß"), Modifiers::alt())
    );
}

#[test]
fn control_and_command_type_nothing() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardC,
            "\u{3}",
            "c",
            UIKeyModifierFlags::Control
        ),
        expected("c", None, Modifiers::control())
    );
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardS,
            "s",
            "s",
            UIKeyModifierFlags::Command
        ),
        expected("s", None, Modifiers::command())
    );
}

#[test]
fn non_latin_layouts_bind_by_the_us_key() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardC,
            "с",
            "с",
            UIKeyModifierFlags::Command
        ),
        expected("c", None, Modifiers::command()),
        "Cyrillic es under command is cmd-c"
    );
    assert_eq!(
        stroke(UIKeyboardHIDUsage::KeyboardQ, "ๆ", "ๆ", NONE),
        expected("q", Some("ๆ"), Modifiers::default()),
        "Thai types its character on the q key"
    );
}

#[test]
fn accented_latin_letters_stay_themselves() {
    assert_eq!(
        stroke(UIKeyboardHIDUsage::KeyboardSemicolon, "ö", "ö", NONE),
        expected("ö", Some("ö"), Modifiers::default())
    );
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardSemicolon,
            "Ö",
            "ö",
            UIKeyModifierFlags::Shift
        ),
        expected("Ö", Some("Ö"), Modifiers::default())
    );
}

#[test]
fn dead_keys_fall_back_to_the_us_key() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::KeyboardE,
            "",
            "",
            UIKeyModifierFlags::Alternate
        ),
        expected("e", None, Modifiers::alt())
    );
}

#[test]
fn keys_without_any_name_are_none() {
    assert_eq!(
        stroke(UIKeyboardHIDUsage::KeyboardInternational1, "", "", NONE),
        None
    );
}

#[test]
fn keypad_digits_are_digits() {
    assert_eq!(
        stroke(
            UIKeyboardHIDUsage::Keypad5,
            "5",
            "5",
            UIKeyModifierFlags::NumericPad
        ),
        expected("5", Some("5"), Modifiers::default())
    );
}
