//! What the text input view needs from its window: a way to dispatch
//! `PlatformInput` and to reach the input handler GPUI registered for the
//! focused element. The window stream implements it over the window's state;
//! the text input stream only calls it.

// Wave 0 skeleton: the window stream replaces these bodies.
#![allow(dead_code, clippy::todo)]

use gpui::{PlatformInput, PlatformInputHandler};

/// A weak handle to one window's event callback and input handler. Cloning is
/// cheap; every method is a no-op once the window has closed.
#[derive(Clone)]
pub(crate) struct InputTarget {}

impl InputTarget {
    /// Sends `input` through the window's event callback. Returns whether
    /// GPUI handled it (`DispatchEventResult::propagate == false`).
    pub(crate) fn dispatch(&self, _input: PlatformInput) -> bool {
        todo!("window stream")
    }

    /// Runs `f` with the focused element's input handler, if GPUI has
    /// registered one, and returns its result.
    pub(crate) fn with_input_handler<R>(
        &self,
        _f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        todo!("window stream")
    }

    /// The window's scale factor, to convert between points and GPUI pixels.
    pub(crate) fn scale_factor(&self) -> f32 {
        todo!("window stream")
    }
}
