//! Limited, explicit native Edit retirement. Queued is a reservation, not a
//! fence or readiness acknowledgement. General text/hardware recovery is unsupported.
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use gpui::AnyWindowHandle;
use objc2::MainThreadMarker;

/// Immutable native origin, including the weak Window allocation identity.
/// Keeping this token does not keep a Window alive, but prevents identity reuse.
#[derive(Clone, Debug)]
pub struct NativeInputOrigin {
    pub(crate) window: Weak<crate::window::Window>,
    pub(crate) client: u64,
}

impl PartialEq for NativeInputOrigin {
    fn eq(&self, other: &Self) -> bool {
        self.client == other.client && self.window.ptr_eq(&other.window)
    }
}
impl Eq for NativeInputOrigin {}
impl NativeInputOrigin {
    pub fn client(&self) -> u64 {
        self.client
    }
}

/// A distinct accepted request, even when retrying the same fenced origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestKey {
    pub(crate) origin: NativeInputOrigin,
    pub(crate) serial: u64,
}
impl RequestKey {
    pub fn origin(&self) -> &NativeInputOrigin {
        &self.origin
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRetirementInterruption {
    NativeEntry,
    LegacyRequest,
}

/// A local state snapshot, never a UIKit delivery/cancellation guarantee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeRetirementRequestStatus {
    Queued {
        old_fenced_at_submission: bool,
    },
    Preparing,
    Replaced {
        successor: NativeInputOrigin,
        responder_requested: bool,
        first_responder: bool,
    },
    Declined {
        old_fenced: bool,
    },
    WindowClosed {
        old_fenced: bool,
    },
    /// Before THIS request's fence step; an earlier request may have fenced old.
    InterruptedBeforeFence {
        reason: NativeRetirementInterruption,
        old_fenced_at_submission: bool,
    },
    InvalidatedAfterFence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRetirementCancellation {
    Marked,
    TooLate,
    AlreadyTerminal,
}

#[derive(Debug)]
pub(crate) struct ReceiptState {
    pub(crate) status: NativeRetirementRequestStatus,
    pub(crate) owner_cancelled: bool,
}

/// Clones observe the same request; dropping all receipts never cancels work.
#[derive(Clone, Debug)]
pub struct NativeRetirementRequest {
    pub(crate) key: RequestKey,
    pub(crate) state: Rc<RefCell<ReceiptState>>,
}
impl NativeRetirementRequest {
    pub fn key(&self) -> &RequestKey {
        &self.key
    }
    pub fn status(&self) -> NativeRetirementRequestStatus {
        self.state.borrow().status.clone()
    }
    /// First invalidate the matching consumer owner ticket, then mark it here.
    /// Wait for terminal publication AND slot release before a new owner ticket.
    pub fn mark_owner_cancelled(&self) -> NativeRetirementCancellation {
        let mut state = self.state.borrow_mut();
        match state.status {
            NativeRetirementRequestStatus::Queued { .. } => {
                state.owner_cancelled = true;
                NativeRetirementCancellation::Marked
            }
            NativeRetirementRequestStatus::Preparing => NativeRetirementCancellation::TooLate,
            _ => NativeRetirementCancellation::AlreadyTerminal,
        }
    }
}

/// Old is permanently fenced; no successor has been constructed yet.
#[derive(Clone, Debug)]
pub struct PreparedNativeRetirement {
    pub key: RequestKey,
    pub old_origin: NativeInputOrigin,
    pub owner_cancelled: bool,
}

/// Terminal publication precedes this callback, but reservations remain held
/// until it returns. Recheck the complete consumer owner ticket/current native
/// client/readiness here before acknowledging a discard revision.
#[derive(Clone, Debug)]
pub struct NativeRetirementOutcome {
    pub key: RequestKey,
    pub status: NativeRetirementRequestStatus,
}

pub enum PrepareDecision {
    /// No successor. The old token stays fenced, even on a later explicit retry.
    Decline,
    /// Authorize replacement, not consumer acknowledgement. The completion runs
    /// once even if close/invalidation prevents installation after this decision.
    Proceed(Box<dyn FnOnce(NativeRetirementOutcome)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NativeRetirementRequestError {
    #[error("native retirement requires the main thread")]
    NotMainThread,
    #[error("the window is not live")]
    WindowUnavailable,
    #[error("the captured window/client origin is stale")]
    StaleOrigin,
    #[error("the window already has a retirement reservation")]
    InProgress,
    #[error("one new retirement already occupies global capacity")]
    NewCapacityBusy,
    #[error("an identity or scope counter is exhausted")]
    IdentityExhausted,
    #[error("this native entry or query phase is unsupported")]
    UnsupportedNativeEntry,
    #[error("physical, repeat or keyboard-menu input was observed")]
    UnsupportedHardwareContext,
    #[error("nested native/task entry is unsupported")]
    UnsupportedReentrancy,
    #[error("legacy retirement is pending or draining")]
    LegacyDispatchActive,
    #[error("there is no protected native Edit or dispatcher task")]
    UnprotectedEntry,
}

/// Sample immutable origin without a handler loan, focus request or readiness claim.
pub fn native_input_origin(window: AnyWindowHandle) -> Option<NativeInputOrigin> {
    crate::window::native_input_origin(window, MainThreadMarker::new()?)
}

/// Reserve one request from a supported native Edit continuation or an explicitly
/// tagged dispatcher task. Refusals have no retirement side effects. Queued keeps
/// the prior fence state; prepare runs only after the entire protected tail and
/// all native/GPUI/handler/callback borrows have ended. Prepare must revalidate the
/// consumer owner and return Decline for cancellation, weak failure or mismatch.
/// Neither Queued nor Proceed alone authorizes clearing a consumer discard fence.
pub fn request_native_edit_retirement(
    window: AnyWindowHandle,
    expected: NativeInputOrigin,
    prepare: impl FnOnce(PreparedNativeRetirement) -> PrepareDecision + 'static,
) -> Result<NativeRetirementRequest, NativeRetirementRequestError> {
    let mtm = MainThreadMarker::new().ok_or(NativeRetirementRequestError::NotMainThread)?;
    crate::window::request_native_edit_retirement(window, mtm, expected, Box::new(prepare))
}
