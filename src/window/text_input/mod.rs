//! Keyboard input through a dedicated, invisible `UIView` subclass,
//! `GPUITextInputView`, that adopts `UIKeyInput` and `UITextInput`. It is
//! UIKit's text input client for one window, so the software keyboard,
//! hardware keyboards, dictation, and IME composition (marked text, such as
//! Japanese kana-kanji conversion) all reach the window's
//! `PlatformInputHandler`. Hardware key presses (`pressesBegan:withEvent:`
//! and friends) arrive here too and become `KeyDown`, `KeyUp`, and
//! `ModifiersChanged`, so shortcuts work with and without a text field.
//!
//! The view stays first responder in two modes. In text mode its `inputView`
//! is nil, so UIKit shows the software keyboard when no hardware keyboard is
//! attached. In key-only mode its `inputView` is an empty view, which hides
//! the software keyboard while hardware key presses keep reaching GPUI. It
//! takes first responder only when the window asks for a mode or it moves
//! into a window. When UIKit gives first responder to someone else (the
//! keyboard's dismiss key, an alert's or a picker's text field), the view
//! leaves it there until the window asks again.
//!
//! The window owns one `TextInputView` as a subview of its `GPUIView` and
//! calls the methods below, often while GPUI is drawing. They only record
//! what the window wants and queue one flush on the run loop, because UIKit
//! answers first-responder and delegate calls by asking the text input for
//! its state, which needs the input handler GPUI is holding at that moment.

mod editing;
mod geometry;
mod keymap;
mod keys;
mod positions;
mod query;
mod run_loop;
mod state;
mod sync;
mod traits;
mod utf16;
mod view_class;

#[cfg(test)]
mod tests;

use gpui::{Bounds, Pixels, TextInputConfiguration};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_ui_kit::UIView;

use super::input_target::InputTarget;
use state::Mode;
use traits::Traits;
use view_class::GPUITextInputView;

/// The window's handle to its text input view.
pub(crate) struct TextInputView {
    view: Retained<GPUITextInputView>,
}

impl TextInputView {
    pub(crate) fn new(mtm: MainThreadMarker, target: InputTarget) -> Self {
        Self {
            view: GPUITextInputView::new(mtm, target),
        }
    }

    /// The view to add as a subview of the window's `GPUIView`. It is
    /// zero-sized at the origin, so it takes no touches and its coordinates
    /// are the `GPUIView`'s, which are GPUI's window coordinates.
    pub(crate) fn view(&self) -> Retained<UIView> {
        Retained::into_super(self.view.clone())
    }

    /// Text mode: the view becomes first responder with no `inputView`, so
    /// the software keyboard shows when no hardware keyboard is attached. The
    /// window calls this when the focused element starts taking text and the
    /// window is active, or when GPUI asks for the software keyboard. It
    /// takes first responder back from wherever UIKit gave it, so the window
    /// also calls it when the user taps a text element after dismissing the
    /// keyboard.
    pub(crate) fn activate(&self) {
        self.view.request_mode(Mode::Text);
    }

    /// Key-only mode: the view stays (or becomes) first responder so hardware
    /// key presses still reach GPUI for shortcuts, but its `inputView` is an
    /// empty zero-height view and `reloadInputViews` hides the software
    /// keyboard. The window calls this when the focused element stops taking
    /// text, or when GPUI dismisses the software keyboard.
    pub(crate) fn deactivate(&self) {
        self.view.request_mode(Mode::KeyOnly);
    }

    /// GPUI replaced or changed the input handler. The flush re-reads the
    /// selection, marked text, and length, and tells UIKit's input delegate
    /// (`textWillChange:`/`textDidChange:`, or the selection pair) only when
    /// they differ from what UIKit last saw, so a composition in progress is
    /// not reset by redundant notifications. GPUI sets a handler on every
    /// drawn frame, so this never takes first responder; only `activate`
    /// and `deactivate` do.
    pub(crate) fn input_handler_changed(&self) {
        self.view.schedule_flush();
    }

    /// Where the caret is, in window coordinates. Kept for
    /// `caretRectForPosition:` and `firstRectForRange:` when the handler
    /// cannot place a range, so the IME candidate window and the software
    /// keyboard's avoidance follow the caret; the selection it implies is
    /// re-read and reported to UIKit.
    pub(crate) fn caret_bounds_changed(&self, bounds: Bounds<Pixels>) {
        self.view.set_caret(bounds);
    }

    /// The focused element's text assistance: autocorrection, capitalization,
    /// suggestions, and the return key's label. Smart punctuation stays off.
    /// The window forwards `PlatformWindow::set_text_input_configuration`.
    pub(crate) fn set_text_input_configuration(&self, configuration: &TextInputConfiguration) {
        self.view.request_traits(Traits::from(configuration));
    }
}

impl Drop for TextInputView {
    /// Queued flushes and key repeats retain the view; cancel them so a
    /// closed window's view is not kept alive or driven afterwards.
    fn drop(&mut self) {
        self.view.cancel_pending();
    }
}
