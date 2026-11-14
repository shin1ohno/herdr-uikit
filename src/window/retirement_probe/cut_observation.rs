//! Test-support only: the passive observer behind `cut_and_observe_selection`.
//! Hooks record into one armed run; they never return early or suppress work.

use std::cell::{Cell, RefCell};

use gpui::{FocusId, TextInputStateChange};
use objc2::{runtime::Sel, sel};

use crate::window::state::Window;

use super::{NativeCutCounter, NativeCutObservationTrace, NativeCutPhase, NativeCutRefusal};
use super::{NativeCutProbeError, NativeCutTraceEvent};

#[derive(Default)]
pub(crate) struct Run {
    id: u64,
    receiver: usize,
    window: usize,
    focus: Option<FocusId>,
    phase: NativeCutPhase,
    permit: bool,
    depth: u8,
    taken: u8,
    seen: u32,
    exhausted: bool,
    trace: NativeCutObservationTrace,
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
    static CONTENDED: Cell<bool> = const { Cell::new(false) };
    static NEXT_ID: Cell<Option<u64>> = const { Cell::new(Some(1)) };
}

fn with_run(f: impl FnOnce(&mut Run)) {
    RUN.with(|run| match run.try_borrow_mut() {
        Ok(mut run) => run.as_mut().map_or((), f),
        Err(_) => CONTENDED.set(true),
    });
}

impl Run {
    fn push(&mut self, refusal: Option<NativeCutRefusal>, range: Option<(usize, usize)>) {
        let (phase, sequence) = (self.phase, self.trace.len);
        let Some(slot) = self.trace.events.get_mut(usize::from(sequence)) else {
            self.exhausted = true;
            return;
        };
        *slot = Some(NativeCutTraceEvent {
            sequence,
            phase,
            refusal,
            range,
        });
        self.trace.len += 1;
    }
    // A refusal is a sticky bit; only its first occurrence takes a trace slot.
    fn refuse(&mut self, reason: NativeCutRefusal) {
        let bit = 1 << reason as u32;
        self.trace.refused |= bit;
        if self.seen & bit == 0 {
            self.seen |= bit;
            self.push(Some(reason), None);
        }
    }
    fn installed(&mut self, same: bool) {
        self.taken = self.taken.saturating_sub(1);
        self.bump(NativeCutCounter::HandlerRestores);
        match same {
            true => self.bump(NativeCutCounter::SameFocusInstalls),
            false => self.refuse(NativeCutRefusal::HandlerReplaced),
        }
    }
    fn bump(&mut self, counter: NativeCutCounter) {
        let value = &mut self.trace.counters[counter as usize];
        match value.checked_add(1) {
            Some(next) => *value = next,
            None => self.refuse(NativeCutRefusal::CounterOverflow),
        }
    }
    fn milestone(&mut self, next: NativeCutPhase, range: Option<(usize, usize)>) {
        let balanced = self.depth == 0 && self.taken == 0;
        if next == NativeCutPhase::FirstGetterEntered || next == NativeCutPhase::FirstResultStored {
            if self.taken != 0 {
                self.refuse(NativeCutRefusal::HandlerUnrestored);
            }
        } else if next == NativeCutPhase::SelectorReturned && !balanced {
            return self.refuse(NativeCutRefusal::PhaseOrder);
        }
        if next as u8 != self.phase as u8 + 1 {
            return self.refuse(NativeCutRefusal::PhaseOrder);
        }
        self.phase = next;
        self.push(None, range);
    }
}

impl NativeCutPhase {
    pub(crate) fn record(self) {
        with_run(|run| run.milestone(self, None));
    }
    /// W3: the accepted edit. Anything but one Cut from Armed is nested.
    pub(crate) fn record_edit(self, selector: Sel) {
        let cut = selector == sel!(cut:);
        with_run(|run| match cut && run.phase == NativeCutPhase::Armed {
            true => run.milestone(self, None),
            false => run.refuse(NativeCutRefusal::NestedEditAction),
        });
    }
    /// Records this milestone now and its successor when dropped.
    pub(crate) fn scope(self) -> PhaseScope {
        self.record();
        PhaseScope(self)
    }
}

pub(crate) struct PhaseScope(NativeCutPhase);
impl Drop for PhaseScope {
    fn drop(&mut self) {
        if self.0 == NativeCutPhase::SnapshotEntered {
            NativeCutPhase::SnapshotReturned.record();
        }
    }
}

impl NativeCutRefusal {
    pub(crate) fn record(self) {
        with_run(|run| {
            run.refuse(self);
            if self == NativeCutRefusal::DeferredExecuted {
                run.bump(NativeCutCounter::TurnCrossings);
            }
        });
    }
    /// Only actual GPUI focus transitions interrupt the interval.
    pub(crate) fn on_state(self, change: &TextInputStateChange) {
        if matches!(
            change,
            TextInputStateChange::FocusGained | TextInputStateChange::FocusLost
        ) {
            self.record();
        }
    }
}

/// The reinstalled handler must belong to the armed window and focus.
pub(crate) fn same_install<K: PartialEq>(
    armed: (usize, &Option<K>),
    set: (usize, &Option<K>),
) -> bool {
    armed.0 == set.0 && armed.1.is_some() && armed.1 == set.1
}

impl NativeCutCounter {
    pub(crate) fn scope(self) -> DepthScope {
        with_run(|run| {
            run.bump(self);
            run.depth = run.depth.saturating_add(1);
        });
        DepthScope
    }
    pub(in crate::window) fn on_take(self, window: &Window) {
        self.took(window.state.borrow().input_handler.is_some());
    }
    fn took(self, present: bool) {
        with_run(|run| run.taken = run.taken.saturating_add(u8::from(present)));
    }
    /// W8: after GPUI stored a handler; reads the installed handler's focus.
    pub(in crate::window) fn on_set(self, window: &Window) {
        let state = window.state.borrow();
        let focus = state
            .input_handler
            .as_ref()
            .and_then(|handler| handler.focus_id());
        let window = std::ptr::from_ref(window).addr();
        with_run(|run| run.installed(same_install((run.window, &run.focus), (window, &focus))));
    }
    /// W9: a lend of the stored handler is restored when the scope ends.
    pub(crate) fn restore_scope(self) -> RestoreScope {
        self.took(true);
        RestoreScope
    }
    pub(crate) fn presented(self) {
        with_run(|run| {
            run.bump(self);
            if run.phase >= NativeCutPhase::SelectorReturned {
                run.refuse(NativeCutRefusal::LatePresentation);
            }
        });
    }
    /// W5: the first getter needs the one permit the core sets after return.
    pub(crate) fn query(self) -> QueryScope {
        with_run(|run| {
            run.bump(self);
            match std::mem::take(&mut run.permit) {
                true => run.milestone(NativeCutPhase::FirstGetterEntered, None),
                false => run.refuse(NativeCutRefusal::EarlyQuery),
            }
        });
        QueryScope
    }
}

pub(crate) struct DepthScope;
impl Drop for DepthScope {
    fn drop(&mut self) {
        with_run(|run| run.depth = run.depth.saturating_sub(1));
    }
}
pub(crate) struct RestoreScope;
impl Drop for RestoreScope {
    fn drop(&mut self) {
        with_run(|run| {
            run.taken = run.taken.saturating_sub(1);
            run.bump(NativeCutCounter::HandlerRestores);
        });
    }
}
pub(crate) struct QueryScope;
impl Drop for QueryScope {
    fn drop(&mut self) {
        with_run(|run| run.bump(NativeCutCounter::QueryExits));
    }
}

/// Owner of the armed run; dropping it disarms only its own run.
pub(crate) struct Armed(u64);

pub(crate) fn arm(
    receiver: usize,
    window: usize,
    focus: FocusId,
) -> Result<Armed, NativeCutProbeError> {
    RUN.with(|slot| {
        let mut slot = slot.try_borrow_mut();
        let Some(slot) = slot.as_mut().ok().filter(|slot| slot.is_none()) else {
            return Err(NativeCutProbeError::CallbackLoanActive);
        };
        let id = NEXT_ID
            .get()
            .ok_or(NativeCutProbeError::IdentityExhausted)?;
        NEXT_ID.set(id.checked_add(1));
        CONTENDED.set(false);
        let mut run = Run {
            id,
            receiver,
            window,
            focus: Some(focus),
            ..Run::default()
        };
        run.push(None, None);
        **slot = Some(run);
        Ok(Armed(id))
    })
}

pub(crate) fn permit() {
    with_run(|run| run.permit = true);
}

pub(crate) fn phase(receiver: usize) -> Option<NativeCutPhase> {
    RUN.with(|run| {
        let run = run.try_borrow().ok()?;
        run.as_ref()
            .filter(|run| run.receiver == receiver)
            .map(|run| run.phase)
    })
}

impl Armed {
    /// Stores the first Result's range, verifies, and freezes a copy.
    pub(crate) fn finish(
        self,
        range: Option<(usize, usize)>,
    ) -> (u64, NativeCutObservationTrace, bool) {
        with_run(|run| run.milestone(NativeCutPhase::FirstResultStored, range));
        if CONTENDED.replace(false) {
            with_run(|run| run.refuse(NativeCutRefusal::Contention));
        }
        let own = |run: &mut Option<Run>| run.take_if(|run| run.id == self.0);
        let run = RUN.with(|slot| {
            slot.try_borrow_mut()
                .ok()
                .and_then(|mut slot| own(&mut slot))
        });
        let run = run.unwrap_or_default();
        (self.0, run.trace, run.exhausted || run.id != self.0)
    }
}

impl Drop for Armed {
    fn drop(&mut self) {
        RUN.with(|slot| {
            let Ok(mut slot) = slot.try_borrow_mut() else {
                return;
            };
            if slot.as_ref().is_some_and(|run| run.id == self.0) {
                *slot = None;
            }
        });
    }
}

#[cfg(test)]
mod tests;
