//! Explicit native text-client retirement. See `docs/composition-retirement.md`.
//!
//! This experimental boundary fences callbacks addressed to the old native
//! object. UIKit's handling of undelivered input across a responder transfer
//! is a separate, unverified OS property; a receipt does not assert it.

use std::{cell::Cell, rc::Rc};

use gpui::AnyWindowHandle;
use objc2::MainThreadMarker;

/// A pair of window-local, monotonically increasing native client identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetiredTextInputSession {
    /// The client permanently fenced before the consumer callback runs.
    pub retired: u64,
    /// The distinct native client installed after the consumer callback.
    pub replacement: u64,
}

/// Observable local progress, not an OS cancellation acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputRetirementStatus {
    /// Old callbacks are fenced; GPUI must release its current update first.
    Pending,
    /// Replacement completed. `first_responder` is sampled after native calls.
    Replaced {
        /// Whether the replacement actually acquired the native responder.
        first_responder: bool,
        /// Whether the old client had or was requesting the text responder.
        responder_requested: bool,
    },
    /// The window closed before replacement; its callback may already have run.
    WindowClosed,
}

/// A receipt for one accepted retirement; dropping it does not cancel the work.
#[derive(Clone, Debug)]
pub struct TextInputRetirement {
    pub(crate) sessions: RetiredTextInputSession,
    pub(crate) status: Rc<Cell<TextInputRetirementStatus>>,
}

impl TextInputRetirement {
    /// Identities assigned before any cancellation or responder operation.
    pub fn sessions(&self) -> RetiredTextInputSession {
        self.sessions
    }
    /// Latest local result. `Pending` and failed acquisition are not readiness.
    pub fn status(&self) -> TextInputRetirementStatus {
        self.status.get()
    }
}

/// Refusals leave the current native client unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextInputRetirementError {
    #[error("text-input retirement requires the main thread")]
    NotMainThread,
    #[error("the window is not in a live UIKit scene")]
    WindowUnavailable,
    #[error("a retirement for this window is already in progress")]
    InProgress,
    #[error("retirement inside a native text/key callback would lose its continuation")]
    NativeCallbackInFlight,
    #[error("the native client identity counter is exhausted")]
    IdentityExhausted,
}

/// Permanently fence the current native client and replace it after the
/// current GPUI update unwinds, before returning to the native event loop.
///
/// `retired` runs exactly once for a live window, before the replacement can
/// acquire the responder. Clear the consumer's marked text and reopen its new
/// session there, using a weak GPUI entity/window handle. No text is committed
/// by this API. A closing window cancels the callback and records WindowClosed.
///
/// Retirement requested *from* a native text/key callback is refused: its
/// remaining UIKit continuation cannot safely be moved to another client.
/// Callers must handle this error; a timer/replayed key is not a substitute.
///
/// No run-loop timer or input queue is introduced. This opt-in prototype does
/// not establish UIKit's treatment of input already queued inside the OS.
/// See the contract and native validation matrix before product adoption.
pub fn retire_text_input_session(
    window: AnyWindowHandle,
    retired: impl FnOnce(RetiredTextInputSession) + 'static,
) -> Result<TextInputRetirement, TextInputRetirementError> {
    let mtm = MainThreadMarker::new().ok_or(TextInputRetirementError::NotMainThread)?;
    crate::window::retire_text_input_session(window, mtm, Box::new(retired))
}

/// Current native state, sampled without querying the GPUI input handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextInputSessionState {
    /// Identity of the native object currently owned by the window.
    pub client: u64,
    /// The object is permanently fenced (for example, while replacement waits).
    pub retired: bool,
    /// An accepted retirement has not finished its native replacement yet.
    pub replacement_pending: bool,
    /// UIKit currently identifies this object as first responder.
    pub first_responder: bool,
    /// The native object is attached to a UIWindow.
    pub attached: bool,
}

/// Inspect the receiving native object's identity and actual responder state.
pub fn text_input_session_state(window: AnyWindowHandle) -> Option<TextInputSessionState> {
    crate::window::text_input_session_state(window, MainThreadMarker::new()?)
}
