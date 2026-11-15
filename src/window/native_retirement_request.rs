//! Reserve without fencing; completion is owned by explicit NativeOperation epilogues.
mod coordinator;
mod transaction;

use std::{cell::RefCell, rc::Rc};

use gpui::AnyWindowHandle;
use objc2::MainThreadMarker;

use super::{Window, scene, state::press_ledger};
use crate::{
    NativeInputOrigin, NativeRetirementInterruption, NativeRetirementRequest,
    NativeRetirementRequestError as Error, NativeRetirementRequestStatus as Status,
    PrepareDecision, PreparedNativeRetirement, RequestKey, native_retirement_request::ReceiptState,
};

pub(super) type Prepare = Box<dyn FnOnce(PreparedNativeRetirement) -> PrepareDecision>;

fn find(handle: AnyWindowHandle, mtm: MainThreadMarker) -> Option<Rc<Window>> {
    scene::live_windows(mtm)
        .into_iter()
        .find(|w| w.handle == handle && !w.state.borrow().closed)
}

pub(crate) fn native_input_origin(
    handle: AnyWindowHandle,
    mtm: MainThreadMarker,
) -> Option<NativeInputOrigin> {
    let window = find(handle, mtm)?;
    Some(window.text_input.captured_native().native_target().origin())
}

/// A press is held by the Window's ledger or, failing that, by the text view's
/// key state (the second witness). Requests are refused while either says so;
/// a press frame still refuses on its own (`native_operation::admission`).
pub(super) fn hardware_held(window: &Window) -> Option<press_ledger::HeldReason> {
    let witness = window.text_input.captured_native().key_witness();
    press_ledger::held(&window.state.borrow().press_ledger, witness)
}

impl Window {
    /// Observation only; see [`crate::NativeHardwareHold`].
    pub(crate) fn native_hardware_hold(
        handle: AnyWindowHandle,
        mtm: MainThreadMarker,
    ) -> Option<crate::NativeHardwareHold> {
        let window = find(handle, mtm)?;
        let witness = window.text_input.captured_native().key_witness();
        let state = window.state.borrow();
        let summary =
            press_ledger::summary(&state.press_ledger, witness, std::time::Instant::now());
        Some(crate::NativeHardwareHold {
            keyed: summary.keyed,
            unkeyed: summary.unkeyed,
            overflow: summary.overflow,
            key_state_only: summary.key_state_only,
            oldest: summary.oldest,
            observed_ever: state.native_hardware_observed,
        })
    }
}

pub(crate) fn request_native_edit_retirement(
    handle: AnyWindowHandle,
    mtm: MainThreadMarker,
    expected: NativeInputOrigin,
    prepare: Prepare,
) -> Result<NativeRetirementRequest, Error> {
    let window = find(handle, mtm).ok_or(Error::WindowUnavailable)?;
    let old = window.text_input.captured_native();
    let origin = old.native_target().origin();
    if expected != origin {
        return Err(Error::StaleOrigin);
    }
    let serial = {
        let state = window.state.borrow();
        if state.native_request_pending.is_some() || window.text_input.replacing.get() {
            return Err(Error::InProgress);
        }
        if hardware_held(&window).is_some() {
            return Err(Error::UnsupportedHardwareContext);
        }
        state
            .native_request_serial
            .checked_add(1)
            .ok_or(Error::IdentityExhausted)?
    };
    if coordinator::busy() {
        return Err(Error::NewCapacityBusy);
    }
    if crate::gpui_call::legacy_busy() {
        return Err(Error::LegacyDispatchActive);
    }
    crate::native_operation::admission(&origin)?;
    origin
        .client
        .checked_add(1)
        .ok_or(Error::IdentityExhausted)?;
    let old_fenced_at_submission = !old.native_target().client.live();
    let receipt = NativeRetirementRequest {
        key: RequestKey { origin, serial },
        state: Rc::new(RefCell::new(ReceiptState {
            status: Status::Queued {
                old_fenced_at_submission,
            },
            owner_cancelled: false,
        })),
    };
    // All fallible validation preceded these reservation-only mutations. No
    // successor allocation, old fence, focus request or native cancellation here.
    {
        let mut state = window.state.borrow_mut();
        state.native_request_serial = serial;
        state.native_request_pending = Some(serial);
    }
    window.text_input.replacing.set(true);
    coordinator::reserve(coordinator::Reservation {
        window: Rc::downgrade(&window),
        old,
        receipt: receipt.clone(),
        old_fenced_at_submission,
        prepare: Some(prepare),
        interrupted: None,
    });
    Ok(receipt)
}

/// New actual entry interrupts before its original body, but terminal publication
/// waits for the enclosing operation's safe epilogue. No token is reopened.
pub(crate) fn interrupt_native_retirement() {
    coordinator::interrupt(NativeRetirementInterruption::NativeEntry);
}

/// Called only AFTER legacy accepted/fenced a different window, BEFORE queuing
/// its AFTER closure. Refused legacy calls never reach this hook.
pub(crate) fn legacy_interrupt_native_retirement() {
    coordinator::legacy_interrupt();
}

pub(crate) fn complete_native_retirement() {
    if !crate::gpui_call::legacy_busy() {
        transaction::complete_one();
    }
}
