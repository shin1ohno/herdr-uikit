use objc2_ui_kit::UIKeyModifierFlags;

use super::*;
use crate::menu::shortcut::{KeyInput, NamedInput, Shortcut};

fn input(source: &str) -> Option<KeyInput> {
    Shortcut::new(&keystroke(source)).map(|shortcut| shortcut.input)
}

fn flags(source: &str) -> Option<UIKeyModifierFlags> {
    Shortcut::new(&keystroke(source)).map(|shortcut| shortcut.flags)
}

#[test]
fn letters_become_lowercase_inputs_with_their_modifiers() {
    assert_eq!(input("cmd-n"), Some(KeyInput::Text("n".to_owned())));
    assert_eq!(flags("cmd-n"), Some(UIKeyModifierFlags::Command));
    assert_eq!(input("cmd-shift-n"), Some(KeyInput::Text("n".to_owned())));
    assert_eq!(
        flags("cmd-shift-n"),
        Some(UIKeyModifierFlags::Command | UIKeyModifierFlags::Shift)
    );
    assert_eq!(
        flags("ctrl-alt-cmd-k"),
        Some(
            UIKeyModifierFlags::Control
                | UIKeyModifierFlags::Alternate
                | UIKeyModifierFlags::Command
        )
    );
}

#[test]
fn shifted_symbols_keep_their_character() {
    assert_eq!(input("cmd-?"), Some(KeyInput::Text("?".to_owned())));
    assert_eq!(flags("cmd-?"), Some(UIKeyModifierFlags::Command));
    assert_eq!(input("cmd-="), Some(KeyInput::Text("=".to_owned())));
}

#[test]
fn navigation_keys_use_uikit_named_inputs() {
    for (source, named) in [
        ("cmd-up", NamedInput::UpArrow),
        ("cmd-down", NamedInput::DownArrow),
        ("ctrl-left", NamedInput::LeftArrow),
        ("ctrl-right", NamedInput::RightArrow),
        ("cmd-escape", NamedInput::Escape),
        ("ctrl-pageup", NamedInput::PageUp),
        ("ctrl-pagedown", NamedInput::PageDown),
        ("cmd-home", NamedInput::Home),
        ("cmd-end", NamedInput::End),
        ("cmd-delete", NamedInput::Delete),
    ] {
        assert_eq!(input(source), Some(KeyInput::Named(named)), "{source}");
    }
}

#[test]
fn function_keys_up_to_f12_are_named() {
    assert_eq!(
        input("cmd-f1"),
        Some(KeyInput::Named(NamedInput::Function(1)))
    );
    assert_eq!(
        input("ctrl-f12"),
        Some(KeyInput::Named(NamedInput::Function(12)))
    );
    assert_eq!(input("cmd-f13"), None, "UIKit has no input beyond F12");
}

#[test]
fn keys_that_send_characters_use_them() {
    assert_eq!(
        input("cmd-backspace"),
        Some(KeyInput::Text("\u{8}".to_owned()))
    );
    assert_eq!(input("cmd-enter"), Some(KeyInput::Text("\r".to_owned())));
    assert_eq!(input("ctrl-tab"), Some(KeyInput::Text("\t".to_owned())));
    assert_eq!(input("ctrl-space"), None);
    assert_eq!(input("cmd-insert"), None);
}

#[test]
fn only_command_and_control_shortcuts_become_key_commands() {
    for source in [
        "escape",
        "f5",
        "a",
        "alt-a",
        "shift-up",
        "enter",
        "alt-shift-f2",
    ] {
        assert_eq!(
            input(source),
            None,
            "{source} must stay with the text system"
        );
    }
}

#[test]
fn the_function_modifier_has_no_ipad_equivalent() {
    assert_eq!(input("cmd-fn-left"), None);
}

#[test]
fn the_keystroke_matches_what_a_press_reports() {
    let shortcut = Shortcut::new(&keystroke("cmd-shift-t"));
    let expected = Keystroke {
        modifiers: keystroke("cmd-shift-t").modifiers,
        key: "t".to_owned(),
        key_char: None,
    };
    assert_eq!(shortcut.map(|shortcut| shortcut.keystroke), Some(expected));
}

#[test]
fn a_taken_shortcut_is_refused_whatever_its_case() {
    let mut set = ShortcutSet::default();
    assert!(set.insert("F", UIKeyModifierFlags::Command));
    assert!(!set.insert("f", UIKeyModifierFlags::Command));
    assert!(set.insert("f", UIKeyModifierFlags::Command | UIKeyModifierFlags::Shift));
    assert!(set.insert("f", UIKeyModifierFlags::Control));
}
