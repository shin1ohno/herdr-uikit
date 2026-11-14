//! Locate the consumer's window without keeping the scene registry borrowed.

use gpui::AnyWindowHandle;
use objc2::MainThreadMarker;

use super::scene;
use crate::{
    RetiredTextInputSession, TextInputRetirement, TextInputRetirementError,
    TextInputRetirementStatus,
};

pub(crate) fn retire_text_input_session(
    handle: AnyWindowHandle,
    mtm: MainThreadMarker,
    retired: Box<dyn FnOnce(RetiredTextInputSession)>,
) -> Result<TextInputRetirement, TextInputRetirementError> {
    let window = scene::live_windows(mtm)
        .into_iter()
        .find(|window| window.handle == handle && !window.state.borrow().closed)
        .ok_or(TextInputRetirementError::WindowUnavailable)?;
    if window.state.borrow().native_request_pending.is_some() {
        return Err(TextInputRetirementError::InProgress);
    }
    let (old, next, receipt) = window.text_input.begin_retirement()?;
    super::legacy_interrupt_native_retirement();
    let result = receipt.clone();
    let weak_window = std::rc::Rc::downgrade(&window);
    crate::gpui_call::after_update(move || {
        let Some(window) = weak_window.upgrade().filter(|w| !w.state.borrow().closed) else {
            receipt.status.set(TextInputRetirementStatus::WindowClosed);
            return;
        };
        // The old native object is already closed. Reopen the consumer before
        // any replacement UIKit query/edit can synchronously reach it.
        {
            let _in_gpui = crate::gpui_call::InGpui::enter();
            retired(receipt.sessions);
        }
        if window.state.borrow().closed {
            receipt.status.set(TextInputRetirementStatus::WindowClosed);
            window.text_input.replacing.set(false);
            return;
        }
        window.text_input.finish_retirement(old, next, &receipt);
    });
    Ok(result)
}

pub(crate) fn text_input_session_state(
    handle: AnyWindowHandle,
    mtm: MainThreadMarker,
) -> Option<crate::TextInputSessionState> {
    let window = scene::live_windows(mtm)
        .into_iter()
        .find(|window| window.handle == handle && !window.state.borrow().closed)?;
    Some(window.text_input.session_state())
}
