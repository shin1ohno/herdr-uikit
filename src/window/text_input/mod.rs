//! Keyboard input through a dedicated, invisible `UIView` subclass that adopts
//! `UIKeyInput` and `UITextInput` and becomes first responder while GPUI's
//! focused element accepts text, so the software keyboard, hardware keyboards,
//! dictation, and IME composition (marked text) all reach the window's
//! `PlatformInputHandler`. Hardware key presses (`pressesBegan:withEvent:`
//! and friends) also arrive here and become `KeyDown`, `KeyUp`, and
//! `ModifiersChanged`, so shortcuts work with and without a text field.
//!
//! The window owns one `TextInputView` as a subview of its `GPUIView` and
//! calls the methods below; this module owns everything else about text.

// Wave 0 skeleton: the text input stream replaces this.
#![allow(dead_code, clippy::todo)]

use gpui::{Bounds, Pixels};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_ui_kit::UIView;

use super::input_target::InputTarget;

/// The window's handle to its text input view.
pub(crate) struct TextInputView {}

impl TextInputView {
    pub(crate) fn new(_mtm: MainThreadMarker, _target: InputTarget) -> Self {
        todo!("text input stream")
    }

    /// The view to add as a subview of the window's `GPUIView`.
    pub(crate) fn view(&self) -> Retained<UIView> {
        todo!("text input stream")
    }

    /// Becomes first responder, which shows the software keyboard when no
    /// hardware keyboard is attached. The window calls this when GPUI sets an
    /// input handler and the window is active.
    pub(crate) fn activate(&self) {
        todo!("text input stream")
    }

    /// Resigns first responder when the focused element stops taking text.
    /// Hardware shortcuts keep arriving through the window's key commands.
    pub(crate) fn deactivate(&self) {
        todo!("text input stream")
    }

    /// GPUI replaced or changed the input handler, so cached text, selection,
    /// and marked-text state must be re-read and UIKit told
    /// (`textWillChange` / `textDidChange`).
    pub(crate) fn input_handler_changed(&self) {
        todo!("text input stream")
    }

    /// Where the caret is, in window coordinates, for the IME candidate
    /// window and the software keyboard's avoidance.
    pub(crate) fn caret_bounds_changed(&self, _bounds: Bounds<Pixels>) {
        todo!("text input stream")
    }
}
