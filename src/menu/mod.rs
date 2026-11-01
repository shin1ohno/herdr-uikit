//! GPUI's application menus on iPadOS, the logic that needs no UIKit object:
//! the prepared model (`model`), shortcuts (`shortcut`), where each top-level
//! menu goes in UIKit's main menu ([`placement`]), the tag a `UICommand`
//! carries ([`MenuTag`]), and whether a performed command came from a key
//! press ([`trigger`]). `platform::menus` builds and answers the UIKit side.
//!
//! # Keyboard precedence
//!
//! A key press must reach GPUI's keymap exactly as it does without menus:
//! bindings are context-specific, and the daemon's custom keys arrive as
//! bindings too, so dispatching a menu item's action in place of the press
//! would skip both. UIKit is assumed to match main-menu key commands before
//! it calls the first responder's `pressesBegan:withEvent:`; this ordering is
//! not documented and has to be verified on a device. Under that assumption a
//! shortcut's press arrives as the menu command instead of as a press, and
//! the window's text input view hands the shortcut's keystroke to GPUI as
//! the press would have ([`Trigger::Keyboard`]). Only when GPUI declines it
//! does the item's action run, as AppKit's key equivalents do on macOS.
//!
//! Whether a performed command came from a key press is read from the event
//! UIKit is delivering ([`EventSource`]), which the application class records
//! around `sendEvent:` (`platform::application`), never from modifier state a
//! view kept, which goes stale when a modifier went down before the view had
//! focus or its release was lost. A key event whose modifiers match the
//! shortcut's is the press; a touch or pointer event, or a key event without
//! them (return in an open menu), is a pick from the menu
//! ([`Trigger::Pointer`]), which dispatches the action. When no event is
//! being delivered, the source is unknown and the keystroke goes to GPUI:
//! that is harmless for a pick, whose keystroke reaches the same binding,
//! while the reverse would skip the keymap.
//!
//! Were UIKit to deliver the press first instead, nothing is offered twice:
//! the text input view records, per key event, which presses GPUI handled
//! and which it declined and forwarded. A key command for a press GPUI
//! handled is dropped ([`Trigger::Handled`]); one for a press GPUI declined
//! dispatches the action without offering the keystroke again
//! ([`Trigger::Declined`]).
//!
//! Further UIKit behavior this rests on, also to be verified on a device
//! (README, "Menus"): that a key command disabled by `validateCommand:` lets
//! its press go on to the responder chain rather than swallowing it; whether
//! UIKit repeats a held key command and sends `pressesEnded:` for its press
//! (the view sends a repeated perform as a held key and starts no timer of
//! its own, so a lost release cannot repeat forever); and that UIKit performs
//! key commands synchronously inside `sendEvent:`.
//!
//! Only command and control shortcuts become key commands (`shortcut`): the
//! text input view gives those to GPUI ahead of UIKit's text system anyway,
//! so plain keys keep going to the IME first.

mod model;
mod shortcut;

#[cfg(test)]
mod tests;

use objc2_ui_kit::UIKeyModifierFlags;

pub(crate) use self::model::{Command, Element, MenuModel, StandardEdit, Submenu};
pub(crate) use self::shortcut::{KeyInput, NamedInput, Shortcut, ShortcutSet};

/// UIKit's top-level menus a GPUI menu can join instead of standing beside
/// them, so the menu bar has one File, Edit, View, Window, and Help.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StandardMenu {
    Edit,
    File,
    Help,
    View,
    Window,
}

impl StandardMenu {
    pub(crate) const ALL: [Self; 5] =
        [Self::File, Self::Edit, Self::View, Self::Window, Self::Help];

    /// The English title GPUI apps give the menu, as macOS GPUI matches its
    /// Window menu by name.
    fn english_title(self) -> &'static str {
        match self {
            Self::Edit => "Edit",
            Self::File => "File",
            Self::Help => "Help",
            Self::View => "View",
            Self::Window => "Window",
        }
    }
}

/// Where a top-level GPUI menu goes in UIKit's main menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    /// GPUI's first menu is the application menu, as on macOS.
    Application,
    /// Its items join one of UIKit's menus, at the start.
    Standard(StandardMenu),
    /// A menu of its own, after the previous one.
    Sibling,
}

/// Places the menu at `position` titled `title`. `uikit_title` gives the
/// title of UIKit's own menu, `None` when the main menu lacks it. A GPUI menu
/// joins a UIKit menu that exists when its title is the English one or
/// UIKit's localized one.
pub(crate) fn placement(
    position: usize,
    title: &str,
    uikit_title: impl Fn(StandardMenu) -> Option<String>,
) -> Placement {
    if position == 0 {
        return Placement::Application;
    }
    StandardMenu::ALL
        .into_iter()
        .find(|menu| {
            uikit_title(*menu).is_some_and(|uikit| title == menu.english_title() || title == uikit)
        })
        .map_or(Placement::Sibling, Placement::Standard)
}

/// What a `UICommand`'s property list carries: the command's index, and the
/// generation of the menus it was built from, so a command UIKit still holds
/// from an older menu names nothing rather than a different item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MenuTag {
    pub(crate) generation: u32,
    pub(crate) index: u32,
}

impl MenuTag {
    /// The tag for command `index` of generation `generation`, `None` when
    /// the index does not fit.
    pub(crate) fn new(generation: u32, index: usize) -> Option<Self> {
        Some(Self {
            generation,
            index: u32::try_from(index).ok()?,
        })
    }

    pub(crate) fn index(self) -> usize {
        // u32 always fits usize on iOS's 64-bit targets.
        usize::try_from(self.index).unwrap_or(usize::MAX)
    }
}

impl From<MenuTag> for u64 {
    fn from(tag: MenuTag) -> Self {
        (u64::from(tag.generation) << 32) | u64::from(tag.index)
    }
}

impl From<u64> for MenuTag {
    fn from(value: u64) -> Self {
        Self {
            generation: u32::try_from(value >> 32).unwrap_or(u32::MAX),
            index: u32::try_from(value & u64::from(u32::MAX)).unwrap_or(u32::MAX),
        }
    }
}

/// Who received a key's press, so its release goes to the same place and a
/// key command for it is not offered to GPUI a second time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// GPUI handled the press; UIKit never saw it.
    Gpui,
    /// The press went on to UIKit's text system.
    Forwarded,
}

/// The event UIKit is delivering while it performs a menu command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EventSource {
    /// None: UIKit performed the command on its own schedule.
    #[default]
    Unknown,
    /// A key event, numbered in delivery order, with the modifier flags it
    /// reports.
    Keys {
        flags: UIKeyModifierFlags,
        serial: u64,
    },
    /// A touch, pointer, scroll, or other event that is not a key's.
    Other,
}

/// How a performed menu command with a shortcut was triggered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// By its key press: GPUI gets the keystroke first.
    Keyboard,
    /// By a key press GPUI already handled: nothing more happens.
    Handled,
    /// By a key press GPUI already saw and declined.
    Declined,
    /// From the menu, with the pointer, by touch, or with keys in the menu.
    Pointer,
}

/// The modifiers that decide a shortcut. Shift only has to be held when the
/// shortcut has it: `cmd-?` is typed with shift but bound without it.
const DECIDING: UIKeyModifierFlags = UIKeyModifierFlags::Command
    .union(UIKeyModifierFlags::Control)
    .union(UIKeyModifierFlags::Alternate);

/// Classifies a performed command whose shortcut has modifier flags
/// `shortcut`, given the event being delivered and the route of a press of
/// the shortcut's key the text input view saw during that event (see the
/// module documentation, "Keyboard precedence").
pub(crate) fn trigger(
    shortcut: UIKeyModifierFlags,
    source: EventSource,
    prior: Option<Route>,
) -> Trigger {
    match prior {
        Some(Route::Gpui) => return Trigger::Handled,
        Some(Route::Forwarded) => return Trigger::Declined,
        None => {}
    }
    match source {
        EventSource::Unknown => Trigger::Keyboard,
        EventSource::Other => Trigger::Pointer,
        EventSource::Keys { flags, .. } => {
            let pressed = (flags & DECIDING) == (shortcut & DECIDING)
                && (flags.contains(UIKeyModifierFlags::Shift)
                    || !shortcut.contains(UIKeyModifierFlags::Shift));
            if pressed {
                Trigger::Keyboard
            } else {
                Trigger::Pointer
            }
        }
    }
}
