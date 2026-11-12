//! Simulator-only native-entry evidence. Not a transport or clipboard oracle.
//! No observer borrow crosses any callback or UIKit operation.
mod queue;
mod types;
pub use types::*;

use objc2::MainThreadMarker;
use queue::Queue;
use std::{
    cell::{Cell, RefCell},
    sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static INVALID: AtomicU64 = AtomicU64::new(0);
static VERSION: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static STATE: RefCell<Option<Queue>> = const { RefCell::new(None) };
    static KEY: Cell<Option<ObservationKey>> = const { Cell::new(None) };
    static RESPONDER: Cell<ResponderOrigin> = const { Cell::new(ResponderOrigin::Text) };
}

fn main_thread() -> Result<(), ObserverError> {
    MainThreadMarker::new()
        .map(|_| ())
        .ok_or(ObserverError::NotMainThread)
}
fn advance_version() {
    if VERSION
        .try_update(SeqCst, SeqCst, |n| n.checked_add(1))
        .is_err()
    {
        INVALID.fetch_or(InvalidReason::CounterOverflow.bit(), SeqCst);
    }
}
/// Independent of STATE, including on try_borrow failure. Main-thread only.
fn reject(reason: InvalidReason) {
    advance_version();
    INVALID.fetch_or(reason.bit(), SeqCst);
    if DROPPED
        .try_update(SeqCst, SeqCst, |n| n.checked_add(1))
        .is_err()
    {
        INVALID.fetch_or(InvalidReason::CounterOverflow.bit(), SeqCst);
    }
    advance_version();
}
fn error(reason: ObserverError) -> ObserverError {
    reject(match reason {
        ObserverError::CounterOverflow => InvalidReason::CounterOverflow,
        ObserverError::Capacity => InvalidReason::Capacity,
        ObserverError::Outstanding => InvalidReason::Outstanding,
        ObserverError::Contention => InvalidReason::Contention,
        ObserverError::InvalidSnapshot => InvalidReason::InvalidSnapshot,
        _ => InvalidReason::StaleIdentity,
    });
    reason
}
fn with_state<T>(
    f: impl FnOnce(&mut Queue) -> Result<T, ObserverError>,
) -> Result<T, ObserverError> {
    STATE
        .try_with(|cell| {
            let mut state = cell
                .try_borrow_mut()
                .map_err(|_| error(ObserverError::Contention))?;
            let queue = state.as_mut().ok_or(ObserverError::NotStarted)?;
            f(queue)
        })
        .map_err(|_| error(ObserverError::Contention))?
}

/// Start exactly once per process, explicitly. OS entropy is never read by an
/// unarmed normal app. A runner must independently reject reuse across launches.
pub fn begin_run() -> Result<ProcessRun, ObserverError> {
    main_thread()?;
    STATE
        .try_with(|cell| {
            let mut state = cell
                .try_borrow_mut()
                .map_err(|_| error(ObserverError::Contention))?;
            if state.is_some() {
                return Err(ObserverError::AlreadyStarted);
            }
            // Generate entropy outside the observer borrow (Uuid uses OS entropy).
            drop(state);
            let run = ProcessRun(uuid::Uuid::new_v4().as_u128());
            state = cell
                .try_borrow_mut()
                .map_err(|_| error(ObserverError::Contention))?;
            if state.is_some() {
                return Err(ObserverError::AlreadyStarted);
            }
            *state = Some(Queue::new(run));
            Ok(run)
        })
        .map_err(|_| error(ObserverError::Contention))?
}

/// Arm a resolved live canonical view. All scheduled selector work in the old
/// phase must already have actual completion receipts; SDK cannot join UIKit's
/// external scheduling. Timeout/cancellation requires invalidate_run().
pub fn arm(
    window: gpui::AnyWindowHandle,
    run: ProcessRun,
    phase: Phase,
) -> Result<Arm, ObserverError> {
    main_thread()?;
    crate::window::with_observation_registration(window, |registration| {
        if INVALID.load(SeqCst) != 0 {
            return Err(ObserverError::InvalidRun);
        }
        let result = with_state(|queue| queue.arm(run, phase, registration.0.get()));
        match result {
            Ok(arm) => {
                registration.0.set(arm.view_generation);
                ENABLED.store(true, SeqCst);
                Ok(arm)
            }
            Err(ObserverError::Contention) => Err(ObserverError::Contention),
            Err(e) => Err(error(e)),
        }
    })
    .map_err(error)?
}

/// Returns process status even after teardown; never clears invalidity.
pub fn run_status(run: ProcessRun) -> Result<Status, ObserverError> {
    main_thread()?;
    with_state(|queue| snapshot(queue, run))
}
fn snapshot(queue: &mut Queue, run: ProcessRun) -> Result<Status, ObserverError> {
    if queue.run != run {
        return Err(error(ObserverError::StaleIdentity));
    }
    queue.sequence = queue
        .sequence
        .checked_add(1)
        .ok_or_else(|| error(ObserverError::CounterOverflow))?;
    for _ in 0..4 {
        let before = VERSION.load(SeqCst);
        if before % 2 != 0 || before == u64::MAX {
            continue;
        }
        let dropped_total = DROPPED.load(SeqCst);
        let invalid_reasons = INVALID.load(SeqCst);
        let after = VERSION.load(SeqCst);
        if before == after {
            return Ok(Status {
                schema_version: SCHEMA_VERSION,
                run,
                latest_epoch: queue.epoch,
                enabled: queue.enabled,
                dropped_total,
                invalid_reasons,
                sticky_invalid: invalid_reasons != 0,
                outstanding: queue.depth as u16,
                reserved: queue.depth as u16,
                queued: queue.len as u16,
                snapshot_sequence: queue.sequence,
            });
        }
    }
    Err(error(ObserverError::InvalidSnapshot))
}
/// Copy ring and independent status as one snapshot, then consume that ring.
/// Status queued count describes the returned snapshot before consumption.
pub fn drain(window: gpui::AnyWindowHandle, expected: Arm) -> Result<Drain, ObserverError> {
    main_thread()?;
    crate::window::with_observation_registration(window, |registration| {
        with_state(|queue| {
            if registration.0.get() != expected.view_generation || !queue.contains(expected) {
                return Err(error(ObserverError::StaleIdentity));
            }
            let status = snapshot(queue, expected.run)?;
            let result = Drain {
                records: queue.copy_records(),
                len: queue.len,
                status,
            };
            queue.clear_records();
            Ok(result)
        })
    })
    .map_err(error)?
}
/// Disable never resets counters or invalidity. Returns original run status.
pub fn disable(run: ProcessRun) -> Result<Status, ObserverError> {
    main_thread()?;
    with_state(|queue| {
        if queue.run != run {
            return Err(error(ObserverError::StaleIdentity));
        }
        if queue.depth != 0 {
            reject(InvalidReason::DisabledOutstanding);
        }
        queue.enabled = false;
        ENABLED.store(false, SeqCst);
        snapshot(queue, run)
    })
}
/// Required when the runner cancels/times out scheduled work. No phase rescue.
pub fn invalidate_run(run: ProcessRun) -> Result<Status, ObserverError> {
    main_thread()?;
    with_state(|queue| {
        if queue.run != run {
            return Err(error(ObserverError::StaleIdentity));
        }
        reject(InvalidReason::Cancelled);
        queue.enabled = false;
        ENABLED.store(false, SeqCst);
        snapshot(queue, run)
    })
}

/// Only an actual observed KeyDown/KeyUp scope exposes this key. It is not
/// inherited by physical input, modifiers, fallback or deferred tasks.
pub fn current_native_observation_key() -> Option<ObservationKey> {
    if ENABLED.load(SeqCst) {
        KEY.try_with(Cell::get).unwrap_or_else(|_| {
            reject(InvalidReason::Contention);
            None
        })
    } else {
        None
    }
}
pub(crate) struct KeyScope(Option<Option<ObservationKey>>);
impl KeyScope {
    pub(crate) fn enter(key: Option<ObservationKey>) -> Self {
        Self(if ENABLED.load(SeqCst) {
            KEY.try_with(|cell| cell.replace(key)).ok().or_else(|| {
                reject(InvalidReason::Contention);
                None
            })
        } else {
            None
        })
    }
}
impl Drop for KeyScope {
    fn drop(&mut self) {
        if let Some(previous) = self.0 {
            if KEY.try_with(|cell| cell.set(previous)).is_err() {
                reject(InvalidReason::Contention);
            }
        }
    }
}

/// Fixed data only; no ownership of a view, handler, sender or window.
#[derive(Default)]
pub(crate) struct Registration(Cell<u64>);
impl Registration {
    pub(crate) fn remove(&self) {
        let generation = self.0.get();
        if generation == 0 {
            return;
        }
        let _ = with_state(|q| {
            if q.remove(generation) {
                reject(InvalidReason::ViewLost);
            }
            Ok(())
        });
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.remove();
    }
}

pub(crate) struct DrawingScope(Option<ResponderOrigin>);
impl DrawingScope {
    pub(crate) fn enter() -> Self {
        Self(if ENABLED.load(SeqCst) {
            RESPONDER
                .try_with(|cell| cell.replace(ResponderOrigin::Drawing))
                .ok()
                .or_else(|| {
                    reject(InvalidReason::Contention);
                    None
                })
        } else {
            None
        })
    }
}
impl Drop for DrawingScope {
    fn drop(&mut self) {
        if let Some(previous) = self.0 {
            if RESPONDER.try_with(|cell| cell.set(previous)).is_err() {
                reject(InvalidReason::Contention);
            }
        }
    }
}

pub(crate) struct InvocationGuard(Option<Record>);
impl InvocationGuard {
    pub(crate) fn enter(registration: &Registration, action: Action) -> Self {
        if !ENABLED.load(SeqCst) {
            return Self(None);
        }
        // Consume the responder scope at canonical entry: nested text selectors
        // must not inherit a drawing origin from an enclosing callback.
        if INVALID.load(SeqCst) & InvalidReason::CounterOverflow.bit() != 0 {
            reject(InvalidReason::CounterOverflow);
            return Self(None);
        }
        let responder = match RESPONDER.try_with(|cell| cell.replace(ResponderOrigin::Text)) {
            Ok(responder) => responder,
            Err(_) => {
                reject(InvalidReason::Contention);
                return Self(None);
            }
        };
        let result = with_state(|q| q.enter(registration.0.get(), action, responder));
        match result {
            Ok(entry) => {
                if action == Action::Unknown {
                    reject(InvalidReason::UnknownSelector);
                }
                Self(Some(entry))
            }
            Err(e) => {
                if e != ObserverError::Contention {
                    error(e);
                }
                Self(None)
            }
        }
    }
    pub(crate) fn key(&self) -> Option<ObservationKey> {
        self.0.map(|r| r.key)
    }
    pub(crate) fn complete(mut self, decision: Decision, outcome: Outcome) {
        if let Some(entry) = self.0.take() {
            finish(entry, Some(decision), outcome);
        }
    }
}
fn finish(entry: Record, decision: Option<Decision>, outcome: Outcome) {
    if outcome == Outcome::Aborted {
        reject(InvalidReason::Aborted);
    }
    let _ = with_state(|q| {
        if let Err(reason) = q.complete(entry, decision, outcome) {
            reject(reason);
        }
        Ok(())
    });
}
impl Drop for InvocationGuard {
    fn drop(&mut self) {
        if let Some(entry) = self.0.take() {
            finish(entry, None, Outcome::Aborted);
        }
    }
}

/// Invoke a fixed standard action through the actual Objective-C entrypoint on
/// a resolved live responder. This is a test stimulus, NOT evidence of callback
/// entry, dispatch, async paste publication or receiving completion. It works
/// with observation disabled, permitting a real off/on negative control.
/// The caller must freeze its manifest/arm before the stimulus, then require
/// matching drained pairs and independent consumer/transport receipts.
pub fn invoke_native_selector(
    window: gpui::AnyWindowHandle,
    action: Action,
    responder: ResponderOrigin,
) -> Result<(), ObserverError> {
    main_thread()?;
    crate::window::invoke_observation_selector(window, action, responder)
}

#[cfg(test)]
mod runtime_tests;

#[cfg(test)]
pub(crate) fn with_source_test_observer(f: impl FnOnce(&Registration, Arm)) {
    let (_lock, registration, arm) = runtime_tests::setup();
    f(&registration, arm);
}
