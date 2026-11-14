//! Checked scope accounting. Unknown arrivals never become Edit continuations.
use std::cell::RefCell;

use super::Phase;
use crate::{NativeInputOrigin, NativeRetirementRequestError as Error};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Edit,
    Refused,
    Event,
    PressesEvent,
    Task,
}

#[derive(Clone, Copy)]
struct Entry {
    serial: u64,
    kind: Kind,
}

struct ActivePhase {
    serial: Option<u64>,
    origin: NativeInputOrigin,
    phase: Phase,
}

#[derive(Default)]
struct State {
    serial: u64,
    poisoned: bool,
    native: u32,
    refused: u32,
    events: u32,
    presses: u32,
    tasks: u32,
    loans: u32,
    top: Option<Entry>,
    phase: Option<ActivePhase>,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

fn increment(counter: &mut u32) -> bool {
    let Some(next) = counter.checked_add(1) else {
        return false;
    };
    *counter = next;
    true
}

pub(super) struct Frame {
    prior: Option<Entry>,
    kind: Kind,
    serial: Option<u64>,
    entered: bool,
}
impl Frame {
    pub(super) fn enter(kind: Kind) -> Self {
        STATE.with_borrow_mut(|s| {
            let prior = s.top;
            let serial = s.serial.checked_add(1);
            let entered = match kind {
                Kind::Edit => increment(&mut s.native),
                Kind::Refused => {
                    // Check both first: no partial increment on overflow.
                    if s.native == u32::MAX || s.refused == u32::MAX {
                        false
                    } else {
                        s.native += 1;
                        s.refused += 1;
                        true
                    }
                }
                Kind::Event => increment(&mut s.events),
                Kind::PressesEvent => {
                    if s.events == u32::MAX || s.presses == u32::MAX {
                        false
                    } else {
                        s.events += 1;
                        s.presses += 1;
                        true
                    }
                }
                Kind::Task => increment(&mut s.tasks),
            };
            if let Some(serial) = serial {
                s.serial = serial;
                s.top = Some(Entry { serial, kind });
            }
            s.poisoned |= serial.is_none() || !entered;
            Self {
                prior,
                kind,
                serial,
                entered,
            }
        })
    }
    pub(super) fn serial(&self) -> Option<u64> {
        self.serial
    }
}
impl Drop for Frame {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|s| {
            s.top = self.prior;
            if self.entered {
                match self.kind {
                    Kind::Edit => s.native -= 1,
                    Kind::Refused => {
                        s.native -= 1;
                        s.refused -= 1;
                    }
                    Kind::Event => s.events -= 1,
                    Kind::PressesEvent => {
                        s.events -= 1;
                        s.presses -= 1;
                    }
                    Kind::Task => s.tasks -= 1,
                }
            }
        });
    }
}

pub(super) struct Loan(bool);
impl Loan {
    pub(super) fn enter() -> Self {
        STATE.with_borrow_mut(|s| {
            let entered = increment(&mut s.loans);
            s.poisoned |= !entered;
            Self(entered)
        })
    }
}
impl Drop for Loan {
    fn drop(&mut self) {
        if self.0 {
            STATE.with_borrow_mut(|s| s.loans -= 1);
        }
    }
}

pub(super) struct PhaseScope(Option<ActivePhase>);
impl PhaseScope {
    pub(super) fn enter(serial: Option<u64>, origin: NativeInputOrigin, phase: Phase) -> Self {
        Self(STATE.with_borrow_mut(|s| {
            s.phase.replace(ActivePhase {
                serial,
                origin,
                phase,
            })
        }))
    }
}
impl Drop for PhaseScope {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|s| s.phase = self.0.take());
    }
}

pub(super) fn hardware_event() -> bool {
    STATE.with_borrow(|s| s.presses != 0)
}

pub(super) fn admission(origin: &NativeInputOrigin) -> Result<(), Error> {
    STATE.with_borrow(|s| {
        if s.poisoned {
            return Err(Error::IdentityExhausted);
        }
        if s.presses != 0 {
            return Err(Error::UnsupportedHardwareContext);
        }
        if s.refused != 0 {
            return Err(Error::UnsupportedNativeEntry);
        }
        if s.tasks > 1 || s.events > 1 || s.native > 1 {
            return Err(Error::UnsupportedReentrancy);
        }
        match s.top {
            Some(Entry {
                serial,
                kind: Kind::Edit,
            }) => {
                let Some(phase) = &s.phase else {
                    return Err(Error::UnsupportedNativeEntry);
                };
                if phase.serial == Some(serial)
                    && phase.origin == *origin
                    && matches!(phase.phase, Phase::EditDispatch | Phase::PasteFallback)
                {
                    Ok(())
                } else {
                    Err(Error::UnsupportedNativeEntry)
                }
            }
            Some(Entry {
                kind: Kind::Task, ..
            }) if s.native == 0 && s.events == 0 => Ok(()),
            Some(_) => Err(Error::UnsupportedNativeEntry),
            None => Err(Error::UnprotectedEntry),
        }
    })
}

pub(super) fn quiescent() -> bool {
    STATE.with_borrow(|s| {
        !s.poisoned
            && s.native == 0
            && s.events == 0
            && s.tasks == 0
            && s.loans == 0
            && s.top.is_none()
            && s.phase.is_none()
    })
}

#[cfg(test)]
pub(crate) fn loan_count_for_test() -> u32 {
    STATE.with_borrow(|s| s.loans)
}
#[cfg(test)]
mod tests;
