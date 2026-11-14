use super::host::{Host, Job, Record, capture, check, require, task};
use gpui::{AsyncWindowContext, WindowHandle};
use herdr_uikit::{
    NativeEditAction as Action, NativeRetirementRequestStatus as Status, retirement_probe as native,
};
use std::{cell::RefCell, rc::Rc};

pub(super) fn edits(
    handle: WindowHandle<Host>,
    app: AsyncWindowContext,
    record: Rc<RefCell<Record>>,
    index: usize,
) {
    let job = match index {
        0..=7 => Job::Proceed,
        8 => Job::Fallback,
        9 => Job::InterruptQuery,
        _ => Job::Cancel,
    };
    *record.borrow_mut() = Record {
        job,
        ..Record::default()
    };
    let old = capture(handle);
    old.mark("same");
    record.borrow_mut().events.clear();
    if job == Job::Fallback {
        check(
            native::set_fixture_pasteboard("fixed-fallback"),
            "fixture-pasteboard",
        );
    }
    let action = if index < 8 {
        [Action::Copy, Action::Cut, Action::Paste, Action::SelectAll][index % 4]
    } else {
        Action::Paste
    };
    check(
        native::edit(handle.into(), action, (4..8).contains(&index)),
        "fixture-native-edit",
    );
    check(
        record.borrow().prepare == 0,
        "prepare-waits-for-outer-task-tail",
    );
    // Observe on a later raw probe turn, after the real dispatcher epilogue.
    native::next_native_turn(move || {
        let receipt = record.borrow().receipt.clone().unwrap_or_else(|| {
            check(false, "edit-receipt-present");
            unreachable!()
        });
        eprintln!(
            "B1_PROBE_RECEIPT: case={index} key={:?} status={:?}",
            receipt.key(),
            receipt.status()
        );
        match job {
            Job::InterruptQuery => {
                check(
                    matches!(
                        receipt.status(),
                        Status::InterruptedBeforeFence {
                            old_fenced_at_submission: false,
                            ..
                        }
                    ),
                    "nested-origin-classification",
                );
                check(
                    record.borrow().prepare == 0 && record.borrow().completed == 0,
                    "interruption-no-prepare-successor",
                );
                check(
                    capture(handle).identity() == old.identity(),
                    "interruption-preserves-live-origin",
                );
            }
            Job::Cancel => {
                check(
                    receipt.status() == Status::Declined { old_fenced: true },
                    "declined-owner-never-activated",
                );
                check(
                    record.borrow().prepare == 1
                        && record.borrow().completed == 0
                        && !record.borrow().acknowledged,
                    "cancel-ticket-terminal-order",
                );
                let count = record.borrow().commits.len();
                old.insert("late-cancel");
                old.unmark();
                check(
                    record.borrow().commits.len() == count,
                    "queued-retry-preserves-existing-fence",
                );
            }
            _ => {
                check(
                    matches!(
                        receipt.status(),
                        Status::Replaced {
                            first_responder: true,
                            ..
                        }
                    ),
                    "replacement-responder-acquired-before-input",
                );
                check(
                    record.borrow().prepare == 1
                        && record.borrow().completed == 1
                        && record.borrow().acknowledged,
                    "owner-revision-ack-once",
                );
                if job == Job::Fallback {
                    check(
                        record.borrow().commits == ["fixed-fallback"],
                        "paste-fallback-once",
                    );
                }
                record.borrow_mut().commits.clear();
                record.borrow_mut().job = Job::Idle;
                old.insert("same");
                old.mark("same");
                old.unmark();
                check(
                    record.borrow().commits.is_empty(),
                    "old-native-origin-no-leak",
                );
                let current = capture(handle);
                check(
                    current.identity() != old.identity(),
                    "replacement-native-identity",
                );
                current.insert("a");
                current.mark("same");
                current.unmark();
                current.unmark();
                current.mark("前😀");
                current.delete();
                current.unmark();
                check(
                    record.borrow().commits == ["a", "same", "前"],
                    "new-native-text-once",
                );
            }
        }
        if index < 10 {
            let next = app.clone();
            task(&app, move || edits(handle, next, record, index + 1));
        } else {
            let mut app = app;
            let second = require(app.update(|_, cx| super::open(cx)), "fixture-second-window");
            super::mixed::ready(handle, second.0, second.1, second.2, 0);
        }
    });
}

pub(super) fn default_refusals(handle: WindowHandle<Host>, record: &Rc<RefCell<Record>>) {
    *record.borrow_mut() = Record::default();
    let client = capture(handle);
    record.borrow_mut().refusal_on_replace = true;
    client.insert("direct");
    client.mark("b1-unmark");
    client.unmark();
    client.unmark();
    check(
        record.borrow().commits == ["direct", "b1-unmark"],
        "default-unmark-commit-once",
    );
    check(
        record.borrow().refused == 2,
        "unsupported-text-entry-no-retirement",
    );
    record.borrow_mut().refusal_on_replace = false;
    record.borrow_mut().refusal_on_query = true;
    let _ = client.document_length_utf16();
    check(record.borrow().refused == 3, "query-loan-is-not-admission");
}
