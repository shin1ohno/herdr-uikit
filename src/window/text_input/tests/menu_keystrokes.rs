//! A menu key command hands GPUI the keystroke of its binding as the press
//! would have reported it (`crate::menu::Shortcut`), so it must equal what
//! `keymap::keystroke` makes of the same press.

use objc2_ui_kit::{UIKeyModifierFlags, UIKeyboardHIDUsage};

use super::*;
use crate::menu::Shortcut;

fn shortcut(binding: &str) -> Option<Keystroke> {
    let keystroke = match Keystroke::parse(binding) {
        Ok(keystroke) => keystroke,
        Err(error) => panic!("{binding} is not a keystroke: {error}"),
    };
    Shortcut::new(&keystroke).map(|shortcut| shortcut.keystroke)
}

#[test]
fn a_shortcut_reports_the_keystroke_of_its_press() {
    let control = UIKeyModifierFlags::Control;
    let command = UIKeyModifierFlags::Command;
    let shift = UIKeyModifierFlags::Shift;
    let cases = [
        // Named keys that type a character keep it with control held.
        (
            "ctrl-enter",
            UIKeyboardHIDUsage::KeyboardReturnOrEnter,
            "\r",
            "\r",
            control,
        ),
        (
            "ctrl-tab",
            UIKeyboardHIDUsage::KeyboardTab,
            "\t",
            "\t",
            control,
        ),
        (
            "cmd-shift-enter",
            UIKeyboardHIDUsage::KeyboardReturnOrEnter,
            "\r",
            "\r",
            command | shift,
        ),
        (
            "cmd-left",
            UIKeyboardHIDUsage::KeyboardLeftArrow,
            "",
            "",
            command,
        ),
        // Shifted punctuation is the shifted key, without shift.
        (
            "cmd-shift-/",
            UIKeyboardHIDUsage::KeyboardSlash,
            "?",
            "/",
            command | shift,
        ),
        (
            "cmd-?",
            UIKeyboardHIDUsage::KeyboardSlash,
            "?",
            "/",
            command | shift,
        ),
        (
            "cmd-shift-1",
            UIKeyboardHIDUsage::Keyboard1,
            "!",
            "1",
            command | shift,
        ),
        (
            "ctrl-shift-=",
            UIKeyboardHIDUsage::KeyboardEqualSign,
            "+",
            "=",
            control | shift,
        ),
        // Letters keep shift and stay lowercase.
        (
            "cmd-shift-t",
            UIKeyboardHIDUsage::KeyboardT,
            "T",
            "t",
            command | shift,
        ),
        ("cmd-t", UIKeyboardHIDUsage::KeyboardT, "t", "t", command),
    ];

    for (binding, usage, characters, unmodified, flags) in cases {
        assert_eq!(
            shortcut(binding),
            stroke(usage, characters, unmodified, flags),
            "{binding}"
        );
    }
}
