//! Whole native-operation ownership. No completion is run by a Drop hook.
//! Only explicit normal-return Edit/Event/DispatcherTask epilogues may drain
//! one new request, after body locals and all counted loans have been released.
#[cfg(feature = "test-support")]
pub(crate) mod probe;
mod state;

use crate::{NativeInputOrigin, NativeRetirementRequestError};
use state::{Frame, Kind, Loan};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    EditDispatch,
    PasteFallback,
    ObserveSnapshot,
}

/// Unexported capability passed by the SDK's exact Edit continuation call sites.
/// A new native selector never receives this by inheritance or by object/depth.
pub(crate) struct NativeOperation {
    serial: Option<u64>,
}

impl NativeOperation {
    pub(crate) fn edit<R>(body: impl FnOnce(&Self) -> R) -> R {
        crate::window::interrupt_native_retirement();
        let frame = Frame::enter(Kind::Edit);
        let operation = Self {
            serial: frame.serial(),
        };
        let result = body(&operation);
        drop(frame);
        complete();
        result
    }

    pub(crate) fn phase<R>(
        &self,
        origin: NativeInputOrigin,
        phase: Phase,
        body: impl FnOnce() -> R,
    ) -> R {
        let _phase = state::PhaseScope::enter(self.serial, origin, phase);
        body()
    }
}

/// Refusal envelopes preserve the existing body. Drop only restores counters;
/// an enclosing explicit safe epilogue terminalizes any interrupted reservation.
pub(crate) fn refused() -> impl Drop {
    crate::window::interrupt_native_retirement();
    Frame::enter(Kind::Refused)
}

pub(crate) fn raw(label: &'static str) -> impl Drop {
    let entry = refused();
    raw_observe(label);
    entry
}
#[inline]
pub(crate) fn raw_observe(_label: &'static str) {
    #[cfg(feature = "test-support")]
    probe::observe(_label);
}

pub(crate) fn event<R>(presses: bool, body: impl FnOnce() -> R) -> R {
    crate::window::interrupt_native_retirement();
    let frame = Frame::enter(if presses {
        Kind::PressesEvent
    } else {
        Kind::Event
    });
    let result = body();
    drop(frame);
    complete();
    result
}

pub(crate) fn dispatcher_task<R>(body: impl FnOnce() -> R) -> R {
    crate::window::interrupt_native_retirement();
    let frame = Frame::enter(Kind::Task);
    let result = body();
    drop(frame);
    complete();
    result
}

pub(crate) fn loan() -> impl Drop {
    Loan::enter()
}

pub(crate) fn hardware_event() -> bool {
    state::hardware_event()
}

pub(crate) fn admission(origin: &NativeInputOrigin) -> Result<(), NativeRetirementRequestError> {
    state::admission(origin)
}

fn complete() {
    if !std::thread::panicking() && state::quiescent() && !crate::gpui_call::is_running() {
        crate::window::complete_native_retirement();
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use state::loan_count_for_test;
