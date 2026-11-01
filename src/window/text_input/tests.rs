use super::*;

mod classes;
mod configuration;
mod external_edits;
mod first_responder;
mod geometry_rects;
mod key_names;
mod keystrokes;
mod modifier_keys;
mod responder_modes;
mod text_state;
mod utf16_offsets;

use gpui::{Keystroke, Modifiers};
use objc2_ui_kit::{UIKeyModifierFlags, UIKeyboardHIDUsage};

/// The keystroke for a key event with the given reported characters.
fn stroke(
    usage: UIKeyboardHIDUsage,
    characters: &str,
    unmodified: &str,
    flags: UIKeyModifierFlags,
) -> Option<Keystroke> {
    keymap::keystroke(&keymap::KeyEvent {
        usage,
        characters,
        unmodified,
        flags,
    })
}

fn expected(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Option<Keystroke> {
    Some(Keystroke {
        modifiers,
        key: key.to_owned(),
        key_char: key_char.map(str::to_owned),
    })
}
