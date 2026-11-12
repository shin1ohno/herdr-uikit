#![allow(
    clippy::unwrap_used,
    reason = "test setup and positive controls must fail immediately"
)]
use super::*;
fn armed() -> (Queue, Arm) {
    let mut queue = Queue::new(ProcessRun(17));
    let arm = queue.arm(ProcessRun(17), Phase(3), 0).unwrap();
    (queue, arm)
}
fn enter(queue: &mut Queue, arm: Arm) -> Record {
    queue
        .enter(arm.view_generation, Action::Copy, ResponderOrigin::Text)
        .unwrap()
}
fn done(queue: &mut Queue, entry: Record) {
    queue
        .complete(
            entry,
            Some(Decision::AllowedExplicitPolicy),
            Outcome::Dispatched {
                key_down_handled: true,
                fallback: Fallback::NotEligible,
            },
        )
        .unwrap();
}
#[test]
fn reserves_both_records_and_rejects_overflow() {
    let (mut q, arm) = armed();
    for _ in 0..127 {
        let entry = enter(&mut q, arm);
        done(&mut q, entry);
    }
    let entry = enter(&mut q, arm);
    assert_eq!(q.len, 255);
    assert_eq!(
        q.enter(arm.view_generation, Action::Cut, ResponderOrigin::Drawing),
        Err(ObserverError::Capacity)
    );
    done(&mut q, entry);
    assert_eq!(q.len, RECORD_CAPACITY);
    assert_eq!(q.depth, 0);
    assert_eq!(
        q.enter(arm.view_generation, Action::Copy, ResponderOrigin::Text),
        Err(ObserverError::Capacity)
    );
}
#[test]
fn nested_completion_matches_identity_not_arrival_order() {
    let (mut q, arm) = armed();
    let outer = enter(&mut q, arm);
    let inner = enter(&mut q, arm);
    assert_eq!(inner.parent, Some(outer.key));
    assert!(inner.key.invocation > outer.key.invocation);
    assert_eq!(
        q.complete(outer, None, Outcome::Aborted),
        Err(InvalidReason::Duplicate)
    );
    done(&mut q, inner);
    done(&mut q, outer);
    let records = q.copy_records();
    assert_eq!(records[2].unwrap().key, inner.key);
    assert_eq!(records[3].unwrap().key, outer.key);
    assert_eq!(
        q.complete(inner, None, Outcome::Aborted),
        Err(InvalidReason::Duplicate)
    );
}
#[test]
fn global_depth_limit_and_drain_preserve_reservations() {
    let (mut q, arm) = armed();
    let mut entries = Vec::new();
    for _ in 0..INVOCATION_CAPACITY {
        entries.push(enter(&mut q, arm));
    }
    assert_eq!(
        q.enter(arm.view_generation, Action::Copy, ResponderOrigin::Text),
        Err(ObserverError::Capacity)
    );
    q.clear_records();
    assert_eq!(q.depth, INVOCATION_CAPACITY);
    for e in entries.into_iter().rev() {
        done(&mut q, e);
    }
    assert_eq!(q.len, INVOCATION_CAPACITY);
}
#[test]
fn stale_arm_and_view_generations_never_reuse() {
    let (mut q, first) = armed();
    assert_eq!(
        q.arm(ProcessRun(18), Phase(4), first.view_generation),
        Err(ObserverError::StaleIdentity)
    );
    assert!(q.remove(first.view_generation));
    assert_eq!(
        q.arm(first.run, Phase(4), first.view_generation),
        Err(ObserverError::StaleIdentity)
    );
    let second = q.arm(first.run, Phase(4), 0).unwrap();
    assert!(second.view_generation > first.view_generation);
    assert!(second.epoch > first.epoch);
    assert!(!q.contains(first));
}
#[test]
fn phase_rearm_refuses_outstanding_and_undrained() {
    let (mut q, arm) = armed();
    let entry = enter(&mut q, arm);
    assert_eq!(
        q.arm(arm.run, Phase(4), arm.view_generation),
        Err(ObserverError::Outstanding)
    );
    done(&mut q, entry);
    assert_eq!(
        q.arm(arm.run, Phase(4), arm.view_generation),
        Err(ObserverError::Undrained)
    );
    q.clear_records();
    let next = q.arm(arm.run, Phase(4), arm.view_generation).unwrap();
    assert!(next.epoch > arm.epoch);
    assert!(!q.contains(arm));
}
#[test]
fn disabled_or_torn_down_return_cannot_be_success() {
    for disable in [true, false] {
        let (mut q, arm) = armed();
        let entry = enter(&mut q, arm);
        if disable {
            q.enabled = false;
        } else {
            q.remove(arm.view_generation);
        }
        done(&mut q, entry);
        assert_eq!(
            q.copy_records()[1].unwrap().event,
            Event::Outcome(Outcome::Invalidated)
        );
    }
}
#[test]
fn all_identity_counters_refuse_wrap() {
    let (mut q, arm) = armed();
    q.invocation = u64::MAX;
    assert_eq!(
        q.enter(arm.view_generation, Action::Copy, ResponderOrigin::Text),
        Err(ObserverError::CounterOverflow)
    );
    assert_eq!(q.len, 0);
    assert_eq!(q.depth, 0);
    q.epoch = u64::MAX;
    assert_eq!(
        q.arm(arm.run, Phase(4), arm.view_generation),
        Err(ObserverError::CounterOverflow)
    );
    q.epoch = 0;
    q.generation = u64::MAX;
    assert_eq!(
        q.arm(arm.run, Phase(4), 0),
        Err(ObserverError::CounterOverflow)
    );
}
#[test]
fn fixed_global_registration_limit() {
    let (mut q, arm) = armed();
    for _ in 1..VIEW_CAPACITY {
        q.arm(arm.run, Phase(3), 0).unwrap();
    }
    assert_eq!(q.arm(arm.run, Phase(3), 0), Err(ObserverError::Capacity));
}
#[test]
fn forged_handled_and_fallback_cannot_complete() {
    let (mut q, arm) = armed();
    let copy = enter(&mut q, arm);
    assert_eq!(
        q.complete(
            copy,
            Some(Decision::AllowedExplicitPolicy),
            Outcome::Dispatched {
                key_down_handled: false,
                fallback: Fallback::Invoked,
            }
        ),
        Err(InvalidReason::InvalidOutcome)
    );
    assert_eq!(
        q.complete(
            copy,
            Some(Decision::AllowedExplicitPolicy),
            Outcome::Refused
        ),
        Err(InvalidReason::InvalidOutcome)
    );
    done(&mut q, copy);
    let paste = q
        .enter(arm.view_generation, Action::Paste, ResponderOrigin::Drawing)
        .unwrap();
    assert_eq!(
        q.complete(
            paste,
            Some(Decision::AllowedDefaultText),
            Outcome::Dispatched {
                key_down_handled: false,
                fallback: Fallback::NotEligible,
            }
        ),
        Err(InvalidReason::InvalidOutcome)
    );
    q.complete(
        paste,
        Some(Decision::AllowedDefaultText),
        Outcome::Dispatched {
            key_down_handled: false,
            fallback: Fallback::Invoked,
        },
    )
    .unwrap();
}
#[test]
fn refusal_and_abort_have_distinct_payloads() {
    let (mut q, arm) = armed();
    let refusal = enter(&mut q, arm);
    q.complete(
        refusal,
        Some(Decision::RefusedNoTextFocus),
        Outcome::Refused,
    )
    .unwrap();
    let aborted = enter(&mut q, arm);
    q.complete(aborted, None, Outcome::Aborted).unwrap();
    let records = q.copy_records();
    assert_eq!(
        records[1].unwrap().decision,
        Some(Decision::RefusedNoTextFocus)
    );
    assert_eq!(records[3].unwrap().decision, None);
    assert_eq!(records[3].unwrap().event, Event::Outcome(Outcome::Aborted));
}
#[test]
fn nested_reservations_cannot_be_stolen_near_full_ring() {
    let (mut q, arm) = armed();
    for _ in 0..126 {
        let e = enter(&mut q, arm);
        done(&mut q, e);
    }
    let outer = enter(&mut q, arm);
    let inner = enter(&mut q, arm);
    assert_eq!(
        q.enter(arm.view_generation, Action::Copy, ResponderOrigin::Text),
        Err(ObserverError::Capacity)
    );
    done(&mut q, inner);
    done(&mut q, outer);
    assert_eq!(q.len, RECORD_CAPACITY);
}
#[test]
fn initial_queue_is_unarmed_and_has_no_registrations() {
    let mut q = Queue::new(ProcessRun(17));
    assert!(!q.enabled);
    assert_eq!(
        q.enter(1, Action::Copy, ResponderOrigin::Text),
        Err(ObserverError::Disabled)
    );
    assert!(q.registrations.iter().all(Option::is_none));
    assert_eq!(
        (q.epoch, q.generation, q.invocation, q.len, q.depth),
        (0, 0, 0, 0, 0)
    );
}
#[test]
fn global_phase_arm_rejects_other_views_previous_epoch() {
    let (mut q, first) = armed();
    let second = q.arm(first.run, Phase(4), 0).unwrap();
    assert!(!q.contains(first));
    assert!(q.contains(second));
    assert_eq!(
        q.enter(first.view_generation, Action::Copy, ResponderOrigin::Text),
        Err(ObserverError::StaleIdentity)
    );
}
