//! These exercise production observation state, NOT actual UIKit callbacks.
#![allow(clippy::unwrap_used, reason = "test setup must fail immediately")]
use super::*;
use std::sync::{Mutex, MutexGuard};
static LOCK: Mutex<()> = Mutex::new(());
pub(super) fn setup() -> (MutexGuard<'static, ()>, Registration, Arm) {
    let lock = LOCK.lock().unwrap();
    ENABLED.store(false, SeqCst);
    DROPPED.store(0, SeqCst);
    INVALID.store(0, SeqCst);
    VERSION.store(0, SeqCst);
    KEY.set(None);
    RESPONDER.set(ResponderOrigin::Text);
    let mut q = Queue::new(ProcessRun(17));
    let arm = q.arm(ProcessRun(17), Phase(3), 0).unwrap();
    STATE.with(|s| *s.borrow_mut() = Some(q));
    ENABLED.store(true, SeqCst);
    (lock, Registration(Cell::new(arm.view_generation)), arm)
}
fn invocation(reg: &Registration) -> InvocationGuard {
    InvocationGuard::enter(reg, Action::Copy)
}
fn complete(inv: InvocationGuard) {
    inv.complete(
        Decision::AllowedExplicitPolicy,
        Outcome::Dispatched {
            key_down_handled: true,
            fallback: Fallback::NotEligible,
        },
    );
}
#[test]
fn disabled_state_does_not_initialize_queue_or_allocate_identity() {
    let (_lock, reg, _) = setup();
    reg.remove();
    ENABLED.store(false, SeqCst);
    STATE.with(|s| *s.borrow_mut() = None);
    DROPPED.store(0, SeqCst);
    INVALID.store(0, SeqCst);
    VERSION.store(0, SeqCst);
    complete(invocation(&Registration::default()));
    let _key = KeyScope::enter(None);
    let _drawing = DrawingScope::enter();
    assert_eq!(current_native_observation_key(), None);
    assert_eq!(DROPPED.load(SeqCst), 0);
    assert_eq!(INVALID.load(SeqCst), 0);
    assert_eq!(VERSION.load(SeqCst), 0);
    STATE.with(|s| assert!(s.borrow().is_none()));
}
#[test]
fn contention_uses_independent_atomic_invalidity() {
    let (_lock, reg, arm) = setup();
    STATE.with(|s| {
        let _held = s.borrow_mut();
        assert!(invocation(&reg).key().is_none());
    });
    let status = with_state(|q| snapshot(q, arm.run)).unwrap();
    assert!(status.sticky_invalid);
    assert_eq!(status.dropped_total, 1);
    assert_ne!(status.invalid_reasons & InvalidReason::Contention.bit(), 0);
    assert_eq!(status.outstanding, 0);
}
#[test]
fn callback_drain_snapshot_does_not_borrow_across_key_scope() {
    let (_lock, reg, arm) = setup();
    let inv = invocation(&reg);
    let key = inv.key();
    {
        let _scope = KeyScope::enter(key);
        let status = with_state(|q| snapshot(q, arm.run)).unwrap();
        assert_eq!(status.outstanding, 1);
        assert_eq!(status.reserved, 1);
        assert!(!status.sticky_invalid);
        assert_eq!(current_native_observation_key(), key);
        with_state(|q| {
            q.clear_records();
            Ok(())
        })
        .unwrap();
    }
    complete(inv);
    assert_eq!(with_state(|q| Ok(q.len)).unwrap(), 1);
}
#[test]
fn nested_native_physical_modifier_fallback_scopes_restore() {
    let (_lock, reg, _) = setup();
    let outer = invocation(&reg);
    let outer_key = outer.key();
    let outer_scope = KeyScope::enter(outer_key);
    {
        let _physical_or_modifier = KeyScope::enter(None);
        assert_eq!(current_native_observation_key(), None);
        let inner = invocation(&reg);
        let inner_key = inner.key();
        {
            let _native = KeyScope::enter(inner_key);
            assert_eq!(current_native_observation_key(), inner_key);
        }
        assert_eq!(current_native_observation_key(), None);
        complete(inner);
    }
    assert_eq!(current_native_observation_key(), outer_key);
    drop(outer_scope);
    assert_eq!(current_native_observation_key(), None);
    complete(outer);
}
#[test]
fn drawing_origin_is_consumed_only_at_canonical_entry() {
    let (_lock, reg, _) = setup();
    let _drawing = DrawingScope::enter();
    let outer = invocation(&reg);
    let inner = invocation(&reg);
    complete(inner);
    complete(outer);
    with_state(|q| {
        let records = q.copy_records();
        assert_eq!(records[0].unwrap().responder, ResponderOrigin::Drawing);
        assert_eq!(records[1].unwrap().responder, ResponderOrigin::Text);
        Ok(())
    })
    .unwrap();
}
#[test]
fn unwind_closes_reserved_outcome_and_marks_invalid() {
    let (_lock, reg, arm) = setup();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _invocation = invocation(&reg);
        panic!("fixed observer unwind test");
    }));
    assert!(result.is_err());
    with_state(|q| {
        let status = snapshot(q, arm.run)?;
        assert!(status.sticky_invalid);
        assert_eq!(status.outstanding, 0);
        assert_eq!(
            q.copy_records()[1].unwrap().event,
            Event::Outcome(Outcome::Aborted)
        );
        Ok(())
    })
    .unwrap();
}
#[test]
fn disabled_or_teardown_outstanding_stays_invalid_after_return() {
    let (_lock, reg, arm) = setup();
    let inv = invocation(&reg);
    reg.remove();
    complete(inv);
    with_state(|q| {
        let first = snapshot(q, arm.run)?;
        assert!(first.sticky_invalid);
        assert_eq!(first.outstanding, 0);
        assert_eq!(
            q.copy_records()[1].unwrap().event,
            Event::Outcome(Outcome::Invalidated)
        );
        q.clear_records();
        q.enabled = false;
        let second = snapshot(q, arm.run)?;
        assert_eq!(second.dropped_total, first.dropped_total);
        assert!(second.sticky_invalid);
        assert!(second.snapshot_sequence > first.snapshot_sequence);
        Ok(())
    })
    .unwrap();
}
#[test]
fn drop_saturation_is_sticky_and_never_wraps() {
    let (_lock, _reg, arm) = setup();
    DROPPED.store(u64::MAX, SeqCst);
    reject(InvalidReason::Capacity);
    let status = with_state(|q| snapshot(q, arm.run)).unwrap();
    assert_eq!(status.dropped_total, u64::MAX);
    assert_ne!(
        status.invalid_reasons & InvalidReason::CounterOverflow.bit(),
        0
    );
}
#[test]
fn inconsistent_snapshot_fails_instead_of_returning_partial_success() {
    let (_lock, _reg, arm) = setup();
    VERSION.store(1, SeqCst);
    assert_eq!(
        with_state(|q| snapshot(q, arm.run)),
        Err(ObserverError::InvalidSnapshot)
    );
    assert_ne!(
        INVALID.load(SeqCst) & InvalidReason::InvalidSnapshot.bit(),
        0
    );
}
#[test]
fn unknown_action_is_invalid_even_when_it_has_a_refusal_pair() {
    let (_lock, reg, arm) = setup();
    InvocationGuard::enter(&reg, Action::Unknown)
        .complete(Decision::RefusedUnknownSelector, Outcome::Refused);
    with_state(|q| {
        let status = snapshot(q, arm.run)?;
        assert!(status.sticky_invalid);
        assert_eq!(status.queued, 2);
        assert_eq!(status.reserved, 0);
        Ok(())
    })
    .unwrap();
}
#[test]
fn counter_overflow_stops_new_observation_admission() {
    let (_lock, reg, _) = setup();
    reject(InvalidReason::CounterOverflow);
    assert!(invocation(&reg).key().is_none());
    with_state(|q| {
        assert_eq!(q.len, 0);
        assert_eq!(q.depth, 0);
        Ok(())
    })
    .unwrap();
}
