//! One commit transaction. Existing legacy AFTER is held only synchronously.

use super::{
    Window,
    coordinator::{self, Reservation},
};
use crate::{
    NativeRetirementInterruption as Reason, NativeRetirementOutcome,
    NativeRetirementRequestStatus as Status, PrepareDecision, PreparedNativeRetirement,
};

fn valid(window: &Window, record: &Reservation) -> bool {
    let state = window.state.borrow();
    !state.closed
        && state.native_request_pending == Some(record.receipt.key.serial)
        && window.text_input.is_captured_current(&record.old)
        && record.old.native_target().origin() == record.receipt.key.origin
}

pub(super) fn complete_one() {
    let Some((mut record, completing)) = coordinator::take() else {
        return;
    };
    let hold = crate::gpui_call::LegacyDrainHold::enter();
    let (status, completed) = transact(&mut record);
    record.publish(status.clone());
    if let Some(completed) = completed {
        let _in_gpui = crate::gpui_call::InGpui::enter();
        completed(NativeRetirementOutcome {
            key: record.receipt.key.clone(),
            status,
        });
    }
    drop(record); // Publish and completion precede all slot/reference release.
    drop(completing);
    drop(hold); // Only now resume the unchanged, possibly refilling legacy loop.
}

type Completed = Box<dyn FnOnce(NativeRetirementOutcome)>;

fn transact(record: &mut Reservation) -> (Status, Option<Completed>) {
    let Some(window) = record.window.upgrade().filter(|w| !w.state.borrow().closed) else {
        return (
            Status::WindowClosed {
                old_fenced: record.old_fenced_at_submission,
            },
            None,
        );
    };
    if let Some(reason) = record.interrupted {
        return (record.interrupted_status(reason), None);
    }
    if !valid(&window, record) || record.old.native_target().client.busy() {
        return (record.interrupted_status(Reason::NativeEntry), None);
    }
    if record
        .old
        .native_target()
        .client
        .identity
        .checked_add(1)
        .is_none()
    {
        return (record.interrupted_status(Reason::NativeEntry), None);
    }
    // Linearization: only after the entire accepted native/task tail. Idempotent
    // retire preserves an existing fence on a tagged-task retry after Decline.
    record.old.fence_reserved();
    record.publish(Status::Preparing);
    record.old.cancel_reserved_work();
    if !valid(&window, record) {
        return (
            if window.state.borrow().closed {
                Status::WindowClosed { old_fenced: true }
            } else {
                Status::InvalidatedAfterFence
            },
            None,
        );
    }
    let cancelled = record.receipt.state.borrow().owner_cancelled;
    let Some(prepare) = record.prepare.take() else {
        return (Status::InvalidatedAfterFence, None);
    };
    let decision = {
        let _in_gpui = crate::gpui_call::InGpui::enter();
        prepare(PreparedNativeRetirement {
            key: record.receipt.key.clone(),
            old_origin: record.receipt.key.origin.clone(),
            owner_cancelled: cancelled,
        })
    };
    let completed = match decision {
        PrepareDecision::Decline => {
            return (
                if window.state.borrow().closed {
                    Status::WindowClosed { old_fenced: true }
                } else if !valid(&window, record) {
                    Status::InvalidatedAfterFence
                } else {
                    Status::Declined { old_fenced: true }
                },
                None,
            );
        }
        PrepareDecision::Proceed(_) if cancelled => {
            return (
                if window.state.borrow().closed {
                    Status::WindowClosed { old_fenced: true }
                } else if !valid(&window, record) {
                    Status::InvalidatedAfterFence
                } else {
                    Status::Declined { old_fenced: true }
                },
                None,
            );
        }
        PrepareDecision::Proceed(completed) => completed,
    };
    if !valid(&window, record) {
        return (
            if window.state.borrow().closed {
                Status::WindowClosed { old_fenced: true }
            } else {
                Status::InvalidatedAfterFence
            },
            Some(completed),
        );
    }
    // Construct a successor only after THIS request's Proceed and revalidation.
    let status = window
        .text_input
        .replace_reserved_native(record.old.clone());
    let status = if window.state.borrow().closed {
        Status::WindowClosed { old_fenced: true }
    } else {
        status
    };
    (status, Some(completed))
}
