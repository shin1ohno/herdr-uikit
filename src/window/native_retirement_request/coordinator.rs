//! One retained new request globally. No queue, timer, history or drain loop.
use std::{cell::RefCell, rc::Weak};

use objc2::rc::Retained;

use super::super::text_input::NativeEndpoint;
use super::{Prepare, Window};
use crate::{
    NativeRetirementInterruption as Reason, NativeRetirementRequest,
    NativeRetirementRequestStatus as Status,
};

pub(super) struct Reservation {
    pub(super) window: Weak<Window>,
    pub(super) old: Retained<NativeEndpoint>,
    pub(super) receipt: NativeRetirementRequest,
    pub(super) old_fenced_at_submission: bool,
    pub(super) prepare: Option<Prepare>,
    pub(super) interrupted: Option<Reason>,
}

impl Reservation {
    pub(super) fn publish(&self, status: Status) {
        self.receipt.state.borrow_mut().status = status;
    }
    pub(super) fn interrupted_status(&self, reason: Reason) -> Status {
        Status::InterruptedBeforeFence {
            reason,
            old_fenced_at_submission: self.old_fenced_at_submission,
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // Passive unwind cleanup never calls a consumer/UIKit function. Receipt
        // drop has no such behavior; only this coordinator-owned record releases.
        if std::thread::panicking() {
            let status = match self.receipt.status() {
                Status::Preparing => Some(Status::InvalidatedAfterFence),
                Status::Queued { .. } => Some(self.interrupted_status(Reason::NativeEntry)),
                _ => None,
            };
            if let Some(status) = status {
                self.publish(status);
            }
        }
        if let Some(window) = self.window.upgrade() {
            let mut state = window.state.borrow_mut();
            if state.native_request_pending == Some(self.receipt.key.serial) {
                state.native_request_pending = None;
                window.text_input.replacing.set(false);
            }
        }
    }
}

#[derive(Default)]
struct Coordinator {
    pending: Option<Reservation>,
    completing: bool,
}
thread_local! { static STATE: RefCell<Coordinator> = RefCell::new(Coordinator::default()); }

pub(super) fn busy() -> bool {
    STATE.with_borrow(|s| s.completing || s.pending.is_some())
}

pub(super) fn reserve(record: Reservation) {
    STATE.with_borrow_mut(|s| {
        // No callback occurs between validation and reservation.
        debug_assert!(!s.completing && s.pending.is_none());
        s.pending = Some(record);
    });
}

pub(super) fn interrupt(reason: Reason) {
    STATE.with_borrow_mut(|s| {
        if let Some(record) = &mut s.pending {
            record.interrupted.get_or_insert(reason);
        }
    });
}

pub(super) struct Completing;
impl Drop for Completing {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|s| s.completing = false);
    }
}

pub(super) fn take() -> Option<(Reservation, Completing)> {
    STATE.with_borrow_mut(|s| {
        if s.completing {
            return None;
        }
        let record = s.pending.take()?;
        s.completing = true;
        Some((record, Completing))
    })
}

pub(super) fn legacy_interrupt() {
    // During Preparing/terminal callback, take returns None: ordinary legacy
    // acceptance is retained, and LegacyDrainHold delays only its AFTER closure.
    if let Some((record, completing)) = take() {
        record.publish(record.interrupted_status(Reason::LegacyRequest));
        drop(record); // terminal FIRST, Window/old/closure references SECOND
        drop(completing); // global capacity LAST, before caller starts legacy AFTER
    }
}
