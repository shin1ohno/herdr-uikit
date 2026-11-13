//! Actual queued adapter bodies. Nested DispatcherTask inheritance is tested
//! separately by extracted raw-body fixtures; this is not an OS pumping claim.
use std::{cell::Cell, rc::Rc};

use gpui::{AsyncWindowContext, WindowHandle};
use herdr_uikit::{
    NativeRetirementRequestError as Error, PrepareDecision, request_native_edit_retirement,
    retirement_probe as native, text_input_session_state,
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
            task(&app, move || {
                check(
                    native::empty_hardware_entry(a.into()),
                    "empty-press-selector-reached",
                );
                check(
                    matches!(
                        request_native_edit_retirement(a.into(), origin(a), |_| {
                            PrepareDecision::Decline
                        }),
                        Err(Error::UnsupportedHardwareContext)
                    ),
                    "hardware-entry-never-admitted",
                );
                eprintln!(
                    "B1_PROBE_PASS: authored subset only; OS/B2/hardware recovery and full A1-A16 acceptance UNVERIFIED"
                );
                std::process::exit(0);
            });
        }
    });
}
