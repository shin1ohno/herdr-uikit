//! Control-letter keys on a Mac, where AppKit would take them first.
//!
//! An iPad app running on a Mac ("Designed for iPad") gets its hardware keys
//! through AppKit while this view is the text input client. AppKit looks a
//! key down up in its standard text key bindings (`StandardKeyBinding.dict`)
//! before UIKit sees it, so a control letter bound there, such as control-A
//! (`moveToBeginningOfParagraph:`), was consumed as a text command and never
//! reached `pressesBegan:withEvent:`: terminal programs and the consumer's
//! key handling (a Herdr prefix) never got it. On the Mac (macOS 26.7) a
//! `UIKeyCommand` for the chord whose `wantsPriorityOverSystemBehavior` is
//! set makes UIKit deliver the key as a press again, so it takes the same
//! route through `keys` as on an iPad: GPUI gets it first, and a press GPUI
//! handles never performs the command. Only when GPUI declines the press does
//! UIKit perform the command; [`GPUITextInputView::control_key`] then sends
//! the control character the way `insertText:` does, which `editing` does
//! not dispatch a second time for a press that went to UIKit.
//!
//! On an iPad the view lists no key commands, so nothing changes there:
//! these keys already arrive as presses.

#![allow(unsafe_code, reason = "creates UIKeyCommands through objc2")]

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, sel};
use objc2_foundation::{NSArray, NSProcessInfo, NSString};
use objc2_ui_kit::{UIKeyCommand, UIKeyModifierFlags};

use super::view_class::GPUITextInputView;

/// The letters AppKit's standard key bindings take with only control held
/// (`StandardKeyBinding.dict` on macOS 26): control-A, B, D, E, F, H, K, L,
/// N, O, P, T, V and Y. Unbound control letters such as control-C reach the
/// view as `insertText:` with the control character, which `editing`
/// already turns into the key.
pub(super) const LETTERS: [char; 14] = [
    'a', 'b', 'd', 'e', 'f', 'h', 'k', 'l', 'n', 'o', 'p', 't', 'v', 'y',
];

/// The action of the control-letter key commands.
pub(super) fn action() -> Sel {
    sel!(herdrControlKey:)
}

/// Whether this is an iPad app running on a Mac, where AppKit's key
/// bindings apply.
pub(super) fn on_mac() -> bool {
    NSProcessInfo::processInfo().isiOSAppOnMac()
}

/// The control character for a key command's input, a lowercase letter.
pub(super) fn control_character(input: &str) -> Option<char> {
    let mut chars = input.chars();
    let (Some(letter), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if !LETTERS.contains(&letter) {
        return None;
    }
    char::from_u32(u32::from(letter) - u32::from('a') + 1)
}

/// The key commands for [`LETTERS`] with control, or none off a Mac.
pub(super) fn key_commands(
    mac: bool,
    mtm: MainThreadMarker,
) -> Option<Retained<NSArray<UIKeyCommand>>> {
    if !mac {
        return None;
    }
    let commands: Vec<_> = LETTERS
        .iter()
        .map(|letter| {
            // SAFETY: the action takes the sender and returns nothing, as
            // the text input view's `herdrControlKey:` does.
            let command = unsafe {
                UIKeyCommand::keyCommandWithInput_modifierFlags_action(
                    &NSString::from_str(&letter.to_string()),
                    UIKeyModifierFlags::Control,
                    action(),
                    mtm,
                )
            };
            command.setWantsPriorityOverSystemBehavior(true);
            // The chord names the key as typed on the US layout, as GPUI's
            // bindings do, so UIKit must not remap it.
            command.setAllowsAutomaticLocalization(false);
            command
        })
        .collect();
    Some(NSArray::from_retained_slice(&commands))
}

impl GPUITextInputView {
    /// UIKit performed a control-letter key command: the press reached
    /// GPUI, which declined it, or never reached this view.
    pub(super) fn control_key(&self, sender: Option<&AnyObject>) {
        let control = sender
            .and_then(|sender| sender.downcast_ref::<UIKeyCommand>())
            .and_then(UIKeyCommand::input)
            .and_then(|input| control_character(&input.to_string()));
        if let Some(control) = control {
            self.insert_control(&control.to_string());
        }
    }
}
