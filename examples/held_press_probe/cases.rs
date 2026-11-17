//! The cases run in order on one window. Each press ends before the next case,
//! so the ledger starts empty; the retired case runs last because the window's
//! text view is replaced there.
use super::{Probe, check, note};
use herdr_uikit::{
    retire_text_input_session,
    retirement_probe::held_press::{
        HeldProbeError, HeldProbePhase::*, HeldProbeScriptError, HeldProbeTarget::*,
    },
};
use std::time::Duration;

const SETTLE: Duration = Duration::from_millis(300);

pub(super) fn start(p: Probe) {
    began_ended(p);
}

/// Began, the SDK's own repeat, then Ended: one KeyDown, held repeats, one
/// KeyUp, and the ledger entry from Began to Ended.
fn began_ended(p: Probe) {
    let press = p.press();
    let before = p.keys();
    check(p.send(&press, Began, Current).is_ok(), "began-sent");
    let keys = p.keys();
    check(
        keys.down == before.down + 1 && keys.held == before.held,
        "began-keydown",
    );
    let hold = p.hold();
    check(hold.keyed == 1 && hold.oldest.is_some(), "began-ledger");
    p.after(Duration::from_millis(1500), move |p| {
        // Nominal (1.5 - 0.5) / 0.033 = 30 repeats; half is the lower bound.
        let repeats = p.keys().held - before.held;
        note(format!("began-ended repeats={repeats} in 1.5 s"));
        check(repeats >= 15, "began-repeats");
        check(p.hold().keyed == 1, "held-ledger-kept");
        check(p.send(&press, Ended, Current).is_ok(), "ended-sent");
        check(p.keys().up == before.up + 1, "ended-keyup");
        let hold = p.hold();
        check(hold.keyed == 0 && !hold.held(), "ended-ledger-clear");
        let stopped = p.keys().held;
        p.after(SETTLE, move |p| {
            check(p.keys().held == stopped, "ended-repeat-stopped");
            changed(p);
        });
    });
}

/// Changed keeps the ledger entry and its age, sends no KeyUp, and goes on
/// to UIResponder's implementation with the injected press.
fn changed(p: Probe) {
    let press = p.press();
    let before = p.keys();
    check(p.send(&press, Began, Current).is_ok(), "changed-began-sent");
    p.after(Duration::from_millis(200), move |p| {
        check(p.send(&press, Changed, Current).is_ok(), "changed-sent");
        let hold = p.hold();
        check(
            hold.keyed == 1
                && hold
                    .oldest
                    .is_some_and(|age| age >= Duration::from_millis(150)),
            "changed-keeps-ledger",
        );
        check(p.keys().up == before.up, "changed-no-keyup");
        check(p.send(&press, Ended, Current).is_ok(), "changed-ended-sent");
        check(p.keys().up == before.up + 1, "changed-ended-keyup");
        check(p.hold().keyed == 0, "changed-ledger-clear");
        p.after(SETTLE, cancelled);
    });
}

/// Cancelled while repeating: one KeyUp, the repeat stops, the ledger clears.
fn cancelled(p: Probe) {
    let press = p.press();
    let before = p.keys();
    check(
        p.send(&press, Began, Current).is_ok(),
        "cancelled-began-sent",
    );
    p.after(Duration::from_millis(1000), move |p| {
        // Nominal (1.0 - 0.5) / 0.033 = 15 repeats.
        let repeats = p.keys().held - before.held;
        note(format!(
            "cancelled repeats-before-cancel={repeats} in 1.0 s"
        ));
        check(repeats >= 7, "cancelled-repeats-before");
        check(p.send(&press, Cancelled, Current).is_ok(), "cancelled-sent");
        check(p.keys().up == before.up + 1, "cancelled-keyup");
        let hold = p.hold();
        check(hold.keyed == 0 && !hold.held(), "cancelled-ledger-clear");
        let stopped = p.keys().held;
        p.after(SETTLE, move |p| {
            check(p.keys().held == stopped, "cancelled-repeat-stopped");
            refusals(p);
        });
    });
}

/// The phase script refuses before anything reaches UIKit.
fn refusals(p: Probe) {
    let press = p.press();
    let before = p.keys();
    check(
        p.send(&press, Ended, Current)
            == Err(HeldProbeError::Script(HeldProbeScriptError::NotBegun)),
        "release-before-began-refused",
    );
    let live = p.client();
    check(
        p.send(&press, Began, Retired(&live)) == Err(HeldProbeError::TargetNotRetired),
        "live-view-is-not-a-retired-target",
    );
    check(
        p.keys() == before && p.hold().keyed == 0,
        "refused-steps-send-nothing",
    );
    retired(p);
}

/// The design's X2 order: Began on the old view, the old view retires, the
/// same press begins again on the new view, and its release reaches the
/// retired view. The retired view sends no KeyUp but the ledger clears.
fn retired(p: Probe) {
    let old = p.client();
    let press = p.press();
    let before = p.keys();
    check(p.send(&press, Began, Current).is_ok(), "retired-began-sent");
    check(p.hold().keyed == 1, "retired-began-ledger");
    let mut app = p.app.clone();
    let retirement =
        app.update(|window, _| retire_text_input_session(window.window_handle(), |_| {}));
    check(matches!(retirement, Ok(Ok(_))), "retirement-accepted");
    let old_identity = old.identity();
    p.clone().after(SETTLE, move |p| {
        check(
            p.client().identity() != old_identity,
            "replacement-installed",
        );
        p.when_ready("replacement-ready", 0, move |p| {
            let base = p.keys();
            note(format!(
                "retired old-view held={} between retirement and redelivery",
                base.held - before.held
            ));
            check(
                p.send(&press, Began, Current).is_ok(),
                "redelivered-began-sent",
            );
            check(p.keys().down == base.down + 1, "redelivered-began-keydown");
            check(p.hold().keyed == 1, "redelivered-ledger-single-entry");
            p.after(Duration::from_millis(1000), move |p| {
                let keys = p.keys();
                check(keys.held - base.held >= 7, "redelivered-repeats");
                check(
                    p.send(&press, Ended, Retired(&old)).is_ok(),
                    "retired-release-sent",
                );
                check(p.keys().up == keys.up, "retired-release-no-keyup");
                let hold = p.hold();
                check(hold.keyed == 0, "retired-release-clears-ledger");
                note(format!(
                    "after retired release key_state_only={} held={}",
                    hold.key_state_only,
                    hold.held()
                ));
                check(
                    p.send(&press, Ended, Current)
                        == Err(HeldProbeError::Script(HeldProbeScriptError::Finished)),
                    "finished-press-refused",
                );
                let held_at = p.keys().held;
                p.after(SETTLE, move |p| {
                    note(format!(
                        "repeat-after-retired-release={} in 0.3 s (recorded, not asserted)",
                        p.keys().held - held_at
                    ));
                    fresh(p);
                });
            });
        });
    });
}

/// A new press after the retired release still reaches GPUI once.
fn fresh(p: Probe) {
    let press = p.press();
    let before = p.keys();
    check(p.send(&press, Began, Current).is_ok(), "fresh-began-sent");
    check(p.keys().down == before.down + 1, "fresh-began-keydown");
    check(p.send(&press, Ended, Current).is_ok(), "fresh-ended-sent");
    check(p.keys().up == before.up + 1, "fresh-ended-keyup");
    let hold = p.hold();
    check(hold.keyed == 0, "final-ledger-clear");
    note(format!(
        "final key_state_only={} held={}",
        hold.key_state_only,
        hold.held()
    ));
    eprintln!(
        "HELD_PRESS_PROBE_PASS: began, repeat, changed, cancelled, ended and retired release through actual selectors; iPadOS delivery UNVERIFIED"
    );
    std::process::exit(0);
}
