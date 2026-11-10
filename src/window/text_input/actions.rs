//! UIKit's standard editing commands use the app's key bindings, so terminal
//! paste retains the app's clipboard handling and bracketed-paste policy.

#![allow(
    unsafe_code,
    reason = "UIPasteboard's non-atomic properties are used only on the main thread"
)]

use gpui::{
    Capslock, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ModifiersChangedEvent, PlatformInput,
};
use objc2::{DefinedClass, runtime::Sel, sel};
use objc2_ui_kit::UIPasteboard;

use super::state::Mode;
use super::view_class::GPUITextInputView;

pub(super) use crate::NativeEditAction as EditAction;
use crate::native_edit_action::NativeEditActionGuard;

impl EditAction {
    pub(super) fn from_selector(selector: Sel) -> Option<Self> {
        if selector == sel!(copy:) {
            Some(Self::Copy)
        } else if selector == sel!(cut:) {
            Some(Self::Cut)
        } else if selector == sel!(paste:) {
            Some(Self::Paste)
        } else if selector == sel!(selectAll:) {
            Some(Self::SelectAll)
        } else {
            None
        }
    }

    fn keystroke(self) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                platform: true,
                ..Modifiers::default()
            },
            key: match self {
                Self::Copy => "c",
                Self::Cut => "x",
                Self::Paste => "v",
                Self::SelectAll => "a",
            }
            .into(),
            key_char: None,
        }
    }
}

impl GPUITextInputView {
    pub(super) fn editing_enabled(&self) -> bool {
        self.ivars().mode.get() == Some(Mode::Text) && self.ivars().target.text_input_focused()
    }

    pub(super) fn perform_edit_action(&self, selector: Sel) {
        if !self.editing_enabled() {
            return;
        }
        dispatch_edit_selector(
            selector,
            self.ivars().target.modifiers(),
            |input| self.ivars().target.dispatch(input),
            || {
                let pasteboard = UIPasteboard::generalPasteboard();
                // SAFETY: this UIView is main-thread-only, so its pasteboard
                // access is serialized with the platform's clipboard access.
                if !unsafe { pasteboard.hasStrings() } {
                    return;
                }
                // SAFETY: as above; this is an explicit user paste action.
                if let Some(text) = unsafe { pasteboard.string() } {
                    self.with_handler(|handler| {
                        handler.replace_text_in_range(None, &text.to_string());
                    });
                }
            },
        );
        self.refresh_snapshot();
    }
}

// Shared by the actual responder selectors and the dispatch regression tests.
fn dispatch_edit_selector(
    selector: Sel,
    original: (Modifiers, Capslock),
    dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
) {
    if let Some(action) = EditAction::from_selector(selector) {
        dispatch_edit_action(action, original, dispatch, paste);
    }
}

fn dispatch_edit_action(
    action: EditAction,
    original: (Modifiers, Capslock),
    mut dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
) {
    let keystroke = action.keystroke();
    let origin = NativeEditActionGuard::enter(Some(action));
    let handled = dispatch(PlatformInput::KeyDown(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    }));
    dispatch(PlatformInput::KeyUp(KeyUpEvent { keystroke }));
    drop(origin);
    dispatch(PlatformInput::ModifiersChanged(ModifiersChangedEvent {
        modifiers: original.0,
        capslock: original.1,
    }));
    if action == EditAction::Paste && !handled {
        paste();
    }
}

#[cfg(test)]
mod tests;
