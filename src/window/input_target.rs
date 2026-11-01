//! What the text input view needs from its window: a way to dispatch
//! `PlatformInput` and to reach the input handler GPUI registered for the
//! focused element. The window implements it over its state; the text input
//! view only calls it.

use std::rc::Weak;

use gpui::{Capslock, Modifiers, PlatformInput, PlatformInputHandler};

use super::{
    keyboard::{ResignContext, ResignReason},
    state::{Window, call},
};

/// A weak handle to one window's event callback and input handler. Cloning is
/// cheap; every method is a no-op once the window has closed.
#[derive(Clone)]
pub(crate) struct InputTarget {
    window: Weak<Window>,
}

impl InputTarget {
    pub(super) fn new(window: Weak<Window>) -> Self {
        Self { window }
    }
}

impl InputTarget {
    /// Sends `input` through the window's event callback. Returns whether
    /// GPUI handled it (`DispatchEventResult::propagate == false`).
    pub(crate) fn dispatch(&self, input: PlatformInput) -> bool {
        self.window
            .upgrade()
            .is_some_and(|window| window.dispatch(input))
    }

    /// Runs `f` with the focused element's input handler, if GPUI has
    /// registered one, and returns its result.
    pub(crate) fn with_input_handler<R>(
        &self,
        f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        self.window.upgrade()?.with_input_handler(f)
    }

    /// The window's scale factor, to convert between points and GPUI pixels.
    pub(crate) fn scale_factor(&self) -> f32 {
        self.window
            .upgrade()
            .map_or(1.0, |window| window.state.borrow().scale_factor)
    }

    pub(crate) fn accessibility_label(&self) -> String {
        self.window
            .upgrade()
            .map_or_else(String::new, |window| window.state.borrow().title.clone())
    }

    pub(crate) fn text_input_focused(&self) -> bool {
        self.window
            .upgrade()
            .is_some_and(|window| window.state.borrow().text_input_focused)
    }

    pub(crate) fn modifiers(&self) -> (Modifiers, Capslock) {
        self.window.upgrade().map_or_else(
            || (Modifiers::default(), Capslock::default()),
            |window| {
                let state = window.state.borrow();
                (state.modifiers, state.capslock)
            },
        )
    }

    /// The drawing view receives and forwards hardware keys in key-only
    /// mode; repeats must work with either of the window's responders.
    pub(crate) fn has_key_responder(&self) -> bool {
        self.window.upgrade().is_some_and(|window| {
            window.text_input.view().isFirstResponder() || window.view.isFirstResponder()
        })
    }

    /// Keyboard visibility can change without a frame notification when
    /// UIKit switches text clients while the keyboard is already up.
    pub(crate) fn reconcile_keyboard_layout(&self) {
        if let Some(window) = self.window.upgrade() {
            window.reconcile_keyboard_layout();
        }
    }

    pub(super) fn keyboard_needs_responder_restart(&self) -> bool {
        self.window
            .upgrade()
            .is_some_and(|window| window.state.borrow().keyboard.minimized)
    }

    pub(super) fn text_input_resign_context(&self, first_responder: bool) -> ResignContext {
        self.window
            .upgrade()
            .map_or_else(ResignContext::default, |window| {
                window.keyboard_resign_context(first_responder)
            })
    }

    pub(super) fn text_input_did_resign(&self, context: ResignContext) {
        if let Some(window) = self.window.upgrade() {
            window.state.borrow_mut().keyboard.resigned(context);
        }
    }

    pub(super) fn expect_keyboard_hide(&self, reason: ResignReason) {
        if let Some(window) = self.window.upgrade() {
            window.state.borrow_mut().keyboard.expect_hide(reason);
        }
    }

    pub(super) fn text_input_did_acquire(&self) {
        if let Some(window) = self.window.upgrade() {
            window.state.borrow_mut().keyboard.acquired();
        }
    }

    pub(super) fn text_input_can_acquire(&self) -> bool {
        self.window.upgrade().is_some_and(|window| {
            let state = window.state.borrow();
            !state.closed
                && state.is_active
                && state.text_input_focused
                && state.keyboard.dismissal.allows_text()
        })
    }
}

impl Window {
    /// Sends `input` to GPUI and returns whether it was handled. Modifier
    /// state is recorded first, so `PlatformWindow::modifiers` agrees with the
    /// event GPUI is handling.
    pub(super) fn dispatch(&self, input: PlatformInput) -> bool {
        {
            let mut state = self.state.borrow_mut();
            if state.closed {
                return false;
            }
            match &input {
                PlatformInput::ModifiersChanged(event) => {
                    state.modifiers = event.modifiers;
                    state.capslock = event.capslock;
                }
                PlatformInput::KeyDown(event) => state.modifiers = event.keystroke.modifiers,
                PlatformInput::KeyUp(event) => state.modifiers = event.keystroke.modifiers,
                _ => {}
            }
        }
        call(&self.callbacks.input, |callback| !callback(input).propagate).unwrap_or(false)
    }

    /// Lends the input handler to `f`. It is taken out of the state while `f`
    /// runs, as GPUI may call back into this window from the handler; a
    /// handler GPUI registered meanwhile is newer and wins.
    pub(super) fn with_input_handler<R>(
        &self,
        f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        let mut handler = self.state.borrow_mut().input_handler.take()?;
        let result = f(&mut handler);
        let mut state = self.state.borrow_mut();
        if state.input_handler.is_none() && !state.closed {
            state.input_handler = Some(handler);
        }
        Some(result)
    }
}
