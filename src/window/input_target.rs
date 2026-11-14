//! What the text input view needs from its window: a way to dispatch
//! `PlatformInput` and to reach the input handler GPUI registered for the
//! focused element. The window implements it over its state; the text input
//! view only calls it.

use std::rc::{Rc, Weak};

use super::client_lifetime::ClientLifetime;

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
    pub(crate) client: Rc<ClientLifetime>,
    #[cfg(all(test, feature = "test-support"))]
    test_handler: Option<Rc<std::cell::RefCell<Option<PlatformInputHandler>>>>,
}

impl InputTarget {
    pub(crate) fn origin(&self) -> crate::NativeInputOrigin {
        crate::NativeInputOrigin {
            window: self.window.clone(),
            client: self.client.identity,
        }
    }
    pub(crate) fn mark_hardware_observed(&self) {
        if let Some(window) = self.window.upgrade() {
            window.state.borrow_mut().native_hardware_observed = true;
        }
    }
    pub(super) fn new(window: Weak<Window>) -> Self {
        Self {
            window,
            client: Rc::new(ClientLifetime::default()),
            #[cfg(all(test, feature = "test-support"))]
            test_handler: None,
        }
    }
}

impl InputTarget {
    #[cfg(all(test, feature = "test-support"))]
    pub(crate) fn with_test_handler(handler: PlatformInputHandler) -> Self {
        Self {
            test_handler: Some(Rc::new(std::cell::RefCell::new(Some(handler)))),
            ..Self::new(Weak::new())
        }
    }

    fn live_window(&self) -> Option<Rc<Window>> {
        if !self.client.live() {
            return None;
        }
        self.window.upgrade()
    }

    pub(crate) fn successor(&self) -> Option<Self> {
        Some(Self {
            window: self.window.clone(),
            client: self.client.successor()?,
            #[cfg(all(test, feature = "test-support"))]
            test_handler: self.test_handler.clone(),
        })
    }

    pub(super) fn menu_switch(
        &self,
    ) -> Option<super::edit_menu::responder_switch::ResponderSwitch<Rc<Window>>> {
        Some(super::edit_menu::responder_switch::ResponderSwitch::new(
            self.window.upgrade()?,
        ))
    }
    #[cfg(target_abi = "sim")]
    pub(super) fn menu_restart(&self) {
        if let Some(window) = self.window.upgrade() {
            window.edit_menu.observation.borrow_mut().restart = true;
        }
    }
    pub(super) fn menu_resigned(&self) {
        if let Some(window) = self.window.upgrade() {
            window.edit_menu.resigned();
        }
    }
    pub(super) fn menu_delivery(
        &self,
        sender: Option<&objc2::runtime::AnyObject>,
    ) -> super::edit_menu::state::Delivery {
        let sender = super::edit_menu::classify(sender);
        self.window
            .upgrade()
            .map_or(super::edit_menu::state::Delivery::Reject, |w| {
                w.edit_menu.state.borrow().delivery(sender)
            })
    }
    /// Sends `input` through the window's event callback. Returns whether
    /// GPUI handled it (`DispatchEventResult::propagate == false`).
    pub(crate) fn dispatch(&self, input: PlatformInput) -> bool {
        let _entry = crate::native_operation::refused();
        if matches!(&input, PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_)) {
            self.mark_hardware_observed();
        }
        self.dispatch_inner(input)
    }
    /// The native Edit producer without observation; the observed build uses
    /// `dispatch_native_edit`.
    #[cfg(not(all(target_os = "ios", target_abi = "sim", feature = "test-support")))]
    pub(crate) fn dispatch_edit(
        &self,
        op: &crate::native_operation::NativeOperation,
        input: PlatformInput,
    ) -> bool {
        op.phase(
            self.origin(),
            crate::native_operation::Phase::EditDispatch,
            || self.dispatch_inner(input),
        )
    }
    fn dispatch_inner(&self, input: PlatformInput) -> bool {
        let Some(_callback) = self.client.enter() else {
            return false;
        };
        self.live_window()
            .is_some_and(|window| window.dispatch_inner(input))
    }

    /// Only the canonical native edit producer supplies an observation key.
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    pub(crate) fn dispatch_native_edit(
        &self,
        op: &crate::native_operation::NativeOperation,
        input: PlatformInput,
        key: Option<crate::native_edit_observer::ObservationKey>,
    ) -> bool {
        op.phase(
            self.origin(),
            crate::native_operation::Phase::EditDispatch,
            || {
                let Some(_callback) = self.client.enter() else {
                    return false;
                };
                self.live_window()
                    .is_some_and(|window| window.dispatch_native_edit(input, key))
            },
        )
    }

    /// Runs `f` with the focused element's input handler, if GPUI has
    /// registered one, and returns its result.
    pub(crate) fn with_input_handler<R>(
        &self,
        f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        let _entry = crate::native_operation::refused();
        self.with_input_handler_inner(f)
    }
    pub(crate) fn with_edit_handler<R>(
        &self,
        op: &crate::native_operation::NativeOperation,
        phase: crate::native_operation::Phase,
        f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        op.phase(self.origin(), phase, || self.with_input_handler_inner(f))
    }
    fn with_input_handler_inner<R>(
        &self,
        f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        let _loan = crate::native_operation::loan();
        let _callback = self.client.enter()?;
        #[cfg(all(test, feature = "test-support"))]
        if let Some(slot) = &self.test_handler {
            let mut handler = slot.borrow_mut().take()?;
            let _in_gpui = crate::gpui_call::InGpui::enter();
            let result = f(&mut handler);
            slot.borrow_mut().get_or_insert(handler);
            return Some(result);
        }
        self.live_window()?.with_input_handler_inner(f)
    }

    /// The window's scale factor, to convert between points and GPUI pixels.
    pub(crate) fn scale_factor(&self) -> f32 {
        self.live_window()
            .map_or(1.0, |window| window.state.borrow().scale_factor)
    }

    pub(crate) fn accessibility_label(&self) -> String {
        self.live_window()
            .map_or_else(String::new, |window| window.state.borrow().title.clone())
    }

    pub(crate) fn text_input_focused(&self) -> bool {
        self.live_window()
            .is_some_and(|window| window.state.borrow().text_input_focused)
    }

    pub(crate) fn native_edit_availability(&self) -> Option<crate::NativeEditAvailability> {
        self.live_window()
            .and_then(|window| window.state.borrow().native_edit_availability)
    }

    pub(crate) fn edit_window_active(&self) -> bool {
        self.live_window().is_some_and(|window| {
            let state = window.state.borrow();
            !state.closed && state.is_active
        }) && self.has_key_responder()
    }

    pub(crate) fn modifiers(&self) -> (Modifiers, Capslock) {
        self.live_window().map_or_else(
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
        self.live_window().is_some_and(|window| {
            window.text_input.view().isFirstResponder() || window.view.isFirstResponder()
        })
    }

    /// Keyboard visibility can change without a frame notification when
    /// UIKit switches text clients while the keyboard is already up.
    pub(crate) fn reconcile_keyboard_layout(&self) {
        if let Some(window) = self.live_window() {
            window.reconcile_keyboard_layout();
        }
    }

    pub(super) fn keyboard_needs_responder_restart(&self) -> bool {
        self.live_window()
            .is_some_and(|window| window.state.borrow().keyboard.minimized)
    }

    pub(super) fn text_input_resign_context(&self, first_responder: bool) -> ResignContext {
        self.live_window()
            .map_or_else(ResignContext::default, |window| {
                window.keyboard_resign_context(first_responder)
            })
    }

    pub(super) fn text_input_did_resign(&self, context: ResignContext) {
        if let Some(window) = self.live_window() {
            window.state.borrow_mut().keyboard.resigned(context);
        }
    }

    pub(super) fn expect_keyboard_hide(&self, reason: ResignReason) {
        if let Some(window) = self.live_window() {
            window.state.borrow_mut().keyboard.expect_hide(reason);
        }
    }

    pub(super) fn text_input_did_acquire(&self) {
        if let Some(window) = self.live_window() {
            window.state.borrow_mut().keyboard.acquired();
        }
    }

    pub(super) fn text_input_can_acquire(&self) -> bool {
        self.live_window().is_some_and(|window| {
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
        let _entry = crate::native_operation::refused();
        if matches!(&input, PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_)) {
            self.state.borrow_mut().native_hardware_observed = true;
        }
        // Generic ingress (including synthetic keys) never inherits a native key.
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        let _observation = crate::native_edit_observer::KeyScope::enter(None);
        self.dispatch_inner(input)
    }

    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    fn dispatch_native_edit(
        &self,
        input: PlatformInput,
        key: Option<crate::native_edit_observer::ObservationKey>,
    ) -> bool {
        let key = if matches!(&input, PlatformInput::KeyDown(_) | PlatformInput::KeyUp(_)) {
            key
        } else {
            None
        };
        let _observation = crate::native_edit_observer::KeyScope::enter(key);
        self.dispatch_inner(input)
    }

    // The two ingress methods establish provenance before reaching callbacks.
    fn dispatch_inner(&self, input: PlatformInput) -> bool {
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
        let _entry = crate::native_operation::refused();
        self.with_input_handler_inner(f)
    }
    fn with_input_handler_inner<R>(
        &self,
        f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        let _loan = crate::native_operation::loan();
        let mut handler = self.state.borrow_mut().input_handler.take()?;
        // A handler can request retirement of another window, whose native
        // client is not busy. Keep its callback outside this GPUI borrow too.
        let _in_gpui = crate::gpui_call::InGpui::enter();
        let result = f(&mut handler);
        let mut state = self.state.borrow_mut();
        if state.input_handler.is_none() && !state.closed {
            state.input_handler = Some(handler);
        }
        Some(result)
    }
}
