//! Actual queued adapter bodies. Nested DispatcherTask inheritance is tested
//! separately by extracted raw-body fixtures; this is not an OS pumping claim.
use std::{cell::Cell, rc::Rc};

use gpui::{AsyncWindowContext, WindowHandle};
use herdr_uikit::{
    NativeRetirementRequestError as Error, PrepareDecision, native_hardware_hold,
    request_native_edit_retirement,
    retirement_probe::{self as native, ProbePhase},
    text_input_session_state,
};

use super::host::{Host, check, origin, task};

pub(super) fn run(a: WindowHandle<Host>, b: WindowHandle<Host>, app: AsyncWindowContext) {
    ingress(a, b, app, 0);
}

fn ingress(a: WindowHandle<Host>, b: WindowHandle<Host>, app: AsyncWindowContext, index: usize) {
    let label = ["shared-later", "scene-refusal", "scene-destroy"][index];
    let seen = Rc::new(Cell::new(false));
    let observed = seen.clone();
    check(
        native::on_raw_ingress(label, move || {
            let before = text_input_session_state(a.into());
            check(
                matches!(
                    request_native_edit_retirement(a.into(), origin(a), |_| {
                        check(false, "raw-ingress-never-prepares");
                        PrepareDecision::Decline
                    }),
                    Err(Error::UnsupportedNativeEntry)
                ),
                "actual-raw-ingress-refuses",
            );
            check(
                text_input_session_state(a.into()) == before,
                "raw-refusal-preserves-native-state",
            );
            observed.set(true);
        }),
        "raw-observer-registration",
    );
    check(
        match index {
            0 => native::raw_later(),
            1 => native::raw_scene_refusal(),
            _ => native::raw_scene_destroy(b.into()),
        },
        "raw-body-scheduled",
    );
    native::next_native_turn(move || {
        check(seen.get(), "actual-raw-body-observed");
        if index < 2 {
            ingress(a, b, app, index + 1);
        } else {
            let next = app.clone();
            task(&app, move || unkeyed_presses(a, next));
        }
    });
}

fn hardware_refused(a: WindowHandle<Host>) -> bool {
    matches!(
        request_native_edit_retirement(a.into(), origin(a), |_| {
            check(false, "held-press-never-prepares");
            PrepareDecision::Decline
        }),
        Err(Error::UnsupportedHardwareContext)
    )
}

fn admitted(a: WindowHandle<Host>) -> bool {
    request_native_edit_retirement(a.into(), origin(a), |_| PrepareDecision::Decline).is_ok()
}

fn press(keyed: bool) -> native::ProbePressHandle {
    match native::probe_press(keyed) {
        Some(press) => press,
        None => {
            check(false, "probe-press-created");
            unreachable!()
        }
    }
}

/// S13, key-less presses on the drawing view. Whether they also reach the
/// text view depends on its first-responder state after the scene cases
/// (`press_target`), so this checks the whole path, not the drawing view's
/// record line alone; that line is fixed by the S9 source guard.
fn unkeyed_presses(a: WindowHandle<Host>, app: AsyncWindowContext) {
    check(
        native::empty_hardware_entry(a.into()),
        "empty-press-selector-reached",
    );
    check(
        native_hardware_hold(a.into()).is_some_and(|hold| !hold.held()),
        "empty-press-leaves-ledger-empty",
    );
    eprintln!(
        "B1_PROBE_NOTE: text-first-responder={:?}",
        text_input_session_state(a.into()).map(|state| state.first_responder)
    );
    let held = press(false);
    check(
        native::inject_press(a.into(), &held, ProbePhase::Began, true),
        "unkeyed-press-injected",
    );
    check(hardware_refused(a), "unkeyed-press-held-refused");
    let other = press(false);
    check(
        native::inject_press(a.into(), &other, ProbePhase::Ended, true),
        "other-unkeyed-release-injected",
    );
    check(hardware_refused(a), "unkeyed-identity-paired");
    check(
        native::inject_press(a.into(), &held, ProbePhase::Ended, true),
        "unkeyed-release-injected",
    );
    check(admitted(a), "unkeyed-release-admitted");
    // The admitted request declines at this task's epilogue, fencing the old
    // text client; the keyed presses then reach a fenced text view.
    native::next_native_turn(move || task(&app, move || keyed_presses(a)));
}

/// S13, fenced text client: the text view's body returns early, so only its
/// own selector's record line can see the press and its release.
fn keyed_presses(a: WindowHandle<Host>) {
    check(
        text_input_session_state(a.into()).is_some_and(|state| state.retired),
        "text-client-fenced-by-decline",
    );
    let key = press(true);
    check(
        native::inject_press(a.into(), &key, ProbePhase::Began, false),
        "keyed-press-injected",
    );
    check(hardware_refused(a), "keyed-press-held-refused");
    check(
        native_hardware_hold(a.into()).is_some_and(|hold| hold.keyed == 1 && !hold.key_state_only),
        "keyed-press-in-ledger",
    );
    check(
        native::inject_press(a.into(), &key, ProbePhase::Ended, false),
        "keyed-release-injected",
    );
    check(admitted(a), "keyed-release-admitted");
    native::next_native_turn(move || {
        eprintln!(
            "B1_PROBE_PASS: authored subset only; OS/B2/hardware recovery and full A1-A16 acceptance UNVERIFIED"
        );
        std::process::exit(0);
    });
}
