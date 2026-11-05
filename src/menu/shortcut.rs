//! Which GPUI keystrokes become a `UIKeyCommand`'s input and modifier flags.
//!
//! Only keystrokes holding command or control get a key input. UIKit is
//! assumed to match a main-menu key command before the press reaches the
//! first responder's `pressesBegan:withEvent:` (to be verified on a device,
//! see `crate::menu`), so a key command takes the press ahead of the text system
//! and an IME composition. Command and control shortcuts normally reach GPUI
//! before UIKit (`keys::uikit_first`). Control-Space is the exception: input
//! source switching always belongs to UIKit, so it gets no menu key command.
//! Plain keys such as escape, return, or the arrows stay with the text system
//! and the IME, and their menu items show no shortcut.

use gpui::Keystroke;
use objc2_ui_kit::UIKeyModifierFlags;

/// A key UIKit names with one of its `UIKeyInput…` constants rather than a
/// character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NamedInput {
    Delete,
    DownArrow,
    End,
    Escape,
    /// F1 to F12; UIKit has no constants beyond F12.
    Function(u8),
    Home,
    LeftArrow,
    PageDown,
    PageUp,
    RightArrow,
    UpArrow,
}

/// What a key command's `input` is: a character, or a named key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeyInput {
    Named(NamedInput),
    Text(String),
}

impl KeyInput {
    /// The input for a GPUI key name, or `None` for a key no key command can
    /// name (F13 and above, insert, a multi-character name).
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        let named = match key {
            "delete" => NamedInput::Delete,
            "down" => NamedInput::DownArrow,
            "end" => NamedInput::End,
            "escape" => NamedInput::Escape,
            "home" => NamedInput::Home,
            "left" => NamedInput::LeftArrow,
            "pagedown" => NamedInput::PageDown,
            "pageup" => NamedInput::PageUp,
            "right" => NamedInput::RightArrow,
            "up" => NamedInput::UpArrow,
            // Keys UIKit spells as the character they send.
            "backspace" => return Some(Self::Text("\u{8}".to_owned())),
            "enter" => return Some(Self::Text("\r".to_owned())),
            "space" => return Some(Self::Text(" ".to_owned())),
            "tab" => return Some(Self::Text("\t".to_owned())),
            _ => return function_key(key).or_else(|| character(key)),
        };
        Some(Self::Named(named))
    }
}

/// `f1` to `f12`.
fn function_key(key: &str) -> Option<KeyInput> {
    let number: u8 = key.strip_prefix('f')?.parse().ok()?;
    (1..=12)
        .contains(&number)
        .then_some(KeyInput::Named(NamedInput::Function(number)))
}

/// A single printable character, lowercased: UIKit matches a letter key
/// command by its unshifted character, with shift as a modifier flag.
fn character(key: &str) -> Option<KeyInput> {
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c.is_control() || c.is_whitespace() {
        return None;
    }
    Some(KeyInput::Text(c.to_lowercase().collect()))
}

/// A menu item's shortcut: the key command's input and flags, and the GPUI
/// keystroke a press of it stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
    pub(crate) flags: UIKeyModifierFlags,
    pub(crate) input: KeyInput,
    pub(crate) keystroke: Keystroke,
}

impl Shortcut {
    /// The shortcut for `keystroke`, or `None` when it holds neither command
    /// nor control (see the module documentation), holds the function
    /// modifier, which iPad keyboards do not report, or names a key no key
    /// command can.
    pub(crate) fn new(keystroke: &Keystroke) -> Option<Self> {
        let modifiers = keystroke.modifiers;
        if !(modifiers.platform || modifiers.control)
            || modifiers.function
            || crate::key_routing::is_input_source_switch(keystroke)
        {
            return None;
        }
        let input = KeyInput::from_key(&keystroke.key)?;
        let mut flags = UIKeyModifierFlags::empty();
        for (held, flag) in [
            (modifiers.platform, UIKeyModifierFlags::Command),
            (modifiers.control, UIKeyModifierFlags::Control),
            (modifiers.alt, UIKeyModifierFlags::Alternate),
            (modifiers.shift, UIKeyModifierFlags::Shift),
        ] {
            if held {
                flags |= flag;
            }
        }
        Some(Self {
            flags,
            input,
            keystroke: pressed(keystroke),
        })
    }
}

/// The keystroke a press of a command or control binding reports
/// (`text_input::keymap::keystroke`), which GPUI's keymap and elements then
/// see: enter, tab, and space type their character even with command or
/// control held, other keys type none, and shift on a non-letter names the
/// shifted key without shift (`cmd-shift-/` is pressed as `cmd-?`).
fn pressed(binding: &Keystroke) -> Keystroke {
    let mut modifiers = binding.modifiers;
    let mut key = binding.key.clone();
    let mut chars = binding.key.chars();
    if modifiers.shift
        && let (Some(base), None) = (chars.next(), chars.next())
        && let Some(shifted) = crate::window::us_shifted(base)
    {
        modifiers.shift = false;
        key = shifted.to_string();
    }
    let key_char = crate::window::named_key_char(&key).map(str::to_owned);
    Keystroke {
        modifiers,
        key,
        key_char,
    }
}

/// Key commands already in the menu, by input and flags, so no two commands
/// share a shortcut. UIKit's own commands go in first; a later item whose
/// shortcut is taken shows none, and its key press keeps reaching the
/// responder chain as before.
#[derive(Debug, Default)]
pub(crate) struct ShortcutSet(std::collections::HashSet<(String, isize)>);

impl ShortcutSet {
    /// Records a shortcut. Returns `false` when it was already taken.
    pub(crate) fn insert(&mut self, input: &str, flags: UIKeyModifierFlags) -> bool {
        self.0.insert((input.to_lowercase(), flags.0))
    }
}
