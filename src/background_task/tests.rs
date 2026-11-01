#![allow(clippy::unwrap_used)]

use std::{cell::RefCell, rc::Rc, thread};

use super::*;

/// Identifiers `end` was called with, in order.
type Ended = Rc<RefCell<Vec<u32>>>;

fn recorder() -> (Ended, impl Fn(u32)) {
    let ended = Ended::default();
    let record = Rc::clone(&ended);
    (ended, move |identifier| {
        record.borrow_mut().push(identifier)
    })
}

#[test]
fn a_refused_task_is_never_ended() {
    let (ended, end) = recorder();

    assert!(Claim::begin(|_| None::<u32>, &end).is_none());
    assert!(ended.borrow().is_empty());
}

#[test]
fn the_owner_ends_a_running_task_once() {
    let (ended, end) = recorder();
    let claim = Claim::begin(|_| Some(7), &end).unwrap();

    claim.end(&end);
    claim.end(&end);

    assert_eq!(*ended.borrow(), [7]);
}

#[test]
fn an_expired_task_is_not_ended_again_by_its_owner() {
    let (ended, end) = recorder();
    let mut expiry = None;
    let claim = Claim::begin(
        |claim| {
            expiry = Some(claim);
            Some(3)
        },
        &end,
    )
    .unwrap();

    expiry.unwrap().end(&end);
    claim.end(&end);

    assert_eq!(*ended.borrow(), [3]);
}

#[test]
fn a_task_that_expires_while_starting_ends_at_once() {
    let (ended, end) = recorder();
    let claim = Claim::begin(
        |claim| {
            // The handler fires before the system returned the identifier:
            // there is nothing to end yet.
            claim.end(&end);
            Some(5)
        },
        &end,
    );

    assert!(claim.is_none());
    assert_eq!(*ended.borrow(), [5]);
}

#[test]
fn a_claim_ended_from_two_threads_ends_once() {
    let claim = Claim::begin(|_| Some(9_u32), |_| ()).unwrap();
    let ends = Arc::new(Mutex::new(Vec::new()));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let claim = Arc::clone(&claim);
            let ends = Arc::clone(&ends);
            thread::spawn(move || claim.end(|identifier| ends.lock().unwrap().push(identifier)))
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }

    assert_eq!(*ends.lock().unwrap(), [9]);
}
