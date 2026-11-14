//! Test-support only: one actual `cut:` on this retained receiver, joined to
//! the same receiver's first selection query. The production tail runs
//! unchanged; passive hooks only record, and nothing repairs or recaptures.
#![allow(
    clippy::result_large_err,
    reason = "the admitted test-support API returns the frozen observation by value"
)]

use std::{ops::Range, rc::Rc};

use objc2::{DefinedClass, MainThreadMarker, msg_send, rc::Retained, runtime::AnyObject, sel};

use super::{NativeClient, NativeQuery, NativeQueryError};
use crate::window::retirement_probe::{cut_observation as obs, next_native_turn};

pub type NativeCutProbeResult = Result<NativeCutSelectionObservation, NativeCutProbeFailure>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCutSelectionObservation {
    pub run: u64,
    pub client: u64,
    pub selector_returned: bool,
    pub first_selection: Result<NativeQuery<Option<Range<usize>>>, NativeQueryError>,
    pub trace: NativeCutObservationTrace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCutProbeFailure {
    pub reason: NativeCutProbeError,
    pub selector_sent: bool,
    pub observation: Option<NativeCutSelectionObservation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCutProbeError {
    CallbackLoanActive,
    NotEligible,
    ClientRetired,
    IdentityExhausted,
    IntervalInterrupted,
    TraceExhausted,
}

/// Observation milestones, in their only legal order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeCutPhase {
    #[default]
    Armed,
    AcceptedCut,
    DownReturned,
    UpReturned,
    OriginRestored,
    ModifiersRestored,
    SnapshotEntered,
    SnapshotReturned,
    SelectorReturned,
    FirstGetterEntered,
    FirstResultStored,
}

/// Sticky reasons; a later correct value never clears one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCutRefusal {
    EarlyQuery,
    Setter,
    DeferredScheduled,
    DeferredExecuted,
    FrameTick,
    InputHandlerFlush,
    TextInputFlush,
    DelegateNotify,
    AfterAction,
    Resign,
    LeavingWindow,
    FocusChange,
    Configuration,
    KeyboardShown,
    KeyboardHidden,
    HandlerReplaced,
    HandlerUnrestored,
    LatePresentation,
    NestedEditAction,
    PhaseOrder,
    Contention,
    CounterOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeCutCounter {
    DispatchDepth,
    HandlerDepth,
    HandlerRestores,
    SameFocusInstalls,
    QueryEntries,
    QueryExits,
    Presentations,
    TurnCrossings,
}

/// Milestones and the first event of each refusal kind; no text or keys.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeCutObservationTrace {
    pub events: [Option<NativeCutTraceEvent>; 16],
    pub len: u8,
    pub refused: u32,
    pub counters: [u16; 8], // indexed by NativeCutCounter
}

impl NativeCutObservationTrace {
    pub fn has_refusal(&self, reason: NativeCutRefusal) -> bool {
        self.refused & (1 << reason as u32) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCutTraceEvent {
    pub sequence: u8,
    pub phase: NativeCutPhase,
    pub refusal: Option<NativeCutRefusal>,
    pub range: Option<(usize, usize)>,
}

fn failure(reason: NativeCutProbeError, selector_sent: bool) -> NativeCutProbeFailure {
    let observation = None;
    NativeCutProbeFailure {
        reason,
        selector_sent,
        observation,
    }
}

impl NativeClient {
    /// The real production `cut:` once, then the same receiver's unchanged
    /// `selected_range_utf16` once. A valid trace is not semantic success.
    pub fn cut_and_observe_selection(&self) -> NativeCutProbeResult {
        let run = self.begin_cut()?;
        self.finish_cut(run)
    }

    // Ordering negative only: SDK N1 fixture mode, and P13's named QA mutant
    // `p13-cut-first-query-order`. Never a baseline/positive entry point.
    pub fn cut_and_observe_selection_next_turn(
        &self,
        done: impl FnOnce(NativeCutProbeResult) + 'static,
    ) -> Result<(), NativeCutProbeFailure> {
        let run = self.begin_cut()?;
        let client = self.clone();
        next_native_turn(move || done(client.finish_cut(run)));
        Ok(())
    }

    // Passive read of the active run's observation milestone for THIS
    // retained receiver; None when unarmed or another receiver owns the run.
    pub fn cut_phase(&self) -> Option<NativeCutPhase> {
        obs::phase(Retained::as_ptr(&self.0).addr())
    }

    fn begin_cut(&self) -> Result<obs::Armed, NativeCutProbeFailure> {
        use NativeCutProbeError::*;
        let target = &self.0.ivars().target;
        if MainThreadMarker::new().is_none() || crate::gpui_call::is_running() {
            return Err(failure(CallbackLoanActive, false));
        } else if !target.client.live() {
            return Err(failure(ClientRetired, false));
        } else if target.client.busy() {
            return Err(failure(CallbackLoanActive, false));
        }
        let window = target.window_handle().filter(|_| self.attached());
        let focus = window.as_ref().and_then(|window| {
            let state = window.state.borrow();
            state
                .input_handler
                .as_ref()
                .and_then(|handler| handler.focus_id())
        });
        let (Some(window), Some(focus)) = (window, focus) else {
            return Err(failure(NotEligible, false));
        };
        if !self.first_responder() || !self.0.editing_enabled(sel!(cut:)) {
            return Err(failure(NotEligible, false));
        }
        let receiver = Retained::as_ptr(&self.0).addr();
        let armed = obs::arm(receiver, Rc::as_ptr(&window).addr(), focus);
        let armed = armed.map_err(|reason| failure(reason, false))?;
        // SAFETY: the view implements the standard sender-only cut: selector.
        unsafe {
            let () = msg_send![&*self.0, cut: None::<&AnyObject>];
        }
        NativeCutPhase::SelectorReturned.record();
        Ok(armed)
    }

    fn finish_cut(&self, run: obs::Armed) -> NativeCutProbeResult {
        use NativeCutProbeError::*;
        let client = &self.0.ivars().target.client;
        let loaned = client.busy() || crate::gpui_call::is_running();
        let retired = (!client.live()).then_some(ClientRetired);
        let skip = retired.or(loaned.then_some(CallbackLoanActive));
        // The query is skipped, never invented, when it would be unsafe.
        let first = skip.is_none().then(|| {
            obs::permit();
            self.selected_range_utf16()
        });
        let stored = first.as_ref().and_then(|first| first.as_ref().ok());
        let range = stored.and_then(|query| query.value.as_ref());
        let (run, trace, exhausted) = run.finish(range.map(|range| (range.start, range.end)));
        let accepted = trace
            .events
            .iter()
            .flatten()
            .any(|event| event.phase == NativeCutPhase::AcceptedCut && event.refusal.is_none());
        let reason = skip
            .or((!accepted).then_some(NotEligible))
            .or((trace.refused != 0).then_some(IntervalInterrupted))
            .or(exhausted.then_some(TraceExhausted));
        let observation = first.map(|first_selection| NativeCutSelectionObservation {
            run,
            client: self.identity(),
            selector_returned: true,
            first_selection,
            trace,
        });
        match reason {
            None => observation.ok_or(failure(NotEligible, true)),
            Some(reason) => Err(NativeCutProbeFailure {
                reason,
                selector_sent: true,
                observation,
            }),
        }
    }
}
