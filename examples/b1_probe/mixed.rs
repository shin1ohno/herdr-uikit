//! Real-window mixed API fixture; OS retargeting remains outside this test.
use std::{cell::RefCell, rc::Rc};

use gpui::{AsyncWindowContext, WindowHandle};
use herdr_uikit::{
    NativeRetirementRequestError as Error, NativeRetirementRequestStatus as Status,
    PrepareDecision, TextInputRetirementError, request_native_edit_retirement,
    retire_text_input_session, retirement_probe as native, text_input_session_state,
};

use super::host::{Host, Record, capture, check, frame, origin, require, task};

pub(super) fn ready(
    a: WindowHandle<Host>,
    b: WindowHandle<Host>,
    app: AsyncWindowContext,
    record: Rc<RefCell<Record>>,
    count: usize,
) {
    if !native::capture(b.into()).is_some_and(|client| client.attached()) {
        check(count < 30, "fixture-second-scene-attached");
        let next = app.clone();
        frame(app, move || ready(a, b, next, record, count + 1));
        return;
    }
    let next = app.clone();
    task(&app, move || cross_product(a, b, next, record, 0));
}

fn cross_product(
    a: WindowHandle<Host>,
    b: WindowHandle<Host>,
    app: AsyncWindowContext,
    record: Rc<RefCell<Record>>,
    index: usize,
) {
    // A was fenced by the previous cancellation. B is initially live; each
    // interrupted live case preserves its endpoint for the next case.
    let (target, other, fenced) = if index.is_multiple_of(2) {
        (a, b, true)
    } else {
        (b, a, false)
    };
    let old = capture(target);
    check(
        text_input_session_state(target.into()).is_some_and(|s| s.retired == fenced),
        "mixed-prior-fence-setup",
    );
    let expected = origin(target);
    let receipt = require(
        request_native_edit_retirement(target.into(), expected.clone(), |_| {
            check(false, "interrupted-request-never-prepares");
            PrepareDecision::Decline
        }),
        "mixed-request-accepted",
    );
    check(
        receipt.status()
            == Status::Queued {
                old_fenced_at_submission: fenced,
            },
        "queued-prior-fence-captured",
    );
    check(
        matches!(
            request_native_edit_retirement(target.into(), expected, |_| PrepareDecision::Decline),
            Err(Error::InProgress)
        ),
        "same-window-one-slot",
    );
    check(
        matches!(
            retire_text_input_session(target.into(), |_| check(false, "refused-legacy-callback")),
            Err(TextInputRetirementError::InProgress)
        ),
        "refused-legacy-noninterference",
    );
    check(
        matches!(
            request_native_edit_retirement(other.into(), origin(other), |_| {
                PrepareDecision::Decline
            }),
            Err(Error::NewCapacityBusy)
        ),
        "cross-window-new-capacity-one",
    );
    let next_app = app.clone();
    let observed = receipt.clone();
    if index < 2 {
        let legacy = require(
            retire_text_input_session(other.into(), move |_| {
                check(
                    matches!(observed.status(), Status::InterruptedBeforeFence { old_fenced_at_submission, .. } if old_fenced_at_submission == fenced),
                    "new-terminal-precedes-legacy-drain",
                );
                check(
                    text_input_session_state(target.into())
                        .is_some_and(|s| !s.replacement_pending && s.retired == fenced),
                    "new-slot-released-before-legacy-drain",
                );
                check(
                    matches!(
                        request_native_edit_retirement(target.into(), origin(target), |_| {
                            PrepareDecision::Decline
                        }),
                        Err(Error::LegacyDispatchActive)
                    ),
                    "legacy-drain-does-not-refill-new",
                );
            }),
            "cross-window-legacy-still-accepted",
        );
        // Keep the legacy receipt alive only for observation, not ownership.
        drop(legacy);
    } else {
        let _ = old.selected_range_utf16();
    }
    native::next_native_turn(move || {
        check(
            matches!(receipt.status(), Status::InterruptedBeforeFence { old_fenced_at_submission, .. } if old_fenced_at_submission == fenced),
            "queued-retry-preserves-existing-fence",
        );
        check(
            text_input_session_state(target.into())
                .is_some_and(|s| s.retired == fenced && !s.replacement_pending),
            "interruption-releases-with-prior-fence",
        );
        if index < 3 {
            // Cross-window legacy in case 1 replaces A. Restore only by a NEW
            // explicit request + Decline, never by reopening the old token.
            if index == 1 {
                let queued_app = next_app.clone();
                task(&next_app, move || {
                    let declined = require(
                        request_native_edit_retirement(a.into(), origin(a), |_| {
                            PrepareDecision::Decline
                        }),
                        "restore-fenced-fixture-by-decline",
                    );
                    native::next_native_turn(move || {
                        check(
                            declined.status() == Status::Declined { old_fenced: true },
                            "decline-does-not-construct-successor",
                        );
                        let after = queued_app.clone();
                        task(&queued_app, move || {
                            cross_product(a, b, after, record, index + 1)
                        });
                    });
                });
            } else {
                let after = next_app.clone();
                task(&next_app, move || {
                    cross_product(a, b, after, record, index + 1)
                });
            }
        } else {
            let after = next_app.clone();
            task(&next_app, move || prepare_legacy(a, b, after, record));
        }
    });
}

fn prepare_legacy(
    a: WindowHandle<Host>,
    b: WindowHandle<Host>,
    app: AsyncWindowContext,
    record: Rc<RefCell<Record>>,
) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let receipt = require(
        request_native_edit_retirement(a.into(), origin(a), move |_| {
            log.borrow_mut().push("prepare");
            let legacy_log = log.clone();
            require(
                retire_text_input_session(b.into(), move |_| {
                    legacy_log.borrow_mut().push("legacy");
                    check(
                        text_input_session_state(a.into()).is_some_and(|s| !s.replacement_pending),
                        "preparing-slot-released-before-legacy",
                    );
                }),
                "preparing-allows-other-legacy",
            );
            check(*log.borrow() == ["prepare"], "legacy-held-through-prepare");
            PrepareDecision::Proceed(Box::new(move |_| {
                log.borrow_mut().push("terminal");
            }))
        }),
        "retry-after-interruption",
    );
    native::next_native_turn(move || {
        check(
            matches!(receipt.status(), Status::Replaced { .. }),
            "own-proceed-installs-successor",
        );
        check(
            *seen.borrow() == ["prepare", "terminal", "legacy"],
            "mixed-api-order-and-new-batch-one",
        );
        super::cases::default_refusals(b, &record);
        super::raw::run(a, b, app);
    });
}
