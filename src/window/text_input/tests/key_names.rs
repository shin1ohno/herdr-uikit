use super::*;

const NONE: UIKeyModifierFlags = UIKeyModifierFlags::empty();

#[test]
fn named_keys_follow_gpui_names() {
    let cases = [
        (
            UIKeyboardHIDUsage::KeyboardReturnOrEnter,
            "enter",
            Some("\n"),
        ),
        (UIKeyboardHIDUsage::KeypadEnter, "enter", Some("\n")),
        (UIKeyboardHIDUsage::KeyboardTab, "tab", Some("\t")),
        (UIKeyboardHIDUsage::KeyboardSpacebar, "space", Some(" ")),
        (UIKeyboardHIDUsage::KeyboardEscape, "escape", None),
        (
            UIKeyboardHIDUsage::KeyboardDeleteOrBackspace,
            "backspace",
            None,
        ),
        (UIKeyboardHIDUsage::KeyboardDeleteForward, "delete", None),
        (UIKeyboardHIDUsage::KeyboardInsert, "insert", None),
        (UIKeyboardHIDUsage::KeyboardHelp, "insert", None),
        (UIKeyboardHIDUsage::KeyboardHome, "home", None),
        (UIKeyboardHIDUsage::KeyboardEnd, "end", None),
        (UIKeyboardHIDUsage::KeyboardPageUp, "pageup", None),
        (UIKeyboardHIDUsage::KeyboardPageDown, "pagedown", None),
        (UIKeyboardHIDUsage::KeyboardUpArrow, "up", None),
        (UIKeyboardHIDUsage::KeyboardDownArrow, "down", None),
        (UIKeyboardHIDUsage::KeyboardLeftArrow, "left", None),
        (UIKeyboardHIDUsage::KeyboardRightArrow, "right", None),
    ];
    for (usage, key, key_char) in cases {
        // UIKit reports control characters or private-use characters for
        // these keys; the name must not depend on them.
        assert_eq!(
            stroke(usage, "\u{f700}", "\u{f700}", NONE),
            expected(key, key_char, Modifiers::default()),
            "{usage:?}"
        );
    }
}

#[test]
fn function_keys_cover_both_hid_runs() {
    let cases = [
        (UIKeyboardHIDUsage::KeyboardF1, "f1"),
        (UIKeyboardHIDUsage::KeyboardF5, "f5"),
        (UIKeyboardHIDUsage::KeyboardF12, "f12"),
        (UIKeyboardHIDUsage::KeyboardF13, "f13"),
        (UIKeyboardHIDUsage::KeyboardF24, "f24"),
    ];
    for (usage, key) in cases {
        assert_eq!(
            stroke(usage, "", "", NONE),
            expected(key, None, Modifiers::default())
        );
    }
}

#[test]
fn named_keys_keep_their_modifiers() {
    let shift_tab = stroke(
        UIKeyboardHIDUsage::KeyboardTab,
        "\t",
        "\t",
        UIKeyModifierFlags::Shift,
    );
    assert_eq!(
        shift_tab,
        expected("tab", Some("\t"), Modifiers::shift()),
        "shift-tab stays tab with shift, as on macOS"
    );
    let cmd_up = stroke(
        UIKeyboardHIDUsage::KeyboardUpArrow,
        "",
        "",
        UIKeyModifierFlags::Command,
    );
    assert_eq!(cmd_up, expected("up", None, Modifiers::command()));
}

#[test]
fn us_layout_covers_letters_digits_and_punctuation() {
    assert_eq!(
        keymap::us_layout(UIKeyboardHIDUsage::KeyboardA),
        Some(('a', 'A'))
    );
    assert_eq!(
        keymap::us_layout(UIKeyboardHIDUsage::KeyboardZ),
        Some(('z', 'Z'))
    );
    assert_eq!(
        keymap::us_layout(UIKeyboardHIDUsage::Keyboard1),
        Some(('1', '!'))
    );
    assert_eq!(
        keymap::us_layout(UIKeyboardHIDUsage::Keyboard9),
        Some(('9', '('))
    );
    assert_eq!(
        keymap::us_layout(UIKeyboardHIDUsage::Keyboard0),
        Some(('0', ')'))
    );
    assert_eq!(
        keymap::us_layout(UIKeyboardHIDUsage::KeyboardSlash),
        Some(('/', '?'))
    );
    assert_eq!(
        keymap::us_layout(UIKeyboardHIDUsage::Keypad7),
        Some(('7', '7'))
    );
    assert_eq!(keymap::us_layout(UIKeyboardHIDUsage::KeyboardF1), None);
}

#[test]
fn control_characters_from_insert_text_are_keys() {
    let cases = [
        ("\n", "enter", Some("\n"), Modifiers::default()),
        ("\r", "enter", Some("\n"), Modifiers::default()),
        ("\t", "tab", Some("\t"), Modifiers::default()),
        ("\u{1b}", "escape", None, Modifiers::default()),
        ("\u{7f}", "backspace", None, Modifiers::default()),
        ("\u{8}", "backspace", None, Modifiers::default()),
        ("\u{3}", "c", None, Modifiers::control()),
        ("\u{1a}", "z", None, Modifiers::control()),
    ];
    for (text, key, key_char, modifiers) in cases {
        assert_eq!(
            keymap::control_keystroke(text),
            expected(key, key_char, modifiers),
            "{text:?}"
        );
    }
    assert_eq!(keymap::control_keystroke("a"), None);
    assert_eq!(keymap::control_keystroke("\n\n"), None);
    assert_eq!(keymap::control_keystroke(""), None);
}

#[test]
fn only_a_single_control_character_is_a_key() {
    assert!(keymap::is_lone_control("\n"));
    assert!(keymap::is_lone_control("\u{0}"));
    assert!(!keymap::is_lone_control("a"));
    assert!(!keymap::is_lone_control("line\nbreak"));
    assert!(!keymap::is_lone_control(""));
}

#[test]
fn composition_languages_are_cjk() {
    for language in ["ja-JP", "ja", "ko-KR", "zh-Hans", "zh_TW", "yue-Hant"] {
        assert!(keymap::is_composition_language(language), "{language}");
    }
    for language in ["en-US", "jam", "de-DE", "emoji", ""] {
        assert!(!keymap::is_composition_language(language), "{language}");
    }
}
