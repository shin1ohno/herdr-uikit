//! The presses a Window has seen begin and not yet end.
//!
//! Every press selector of the text and drawing views feeds this ledger right
//! after its refusal frame opens, before anything else in its body. A native
//! retirement request is refused while the ledger or the text view's key state
//! (the second witness) says a press is held.
//!
//! Std only: the host tests (`tests/press_ledger.rs`) and the extracted-source
//! fixture (`scripts/check-b1-source.py`) compile this file without UIKit.
//!
//! Fail-closed cases: a lost release keeps its entry; a key-less press pairs
//! only with the release of the same press object; more than [`CAPACITY`]
//! distinct presses set `overflow`, which nothing clears. Nothing here clears
//! entries on its own (no deactivation or timer clear): a keyed entry leaves
//! when the same key is pressed and released again; a key-less entry or
//! overflow stays until the Window is rebuilt.

use std::time::{Duration, Instant};

/// More than a HID boot keyboard reports at once (6 keys and 8 modifiers).
pub(crate) const CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PressPhase {
    Began,
    Changed,
    Ended,
    Cancelled,
}

/// One press as a selector received it: its HID usage when it has a key, and
/// the press object's address. `pin` keeps the object alive while its entry
/// exists, so the address cannot be reused by a different press meanwhile.
pub(crate) struct PressFact<P> {
    pub(crate) usage: Option<isize>,
    pub(crate) identity: usize,
    pub(crate) pin: P,
}

impl<P> PressFact<P> {
    pub(crate) fn new(usage: Option<isize>, identity: usize, pin: P) -> Self {
        Self {
            usage,
            identity,
            pin,
        }
    }

    fn id(&self) -> Id {
        match self.usage {
            Some(usage) => Id::Usage(usage),
            None => Id::Identity(self.identity),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Id {
    Usage(isize),
    Identity(usize),
}

struct Entry<P> {
    id: Id,
    since: Instant,
    _pin: P,
}

pub(crate) struct PressLedger<P> {
    entries: Vec<Entry<P>>,
    overflow: bool,
}

impl<P> Default for PressLedger<P> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            overflow: false,
        }
    }
}

/// What the text view's key state says is in flight: non-modifier key
/// presses, a repeat timer, a menu key command's keystroke, held modifiers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyWitness {
    pub(crate) presses: usize,
    pub(crate) repeat: bool,
    pub(crate) menu_press: bool,
    pub(crate) modifiers: bool,
}

impl KeyWitness {
    fn held(self) -> bool {
        self.presses != 0 || self.repeat || self.menu_press || self.modifiers
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeldReason {
    /// More presses than [`CAPACITY`] were in flight at some point.
    Overflow,
    /// A press without a key has not ended.
    Unkeyed,
    /// A press with a key has not ended.
    Keyed,
    /// The ledger is empty but the key state is not: a press selector did not
    /// feed the ledger. Still refused.
    KeyStateOnly,
}

/// The ledger's counts for observation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LedgerSummary {
    pub(crate) keyed: u8,
    pub(crate) unkeyed: u8,
    pub(crate) overflow: bool,
    pub(crate) key_state_only: bool,
    pub(crate) oldest: Option<Duration>,
}

pub(crate) fn apply<P>(
    ledger: &mut PressLedger<P>,
    phase: PressPhase,
    facts: impl IntoIterator<Item = PressFact<P>>,
    now: Instant,
) {
    for fact in facts {
        let id = fact.id();
        let known = ledger.entries.iter().position(|entry| entry.id == id);
        match (phase, known) {
            (PressPhase::Began | PressPhase::Changed, None) => {
                if ledger.entries.len() >= CAPACITY {
                    ledger.overflow = true;
                } else {
                    ledger.entries.push(Entry {
                        id,
                        since: now,
                        _pin: fact.pin,
                    });
                }
            }
            (PressPhase::Began | PressPhase::Changed, Some(_)) => {}
            (PressPhase::Ended | PressPhase::Cancelled, Some(index)) => {
                ledger.entries.remove(index);
            }
            (PressPhase::Ended | PressPhase::Cancelled, None) => {}
        }
    }
}

pub(crate) fn held<P>(ledger: &PressLedger<P>, witness: KeyWitness) -> Option<HeldReason> {
    if ledger.overflow {
        Some(HeldReason::Overflow)
    } else if ledger
        .entries
        .iter()
        .any(|entry| matches!(entry.id, Id::Identity(_)))
    {
        Some(HeldReason::Unkeyed)
    } else if !ledger.entries.is_empty() {
        Some(HeldReason::Keyed)
    } else if witness.held() {
        Some(HeldReason::KeyStateOnly)
    } else {
        None
    }
}

pub(crate) fn summary<P>(
    ledger: &PressLedger<P>,
    witness: KeyWitness,
    now: Instant,
) -> LedgerSummary {
    let count = |unkeyed: bool| {
        let n = ledger
            .entries
            .iter()
            .filter(|entry| matches!(entry.id, Id::Identity(_)) == unkeyed)
            .count();
        u8::try_from(n).unwrap_or(u8::MAX)
    };
    LedgerSummary {
        keyed: count(false),
        unkeyed: count(true),
        overflow: ledger.overflow,
        key_state_only: held(ledger, witness) == Some(HeldReason::KeyStateOnly),
        oldest: ledger
            .entries
            .iter()
            .map(|entry| now.saturating_duration_since(entry.since))
            .max(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyed(usage: isize) -> PressFact<()> {
        PressFact::new(Some(usage), 0, ())
    }
    fn unkeyed(identity: usize) -> PressFact<()> {
        PressFact::new(None, identity, ())
    }
    fn step(ledger: &mut PressLedger<()>, phase: PressPhase, facts: Vec<PressFact<()>>) {
        apply(ledger, phase, facts, Instant::now());
    }
    const NONE: KeyWitness = KeyWitness {
        presses: 0,
        repeat: false,
        menu_press: false,
        modifiers: false,
    };
    const KEY_D: isize = 7;
    const LEFT_GUI: isize = 227;

    // S1
    #[test]
    fn keyed_began_holds() {
        let mut ledger = PressLedger::default();
        assert_eq!(held(&ledger, NONE), None, "empty-ledger-admits");
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Keyed),
            "keyed-began-holds"
        );
    }

    // S2
    #[test]
    fn release_and_repress_recover() {
        let mut ledger = PressLedger::default();
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Ended, vec![keyed(KEY_D)]);
        assert_eq!(held(&ledger, NONE), None, "release-clears-usage");
        // A lost release keeps the entry; the same key pressed and released
        // again clears it.
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        assert!(held(&ledger, NONE).is_some(), "lost-release-holds");
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Ended, vec![keyed(KEY_D)]);
        assert_eq!(held(&ledger, NONE), None, "repress-recovers");
    }

    // S2 (G3): Cancelled is a release too.
    #[test]
    fn cancelled_releases() {
        let mut ledger = PressLedger::default();
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Cancelled, vec![keyed(KEY_D)]);
        assert_eq!(held(&ledger, NONE), None, "cancel-clears-usage");
    }

    // G3 / N1: releasing D while Cmd stays down leaves Cmd held.
    #[test]
    fn release_removes_only_its_own_press() {
        let mut ledger = PressLedger::default();
        step(&mut ledger, PressPhase::Began, vec![keyed(LEFT_GUI)]);
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Ended, vec![keyed(KEY_D)]);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Keyed),
            "release-keeps-other-usage"
        );
        let summary = summary(&ledger, NONE, Instant::now());
        assert_eq!(summary.keyed, 1, "release-keeps-other-usage");
        step(&mut ledger, PressPhase::Ended, vec![keyed(LEFT_GUI)]);
        assert_eq!(held(&ledger, NONE), None, "modifier-release-clears");
    }

    // G3: a release of an unknown usage changes nothing.
    #[test]
    fn unknown_release_keeps_held_press() {
        let mut ledger = PressLedger::default();
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Ended, vec![keyed(LEFT_GUI)]);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Keyed),
            "unknown-release-keeps-held"
        );
    }

    // S3
    #[test]
    fn forwarded_press_counted_once() {
        let mut ledger = PressLedger::default();
        // The text view and the drawing view both see the same press.
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Ended, vec![keyed(KEY_D)]);
        assert_eq!(held(&ledger, NONE), None, "same-usage-dedup");
    }

    // S4
    #[test]
    fn unkeyed_press_paired_by_identity() {
        let mut ledger = PressLedger::default();
        step(&mut ledger, PressPhase::Began, vec![unkeyed(0x1000)]);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Unkeyed),
            "unkeyed-held"
        );
        step(&mut ledger, PressPhase::Ended, vec![unkeyed(0x2000)]);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Unkeyed),
            "unkeyed-identity"
        );
        step(&mut ledger, PressPhase::Ended, vec![unkeyed(0x1000)]);
        assert_eq!(held(&ledger, NONE), None, "unkeyed-release-clears");
    }

    // S5 (H1: overflow is not cleared by releases).
    #[test]
    fn overflow_stays_held() {
        let mut ledger = PressLedger::default();
        let presses: Vec<_> = (0..=CAPACITY as isize).map(keyed).collect();
        step(&mut ledger, PressPhase::Began, presses);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Overflow),
            "overflow-fail-closed"
        );
        let releases: Vec<_> = (0..=CAPACITY as isize).map(keyed).collect();
        step(&mut ledger, PressPhase::Ended, releases);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Overflow),
            "overflow-survives-drain"
        );
    }

    // S5: exactly CAPACITY distinct presses do not overflow.
    #[test]
    fn capacity_presses_fit() {
        let mut ledger = PressLedger::default();
        let presses: Vec<_> = (0..CAPACITY as isize).map(keyed).collect();
        step(&mut ledger, PressPhase::Began, presses);
        assert!(
            !summary(&ledger, NONE, Instant::now()).overflow,
            "capacity-fits"
        );
        let releases: Vec<_> = (0..CAPACITY as isize).map(keyed).collect();
        step(&mut ledger, PressPhase::Ended, releases);
        assert_eq!(held(&ledger, NONE), None, "capacity-fits");
    }

    // S6
    #[test]
    fn changed_unseen_press_holds() {
        let mut ledger = PressLedger::default();
        step(&mut ledger, PressPhase::Changed, vec![keyed(KEY_D)]);
        assert_eq!(
            held(&ledger, NONE),
            Some(HeldReason::Keyed),
            "changed-unseen-holds"
        );
        step(&mut ledger, PressPhase::Changed, vec![keyed(KEY_D)]);
        step(&mut ledger, PressPhase::Ended, vec![keyed(KEY_D)]);
        assert_eq!(held(&ledger, NONE), None, "changed-known-no-duplicate");
    }

    // S7
    #[test]
    fn key_state_witness_holds_with_empty_ledger() {
        let ledger = PressLedger::<()>::default();
        for (witness, label) in [
            (
                KeyWitness { presses: 1, ..NONE },
                "key-state-presses-witness",
            ),
            (
                KeyWitness {
                    repeat: true,
                    ..NONE
                },
                "key-state-repeat-witness",
            ),
            (
                KeyWitness {
                    menu_press: true,
                    ..NONE
                },
                "key-state-menu-witness",
            ),
            (
                KeyWitness {
                    modifiers: true,
                    ..NONE
                },
                "key-state-modifiers-witness",
            ),
        ] {
            assert_eq!(
                held(&ledger, witness),
                Some(HeldReason::KeyStateOnly),
                "{label}"
            );
            assert!(
                summary(&ledger, witness, Instant::now()).key_state_only,
                "{label}"
            );
        }
    }

    #[test]
    fn summary_counts_and_age() {
        let mut ledger = PressLedger::default();
        let start = Instant::now();
        apply(&mut ledger, PressPhase::Began, vec![keyed(KEY_D)], start);
        apply(
            &mut ledger,
            PressPhase::Began,
            vec![unkeyed(0x1000)],
            start + Duration::from_secs(1),
        );
        let summary = summary(&ledger, NONE, start + Duration::from_secs(3));
        assert_eq!(
            summary,
            LedgerSummary {
                keyed: 1,
                unkeyed: 1,
                overflow: false,
                key_state_only: false,
                oldest: Some(Duration::from_secs(3)),
            },
            "hold-summary"
        );
    }
}
