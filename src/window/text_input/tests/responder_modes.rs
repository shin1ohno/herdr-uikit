use super::*;

use state::{Acquire, Mode};

#[test]
fn key_only_cancels_pending_text_view_acquisition() {
    assert!(Acquire::for_mode(Some(Mode::Text)).wants_first_responder());
    assert_eq!(Acquire::for_mode(Some(Mode::KeyOnly)), Acquire::Settled);
    assert!(!Acquire::for_mode(None).wants_first_responder());
}

#[test]
fn an_explicit_text_request_reacquires_after_a_successful_old_request() {
    let settled = Acquire::for_mode(Some(Mode::Text)).attempted(true);
    assert!(!settled.wants_first_responder());
    // UIKit may have dismissed the keyboard since that settled request.
    assert_eq!(Acquire::for_mode(Some(Mode::Text)), Acquire::Requested);
}

#[test]
fn an_explicit_request_restarts_only_a_minimized_keyboard() {
    let request = Acquire::for_mode(Some(Mode::Text));
    assert!(!request.needs_responder_restart(false));
    assert!(request.needs_responder_restart(true));

    // Repeated requests with a hardware keyboard and no WillHide evidence
    // preserve the responder, including any active marked text.
    let request = request.attempted(true);
    assert!(!request.needs_responder_restart(false));
    let request = Acquire::for_mode(Some(Mode::Text));
    assert!(!request.needs_responder_restart(false));
}

#[test]
fn a_flush_or_retry_does_not_restart_a_minimized_keyboard() {
    for acquire in [Acquire::Settled, Acquire::Requested.attempted(false)] {
        assert!(!acquire.needs_responder_restart(true));
    }
    assert!(!Acquire::for_mode(Some(Mode::KeyOnly)).needs_responder_restart(true));
    assert!(!Acquire::for_mode(None).needs_responder_restart(true));
}
