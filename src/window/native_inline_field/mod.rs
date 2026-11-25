//! The inline field's session: its consumer side beside the window's native
//! modal gate (shared with the search panel, `super::native_modal`), the
//! deferred present (`present`), the UIKit objects (`controller`) and the
//! std-only decisions (`policy`).

mod controller;
mod policy;
mod present;
#[cfg(all(target_abi = "sim", feature = "test-support"))]
mod qa;
#[cfg(feature = "test-support")]
pub(crate) mod tests;

use std::rc::{Rc, Weak};

use gpui::{AnyWindowHandle, Bounds, Pixels};

// `policy` names the gate as `super::gate`, as the host tests include both.
use super::native_modal::{self as modal, Note, OpenError, Session, SessionId, gate};
use super::state::Window;
use crate::native_inline_field::{InlineQueue, NativeInlineFieldError, NativeInlineFieldRequest};
pub(crate) use policy::Bounded;
pub(crate) use policy::Decision;
pub use policy::{NativeInlineDismissReason, NativeInlineFieldEvent};
#[cfg(all(target_abi = "sim", feature = "test-support"))]
pub use qa::{NativeInlineFieldState, native_inline_field_press, native_inline_field_state};

/// The live inline field session's consumer side.
pub(crate) struct Inline {
    pub(super) id: SessionId,
    pub(super) window: Weak<Window>,
    pub(super) queue: InlineQueue,
    pub(super) controller: Option<Rc<controller::Controller>>,
    /// The label's latest rectangle and clip (`set_frame`).
    frame: Bounds<Pixels>,
    clip: Option<Bounds<Pixels>>,
    initial: String,
    allow_empty: bool,
    max_chars: usize,
    /// What the first non-composing Return decided.
    pub(super) decision: Option<Decision>,
}

impl From<OpenError> for NativeInlineFieldError {
    fn from(error: OpenError) -> Self {
        match error {
            OpenError::WindowUnavailable => Self::WindowUnavailable,
            OpenError::AlreadyPresented => Self::AlreadyPresented,
            OpenError::RetirementInProgress => Self::RetirementInProgress,
        }
    }
}

pub(crate) fn present(
    handle: AnyWindowHandle,
    request: NativeInlineFieldRequest,
    queue: InlineQueue,
) -> Result<SessionId, NativeInlineFieldError> {
    let (frame, clip, initial) = (request.frame, request.clip, request.text.clone());
    let (allow_empty, max_chars) = (request.allow_empty, request.max_chars);
    modal::open_session(
        handle,
        move |id, window| {
            Session::Inline(Inline {
                id,
                window,
                queue,
                controller: None,
                frame,
                clip,
                initial,
                allow_empty,
                max_chars,
                decision: None,
            })
        },
        move |w, id, this| w.present_inline(id, request, this),
    )
    .map_err(NativeInlineFieldError::from)
}

pub(crate) fn set_frame(
    handle: AnyWindowHandle,
    id: SessionId,
    frame: Bounds<Pixels>,
    clip: Option<Bounds<Pixels>>,
) -> Result<(), NativeInlineFieldError> {
    let window = modal::window(handle).ok_or(NativeInlineFieldError::Ended)?;
    {
        let mut slot = window.native_search.borrow_mut();
        let session = slot
            .as_mut()
            .and_then(Session::inline_mut)
            .filter(|s| s.id == id)
            .ok_or(NativeInlineFieldError::Ended)?;
        session.frame = frame;
        session.clip = clip;
    }
    window.defer(move |w| w.place_inline(id));
    Ok(())
}

pub(crate) fn dismiss(
    handle: AnyWindowHandle,
    id: SessionId,
) -> Result<(), NativeInlineFieldError> {
    let window = modal::window(handle).ok_or(NativeInlineFieldError::Ended)?;
    let inline = window
        .native_search
        .borrow()
        .as_ref()
        .and_then(Session::inline)
        .is_some_and(|s| s.id == id);
    if !inline {
        return Err(NativeInlineFieldError::Ended);
    }
    modal::dismiss(handle, id, gate::DismissReason::Requested)
        .map_err(|_| NativeInlineFieldError::Ended)
}

/// The consumer's event for the gate's note.
pub(super) fn emit(queue: &InlineQueue, note: Note, decision: Option<&Decision>) {
    let event = match note {
        Note::Opened => NativeInlineFieldEvent::Opened,
        Note::Dismissed(reason) => {
            NativeInlineFieldEvent::Dismissed(policy::dismiss_reason(reason, decision))
        }
        Note::Failed(failure) => NativeInlineFieldEvent::Dismissed(
            NativeInlineDismissReason::PresentationFailed(failure),
        ),
        Note::Restored(responder) => NativeInlineFieldEvent::InputRestored(responder),
    };
    let mut queue = queue.borrow_mut();
    if queue.push(event).is_err() {
        tracing::error!(
            ?note,
            "the inline field's event queue is full; event dropped"
        );
    }
    if matches!(note, Note::Restored(_)) {
        queue.close();
    }
}

impl Window {
    /// Return outside composition: `Committed` goes out first when the name
    /// changed, then the shared dismissal (`Return`). The first Return wins.
    pub(super) fn inline_return(&self, id: SessionId, text: &str) -> Option<Decision> {
        let up = matches!(
            self.state.borrow().native_modal.phase(),
            Some((current, gate::Phase::Presented { focused: true })) if current == id
        );
        if !up {
            return None;
        }
        let decision = {
            let mut slot = self.native_search.borrow_mut();
            let session = slot
                .as_mut()
                .and_then(Session::inline_mut)
                .filter(|s| s.id == id && s.decision.is_none())?;
            let decision = policy::commit_decision(
                &session.initial,
                text,
                session.allow_empty,
                session.max_chars,
            );
            if decision == Decision::Hint {
                return Some(decision);
            }
            if let Decision::Commit(name) = &decision
                && session
                    .queue
                    .borrow_mut()
                    .push(NativeInlineFieldEvent::Committed(name.clone()))
                    .is_err()
            {
                tracing::error!("the inline field's event queue is full; commit dropped");
            }
            session.decision = Some(decision.clone());
            decision
        };
        self.native_modal_dismiss(id, gate::DismissReason::Return);
        Some(decision)
    }
}
