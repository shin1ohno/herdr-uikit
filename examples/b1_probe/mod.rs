mod cases;
mod host;
mod mixed;
mod raw;

use gpui::{App, WindowOptions, prelude::*};
use herdr_uikit::{
    NativeRetirementRequestError as Error, PrepareDecision, request_native_edit_retirement,
};
use host::{Host, Record, check, frame, origin, require, task};
use std::{cell::RefCell, rc::Rc};

fn open(
    cx: &mut App,
) -> (
    gpui::WindowHandle<Host>,
    gpui::AsyncWindowContext,
    Rc<RefCell<Record>>,
) {
    let record = Rc::new(RefCell::new(Record::default()));
    let captured = record.clone();
    let handle = require(
        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                Host {
                    focus,
                    marked: String::new(),
                    record: captured,
                }
            })
        }),
        "fixture-open-window",
    );
    let app = require(
        handle.update(cx, |_, window, cx| window.to_async(cx)),
        "fixture-context",
    );
    (handle, app, record)
}

pub fn run() {
    herdr_uikit::application().run(|cx: &mut App| {
        eprintln!("B1_PROBE_SCOPE: SDK adapter injection only; original S/Z/B2 and physical/OS delivery UNVERIFIED");
        let (handle, app, record) = open(cx);
        check(matches!(request_native_edit_retirement(handle.into(), origin(handle), |_| PrepareDecision::Decline),
            Err(Error::UnsupportedNativeEntry)), "native-launch-is-not-dispatcher-task");
        let next = app.clone();
        require(
            handle.update(cx, move |_, window, _| {
                window.on_next_frame(move |_, _| {
                    herdr_uikit::retirement_probe::next_native_turn(move || {
                        ready(handle, next, record, 0);
                    });
                });
            }),
            "fixture-register-launch-frame",
        );
    });
}

fn ready(
    handle: gpui::WindowHandle<Host>,
    app: gpui::AsyncWindowContext,
    record: Rc<RefCell<Record>>,
    count: usize,
) {
    let state = host::capture(handle).text_readiness();
    eprintln!("B1_PROBE_SETUP: frame={count} readiness={state:?}");
    if state != (true, true, true) {
        check(count < 30, "fixture-active-text-responder");
        let next = app.clone();
        frame(app, move || ready(handle, next, record, count + 1));
        return;
    }
    let next = app.clone();
    task(&app, move || cases::edits(handle, next, record, 0));
}
