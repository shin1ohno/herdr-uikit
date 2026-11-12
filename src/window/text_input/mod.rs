//! Keyboard input through a dedicated, invisible `UIView` subclass,
//! `GPUITextInputView`, that adopts `UIKeyInput` and `UITextInput`. It is
//! UIKit's text input client for one window, so the software keyboard,
//! hardware keyboards, dictation, and IME composition (marked text, such as
//! Japanese kana-kanji conversion) all reach the window's
//! `PlatformInputHandler`. Hardware key presses (`pressesBegan:withEvent:`
//! and friends) arrive here too and become `KeyDown`, `KeyUp`, and
//! `ModifiersChanged`, so shortcuts work with and without a text field.
//!
//! In text mode the view is first responder and its `inputView` is nil, so
//! UIKit shows the software keyboard when no hardware keyboard is attached.
//! In key-only mode the drawing view owns first responder and forwards
//! hardware presses here; resigning this text client also removes UIKit's
//! input-assistant bar. The text view takes first responder only when the
//! window asks for text mode or it moves into a window with text mode still
//! requested. When UIKit gives first responder to someone else (the keyboard's
//! dismiss key, an alert's or a picker's text field), it leaves it there until
//! the window asks again.
//!
//! The window owns one `TextInputView` as a subview of its `GPUIView` and
//! calls the methods below, often while GPUI is drawing. They only record
//! what the window wants and queue one flush on the run loop, because UIKit
//! answers first-responder and delegate calls by asking the text input for
//! its state, which needs the input handler GPUI is holding at that moment.

mod accessibility;
mod actions;
mod editing;
mod geometry;
mod keymap;
mod keys;
mod positions;
#[cfg(feature = "test-support")]
pub(super) mod probe;
mod query;
mod retirement;
mod run_loop;
mod state;
mod sync;
mod traits;
mod utf16;
mod view_class;

#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};

use gpui::{Bounds, Pixels, TextInputConfiguration};
use objc2::{DefinedClass, MainThreadMarker, rc::Retained, runtime::Sel};
use objc2_ui_kit::UIView;

pub(crate) use self::keymap::{named_key_char, us_shifted};
use super::{input_target::InputTarget, keyboard::ResignReason};
use crate::menu::Shortcut;
use state::Mode;
use traits::Traits;
use view_class::GPUITextInputView;

/// The window's handle to its text input view.
pub(crate) struct TextInputView {
    view: RefCell<Retained<GPUITextInputView>>,
    pub(super) replacing: Cell<bool>,
}

impl TextInputView {
    fn current(&self) -> Retained<GPUITextInputView> {
        self.view.borrow().clone()
    }

    pub(crate) fn new(mtm: MainThreadMarker, target: InputTarget) -> Self {
        Self {
            view: RefCell::new(GPUITextInputView::new(mtm, target)),
            replacing: Cell::new(false),
        }
    }

    /// The view to add as a subview of the window's `GPUIView`. It is
    /// at the drawing view's origin with matching bounds. Its hit-test
    /// override takes no touches, and its coordinates are the `GPUIView`'s,
    /// which are GPUI's window coordinates.
    pub(crate) fn view(&self) -> Retained<UIView> {
        Retained::into_super(self.current())
    }

    pub(super) fn editing_enabled(&self, selector: Sel) -> bool {
        self.current().editing_enabled(selector)
    }

    pub(super) fn perform_edit_action(&self, selector: Sel) {
        self.current().perform_edit_action(selector);
    }

    pub(super) fn validate_edit_command(&self, command: &objc2_ui_kit::UICommand) {
        self.current().validate_edit_command(command);
    }

    /// Text mode: the view becomes first responder with no `inputView`, so
    /// the software keyboard shows when no hardware keyboard is attached. The
    /// window calls this when the focused element starts taking text and the
    /// window is active, or when GPUI asks for the software keyboard. It
    /// takes first responder back from wherever UIKit gave it, so the window
    /// also calls it when the user taps a text element after dismissing the
    /// keyboard.
    pub(crate) fn activate(&self) {
        self.current().request_mode(Mode::Text);
    }

    /// Key-only mode: the window gives first responder to the drawing view,
    /// which forwards hardware presses for shortcuts. This text client does
    /// not acquire first responder, so UIKit removes its keyboard and input
    /// assistant. The window calls this when text focus ends or GPUI dismisses
    /// the software keyboard.
    pub(crate) fn deactivate(&self) {
        self.current().request_mode(Mode::KeyOnly);
    }

    /// Marks an intentional native resignation before UIKit can synchronously
    /// deliver its hide notifications or responder callback.
    pub(super) fn resign(&self, reason: ResignReason) {
        if self.current().isFirstResponder() {
            self.current().ivars().target.expect_keyboard_hide(reason);
            self.current().resignFirstResponder();
        }
    }

    /// A menu command with `shortcut` was performed; see
    /// `GPUITextInputView::menu_shortcut`. Returns whether the key press
    /// was GPUI's to handle, so the command's action must not run.
    pub(crate) fn menu_shortcut(&self, shortcut: &Shortcut) -> bool {
        self.current().menu_shortcut(shortcut)
    }

    /// GPUI replaced or changed the input handler. The flush re-reads the
    /// selection, marked text, and length, and tells UIKit's input delegate
    /// (`textWillChange:`/`textDidChange:`, or the selection pair) only when
    /// they differ from what UIKit last saw, so a composition in progress is
    /// not reset by redundant notifications. GPUI sets a handler on every
    /// drawn frame, so this never takes first responder; only `activate`
    /// and `deactivate` do.
    pub(crate) fn input_handler_changed(&self) {
        self.current().schedule_flush();
    }

    /// Where the caret is, in window coordinates. Kept for
    /// `caretRectForPosition:` and `firstRectForRange:` when the handler
    /// cannot place a range, so the IME candidate window and the software
    /// keyboard's avoidance follow the caret; the selection it implies is
    /// re-read and reported to UIKit.
    pub(crate) fn caret_bounds_changed(&self, bounds: Bounds<Pixels>) {
        self.current().set_caret(bounds);
    }

    /// The focused element's text assistance: autocorrection, capitalization,
    /// suggestions, and the return key's label. Smart punctuation stays off.
    /// The window forwards `PlatformWindow::set_text_input_configuration`.
    pub(crate) fn set_text_input_configuration(&self, configuration: &TextInputConfiguration) {
        self.current().request_traits(Traits::from(configuration));
    }
}

impl Drop for TextInputView {
    /// Queued flushes and key repeats retain the view; cancel them so a
    /// closed window's view is not kept alive or driven afterwards.
    fn drop(&mut self) {
        self.current().cancel_pending();
    }
}
