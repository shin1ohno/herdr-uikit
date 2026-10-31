use super::*;

use state::Acquire;

#[test]
fn flushes_without_a_request_leave_first_responder_alone() {
    // UIKit gave first responder to another responder, or the view has it.
    assert!(!Acquire::default().wants_first_responder());
    assert!(!Acquire::Settled.wants_first_responder());
}

#[test]
fn a_request_takes_first_responder_once() {
    let requested = Acquire::Requested;
    assert!(requested.wants_first_responder());
    let after = requested.attempted(true);
    assert_eq!(after, Acquire::Settled);
    assert!(
        !after.wants_first_responder(),
        "later flushes do not take it back"
    );
}

#[test]
fn a_failed_request_retries_and_reports_once() {
    let requested = Acquire::Requested;
    assert!(requested.reports_failure());
    let retrying = requested.attempted(false);
    assert_eq!(retrying, Acquire::Retrying);
    assert!(retrying.wants_first_responder());
    assert!(!retrying.reports_failure());
    assert_eq!(retrying.attempted(false), Acquire::Retrying);
    assert_eq!(retrying.attempted(true), Acquire::Settled);
}
