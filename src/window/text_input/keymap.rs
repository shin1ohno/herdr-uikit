//! Pure translation from UIKit key events to GPUI keystrokes and modifiers.
//!
//! The rules follow GPUI's macOS platform (`gpui-pre-macos/src/events.rs`,
//! `parse_keystroke`) so key bindings written for the desktop app match on
//! iPad: `key` is the unmodified key (`s` for option-s, `!` for shift-1 with
//! `shift` cleared, letters stay lowercase with `shift` set), and `key_char`
//! is what the key would type, absent when control or command is held.
//!
//! UIKit has no `UCKeyTranslate`, so where macOS asks the keyboard layout for
//! a key's ASCII equivalent this module falls back to the US layout of the
//! key's HID usage. That keeps `cmd-c` working on Cyrillic, Greek, Thai, or
//! kana layouts.

use gpui::{Keystroke, Modifiers};
use objc2_ui_kit::{UIKeyModifierFlags, UIKeyboardHIDUsage};

/// One key event as UIKit reports it on `UIKey`.
#[derive(Clone, Copy, Debug)]
pub(super) struct KeyEvent<'a> {
    /// `keyCode`: the key's HID usage, independent of the layout.
    pub(super) usage: UIKeyboardHIDUsage,
    /// `characters`: what the key types with the held modifiers applied.
    pub(super) characters: &'a str,
    /// `charactersIgnoringModifiers`: what the key types with none held.
    pub(super) unmodified: &'a str,
    /// `modifierFlags`.
    pub(super) flags: UIKeyModifierFlags,
}

/// Keys GPUI names rather than spells.
fn named_key(usage: UIKeyboardHIDUsage) -> Option<&'static str> {
    let named = match usage {
        UIKeyboardHIDUsage::KeyboardReturnOrEnter | UIKeyboardHIDUsage::KeypadEnter => "enter",
        UIKeyboardHIDUsage::KeyboardTab => "tab",
        UIKeyboardHIDUsage::KeyboardSpacebar => "space",
        UIKeyboardHIDUsage::KeyboardEscape => "escape",
        UIKeyboardHIDUsage::KeyboardDeleteOrBackspace => "backspace",
        UIKeyboardHIDUsage::KeyboardDeleteForward => "delete",
        // Apple keyboards label the Insert position "help", and macOS GPUI
        // reports it as insert as well.
        UIKeyboardHIDUsage::KeyboardInsert | UIKeyboardHIDUsage::KeyboardHelp => "insert",
        UIKeyboardHIDUsage::KeyboardHome => "home",
        UIKeyboardHIDUsage::KeyboardEnd => "end",
        UIKeyboardHIDUsage::KeyboardPageUp => "pageup",
        UIKeyboardHIDUsage::KeyboardPageDown => "pagedown",
        UIKeyboardHIDUsage::KeyboardUpArrow => "up",
        UIKeyboardHIDUsage::KeyboardDownArrow => "down",
        UIKeyboardHIDUsage::KeyboardLeftArrow => "left",
        UIKeyboardHIDUsage::KeyboardRightArrow => "right",
        _ => return function_key(usage),
    };
    Some(named)
}

/// The text macOS reports as the `key_char` of a named key that types
/// something, whatever modifiers are held: enter, tab, and space.
pub(crate) fn named_key_char(name: &str) -> Option<&'static str> {
    match name {
        "enter" => Some("\n"),
        "tab" => Some("\t"),
        "space" => Some(" "),
        _ => None,
    }
}

const FUNCTION_KEYS: [&str; 24] = [
    "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15",
    "f16", "f17", "f18", "f19", "f20", "f21", "f22", "f23", "f24",
];

/// F1 to F12 and F13 to F24 are two separate runs in the HID usage table.
fn function_key(usage: UIKeyboardHIDUsage) -> Option<&'static str> {
    let low = UIKeyboardHIDUsage::KeyboardF1.0..=UIKeyboardHIDUsage::KeyboardF12.0;
    let high = UIKeyboardHIDUsage::KeyboardF13.0..=UIKeyboardHIDUsage::KeyboardF24.0;
    let index = if low.contains(&usage.0) {
        usage.0 - low.start()
    } else if high.contains(&usage.0) {
        usage.0 - high.start() + 12
    } else {
        return None;
    };
    usize::try_from(index)
        .ok()
        .and_then(|index| FUNCTION_KEYS.get(index))
        .copied()
}

/// The main block's keys other than letters, with their unshifted and
/// shifted characters on the US layout.
const US_SYMBOLS: [(UIKeyboardHIDUsage, char, char); 21] = [
    (UIKeyboardHIDUsage::Keyboard1, '1', '!'),
    (UIKeyboardHIDUsage::Keyboard2, '2', '@'),
    (UIKeyboardHIDUsage::Keyboard3, '3', '#'),
    (UIKeyboardHIDUsage::Keyboard4, '4', '$'),
    (UIKeyboardHIDUsage::Keyboard5, '5', '%'),
    (UIKeyboardHIDUsage::Keyboard6, '6', '^'),
    (UIKeyboardHIDUsage::Keyboard7, '7', '&'),
    (UIKeyboardHIDUsage::Keyboard8, '8', '*'),
    (UIKeyboardHIDUsage::Keyboard9, '9', '('),
    (UIKeyboardHIDUsage::Keyboard0, '0', ')'),
    (UIKeyboardHIDUsage::KeyboardHyphen, '-', '_'),
    (UIKeyboardHIDUsage::KeyboardEqualSign, '=', '+'),
    (UIKeyboardHIDUsage::KeyboardOpenBracket, '[', '{'),
    (UIKeyboardHIDUsage::KeyboardCloseBracket, ']', '}'),
    (UIKeyboardHIDUsage::KeyboardBackslash, '\\', '|'),
    (UIKeyboardHIDUsage::KeyboardSemicolon, ';', ':'),
    (UIKeyboardHIDUsage::KeyboardQuote, '\'', '"'),
    (UIKeyboardHIDUsage::KeyboardGraveAccentAndTilde, '`', '~'),
    (UIKeyboardHIDUsage::KeyboardComma, ',', '<'),
    (UIKeyboardHIDUsage::KeyboardPeriod, '.', '>'),
    (UIKeyboardHIDUsage::KeyboardSlash, '/', '?'),
];

/// The unshifted and shifted characters of a key on the US layout, the
/// fallback when the reported characters cannot name the key.
pub(super) fn us_layout(usage: UIKeyboardHIDUsage) -> Option<(char, char)> {
    let code = usage.0;
    let letters = UIKeyboardHIDUsage::KeyboardA.0..=UIKeyboardHIDUsage::KeyboardZ.0;
    let keypad_digits = UIKeyboardHIDUsage::Keypad1.0..=UIKeyboardHIDUsage::Keypad9.0;
    if letters.contains(&code) {
        let lower = ascii_offset(b'a', code - letters.start())?;
        return Some((lower, lower.to_ascii_uppercase()));
    }
    if let Some((_, lower, upper)) = US_SYMBOLS.iter().find(|(key, ..)| *key == usage) {
        return Some((*lower, *upper));
    }
    if keypad_digits.contains(&code) {
        let digit = ascii_offset(b'1', code - keypad_digits.start())?;
        return Some((digit, digit));
    }
    let keypad = match usage {
        UIKeyboardHIDUsage::Keypad0 => '0',
        UIKeyboardHIDUsage::KeypadPeriod => '.',
        UIKeyboardHIDUsage::KeypadSlash => '/',
        UIKeyboardHIDUsage::KeypadAsterisk => '*',
        UIKeyboardHIDUsage::KeypadHyphen => '-',
        UIKeyboardHIDUsage::KeypadPlus => '+',
        UIKeyboardHIDUsage::KeypadEqualSign => '=',
        _ => return None,
    };
    Some((keypad, keypad))
}

/// What shift turns a non-letter main-block character into on the US
/// layout (`/` into `?`), the key GPUI names a shifted press by.
pub(crate) fn us_shifted(base: char) -> Option<char> {
    US_SYMBOLS
        .iter()
        .find(|(_, lower, _)| *lower == base)
        .map(|(_, _, upper)| *upper)
}

fn ascii_offset(base: u8, offset: isize) -> Option<char> {
    let offset = u8::try_from(offset).ok()?;
    base.checked_add(offset).map(char::from)
}

/// GPUI's modifiers for UIKit's flags. iPad keyboards report no function
/// modifier (the globe key is not a modifier flag), and caps lock is separate
/// state ([`capslock`]).
pub(super) fn modifiers(flags: UIKeyModifierFlags) -> Modifiers {
    Modifiers {
        control: flags.contains(UIKeyModifierFlags::Control),
        alt: flags.contains(UIKeyModifierFlags::Alternate),
        shift: flags.contains(UIKeyModifierFlags::Shift),
        platform: flags.contains(UIKeyModifierFlags::Command),
        function: false,
    }
}

pub(super) fn capslock(flags: UIKeyModifierFlags) -> bool {
    flags.contains(UIKeyModifierFlags::AlphaShift)
}

/// Which held modifier key a usage is, as a bit of [`HeldModifiers`]: the
/// eight modifier usages are consecutive, left control to right GUI.
pub(super) fn modifier_bit(usage: UIKeyboardHIDUsage) -> Option<u8> {
    let first = UIKeyboardHIDUsage::KeyboardLeftControl.0;
    let last = UIKeyboardHIDUsage::KeyboardRightGUI.0;
    if !(first..=last).contains(&usage.0) {
        return None;
    }
    u32::try_from(usage.0 - first)
        .ok()
        .and_then(|shift| 1u8.checked_shl(shift))
}

pub(super) fn is_capslock(usage: UIKeyboardHIDUsage) -> bool {
    matches!(
        usage,
        UIKeyboardHIDUsage::KeyboardCapsLock | UIKeyboardHIDUsage::KeyboardLockingCapsLock
    )
}

/// The modifier keys currently held, one bit per physical key so releasing
/// one shift keeps `shift` while the other is still down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct HeldModifiers(u8);

impl HeldModifiers {
    // Bit order follows the HID usages: control, shift, alt, GUI for the left
    // side in bits 0-3, then the same for the right side in bits 4-7.
    const CONTROL: u8 = 0b0001_0001;
    const SHIFT: u8 = 0b0010_0010;
    const ALT: u8 = 0b0100_0100;
    const GUI: u8 = 0b1000_1000;

    pub(super) fn set(&mut self, bit: u8, held: bool) {
        if held {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    pub(super) fn modifiers(self) -> Modifiers {
        Modifiers {
            control: self.0 & Self::CONTROL != 0,
            alt: self.0 & Self::ALT != 0,
            shift: self.0 & Self::SHIFT != 0,
            platform: self.0 & Self::GUI != 0,
            function: false,
        }
    }

    /// Adopts the modifiers a key event reports, for presses that began
    /// before this view was first responder and so were never seen. Keys whose
    /// side is unknown are recorded as the left one.
    pub(super) fn sync(&mut self, modifiers: Modifiers) {
        let held = self.modifiers();
        for (wanted, have, mask) in [
            (modifiers.control, held.control, Self::CONTROL),
            (modifiers.alt, held.alt, Self::ALT),
            (modifiers.shift, held.shift, Self::SHIFT),
            (modifiers.platform, held.platform, Self::GUI),
        ] {
            if wanted == have {
                continue;
            }
            if wanted {
                self.0 |= mask & 0x0f;
            } else {
                self.0 &= !mask;
            }
        }
    }
}

/// The GPUI keystroke for a key event, or `None` for a key that has no name
/// GPUI could bind (a dead key on a layout without an ASCII equivalent).
pub(super) fn keystroke(event: &KeyEvent<'_>) -> Option<Keystroke> {
    let mut modifiers = modifiers(event.flags);
    if let Some(name) = named_key(event.usage) {
        return Some(Keystroke {
            modifiers,
            key: name.to_owned(),
            key_char: named_key_char(name).map(str::to_owned),
        });
    }

    let typed = event.characters;
    let key_char = (!modifiers.control
        && !modifiers.platform
        && !typed.is_empty()
        && !typed.chars().any(char::is_control))
    .then(|| typed.to_owned());

    let us = us_layout(event.usage);
    let base = base_key(event.unmodified, us)?;
    let key = if !modifiers.shift || base.chars().all(|c| c.is_ascii_lowercase()) {
        base
    } else if let Some(shifted) = shifted_key(&base, typed, us, modifiers) {
        // Like macOS, shift-1 is the key `!` without shift, so `cmd-?`
        // bindings match rather than `cmd-shift-/`.
        modifiers.shift = false;
        shifted
    } else {
        base
    };
    Some(Keystroke {
        modifiers,
        key,
        key_char,
    })
}

/// The unmodified key name: the reported character, lowercase for letters,
/// and the US equivalent where the layout types no Latin character.
fn base_key(unmodified: &str, us: Option<(char, char)>) -> Option<String> {
    let mut chars = unmodified.chars();
    let usable = match (chars.next(), chars.next()) {
        (Some(c), None) => !c.is_control() && !is_non_latin_letter(c),
        (Some(_), Some(_)) => !unmodified.chars().any(char::is_control),
        (None, _) => false,
    };
    if usable {
        return Some(if unmodified.is_ascii() {
            unmodified.to_ascii_lowercase()
        } else {
            unmodified.to_owned()
        });
    }
    us.map(|(lower, _)| lower.to_string())
}

/// Letters outside the Latin blocks (Cyrillic, Greek, Thai, kana, and so on)
/// are layouts without ASCII keys, where macOS also binds by the ASCII key.
/// Accented Latin letters such as `ö` stay as they are, as on macOS.
fn is_non_latin_letter(c: char) -> bool {
    c.is_alphabetic() && u32::from(c) > 0x024f
}

/// The key a shifted non-letter names. With only shift (and command) held,
/// UIKit's `characters` is exactly that; with option or control it carries
/// their effect too, so the US layout answers instead.
fn shifted_key(
    base: &str,
    typed: &str,
    us: Option<(char, char)>,
    modifiers: Modifiers,
) -> Option<String> {
    let mut typed_chars = typed.chars();
    let single = match (typed_chars.next(), typed_chars.next()) {
        (Some(c), None) if !c.is_control() => Some(c),
        _ => None,
    };
    if !modifiers.alt
        && !modifiers.control
        && let Some(c) = single
        && c.to_string() != base
    {
        return Some(c.to_string());
    }
    let (lower, upper) = us?;
    (base.chars().eq(std::iter::once(lower)) && upper != lower).then(|| upper.to_string())
}

/// The key a lone control character typed through `insertText:` stands for,
/// so the software keyboard's return key reaches GPUI as `enter`.
pub(super) fn control_keystroke(text: &str) -> Option<Keystroke> {
    let mut chars = text.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let (key, key_char, control) = match c {
        '\n' | '\r' => ("enter".to_owned(), Some("\n"), false),
        '\t' => ("tab".to_owned(), Some("\t"), false),
        '\u{1b}' => ("escape".to_owned(), None, false),
        '\u{8}' | '\u{7f}' => ("backspace".to_owned(), None, false),
        '\u{1}'..='\u{1a}' => {
            let letter = u8::try_from(u32::from(c)).ok()?;
            (char::from(b'a' + letter - 1).to_string(), None, true)
        }
        _ => return None,
    };
    Some(Keystroke {
        modifiers: Modifiers {
            control,
            ..Modifiers::default()
        },
        key,
        key_char: key_char.map(str::to_owned),
    })
}

/// Whether `text` is a single control character, which is a key and never
/// text: an unhandled one is dropped, as macOS drops an unhandled return.
pub(super) fn is_lone_control(text: &str) -> bool {
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_control())
}

/// The software keyboard's backspace, as GPUI binds it.
pub(super) fn backspace() -> Keystroke {
    Keystroke {
        modifiers: Modifiers::default(),
        key: "backspace".to_owned(),
        key_char: None,
    }
}

/// Input modes whose IME composes printable keys into other text, so a
/// printable key reaches the IME before key bindings when the focused
/// element asks for that (`prefers_ime_for_printable_keys`), as on macOS.
pub(super) fn is_composition_language(primary_language: &str) -> bool {
    ["ja", "ko", "zh", "yue"].iter().any(|prefix| {
        primary_language
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_']))
    })
}
