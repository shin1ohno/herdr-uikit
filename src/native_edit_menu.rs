//! Explicit per-window native Copy/Paste menus and their synchronous origin.
use gpui::{AnyWindowHandle, Pixels, Point};
use objc2::MainThreadMarker;
use std::cell::Cell;

pub use crate::window::edit_menu::state::{
    EDIT_MENU_IDENTIFIER_PREFIX, EDIT_MENU_PROPERTY_KEY, NativeEditMenuStatus, NativeEditMenuToken,
    RevokeReason,
};

thread_local! {
    static CURRENT: Cell<Option<NativeEditMenuToken>> = const { Cell::new(None) };
}

/// Valid only during native key-down/key-up delivery; never inherited by tasks.
pub fn native_edit_menu_token() -> Option<NativeEditMenuToken> {
    CURRENT.get()
}

pub(crate) struct NativeEditMenuGuard(Option<NativeEditMenuToken>);
impl NativeEditMenuGuard {
    pub(crate) fn enter(token: Option<NativeEditMenuToken>) -> Self {
        Self(CURRENT.replace(token))
    }
}
impl Drop for NativeEditMenuGuard {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

/// Requests a menu in the drawing view's logical point coordinates.
pub fn present_native_edit_menu(
    window: AnyWindowHandle,
    at: Point<Pixels>,
) -> Option<NativeEditMenuToken> {
    MainThreadMarker::new()?;
    crate::window::edit_menu::present(window, at)
}

/// A stale token cannot dismiss a newer presentation.
pub fn dismiss_native_edit_menu(window: AnyWindowHandle, token: NativeEditMenuToken) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    crate::window::edit_menu::dismiss(window, token)
}

pub fn native_edit_menu_status(
    window: AnyWindowHandle,
    token: NativeEditMenuToken,
) -> NativeEditMenuStatus {
    if MainThreadMarker::new().is_none() {
        return NativeEditMenuStatus::Unknown;
    }
    crate::window::edit_menu::status(window, token)
}

/// Independent inputs to the responder-switch decision (Simulator QA only).
#[cfg(target_abi = "sim")]
#[derive(Clone, Debug)]
pub struct NativeEditMenuSwitchObservation {
    pub restart_reached: bool,
    pub acquire_returns: Vec<bool>,
    pub drawing_owned: bool,
    pub text_owned: bool,
}

/// Drains switch observations; the overflow count must be zero for a QA verdict.
#[cfg(target_abi = "sim")]
pub fn take_native_edit_menu_switch_observations(
    window: AnyWindowHandle,
) -> (usize, Vec<NativeEditMenuSwitchObservation>) {
    if MainThreadMarker::new().is_none() {
        return (0, Vec::new());
    }
    crate::window::edit_menu::take_observations(window)
}
