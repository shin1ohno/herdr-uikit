mod cases;

use gpui::{
    AnyWindowHandle, App, AsyncWindowContext, Bounds, Context, ElementInputHandler,
    EntityInputHandler, FocusHandle, KeyDownEvent, KeyUpEvent, Pixels, Point, UTF16Selection,
    Window, WindowHandle, WindowOptions, canvas, div, prelude::*, rgb,
};
use herdr_uikit::{
    NativeHardwareHold, native_hardware_hold,
    retirement_probe::{
        self as native, NativeClient,
        held_press::{
            HeldProbeError, HeldProbeHandle, HeldProbeKeySpec, HeldProbePhase, HeldProbeTarget,
            held_press, inject_held_press,
        },
    },
};
use std::{cell::RefCell, ops::Range, rc::Rc, time::Duration};

pub(super) fn check(ok: bool, name: &str) {
    if !ok {
        eprintln!("HELD_PRESS_PROBE_ASSERT: {name}");
        std::process::exit(2);
    }
    eprintln!("HELD_PRESS_PROBE_CHECK: {name}");
}

/// Recorded for the report, never asserted.
pub(super) fn note(text: String) {
    eprintln!("HELD_PRESS_PROBE_RECORD: {text}");
}

pub(super) fn some<T>(value: Option<T>, name: &str) -> T {
    match value {
        Some(value) => value,
        None => {
            eprintln!("HELD_PRESS_PROBE_ASSERT: {name}");
            std::process::exit(2);
        }
    }
}

fn require<T, E: std::fmt::Debug>(result: Result<T, E>, name: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            eprintln!("HELD_PRESS_PROBE_ASSERT: {name}: {error:?}");
            std::process::exit(2);
        }
    }
}

/// Escape key events the GPUI view received.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Keys {
    pub down: u32,
    pub held: u32,
    pub up: u32,
}

pub(super) struct Host {
    focus: FocusHandle,
    keys: Rc<RefCell<Keys>>,
}

impl Host {
    /// Handling Escape makes the SDK keep the press (`Route::Gpui`) and
    /// repeat it on its own timer.
    fn down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key != "escape" {
            return;
        }
        let mut keys = self.keys.borrow_mut();
        if event.is_held {
            keys.held += 1;
        } else {
            keys.down += 1;
        }
        cx.stop_propagation();
    }

    fn up(&mut self, event: &KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.keys.borrow_mut().up += 1;
        }
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let focus = self.focus.clone();
        div()
            .size_full()
            .bg(rgb(0x203040))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::down))
            .on_key_up(cx.listener(Self::up))
            .child("SDK held-press injected fixture; iPadOS delivery unverified")
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
                    },
                )
                .size_full(),
            )
    }
}

impl EntityInputHandler for Host {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        *adjusted = Some(0..0);
        Some(String::new())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        _: &str,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        _: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

/// The fixture's window, its async context and the received key counts.
#[derive(Clone)]
pub(super) struct Probe {
    pub handle: WindowHandle<Host>,
    pub app: AsyncWindowContext,
    keys: Rc<RefCell<Keys>>,
}

impl Probe {
    pub fn any(&self) -> AnyWindowHandle {
        self.handle.into()
    }
    pub fn keys(&self) -> Keys {
        *self.keys.borrow()
    }
    pub fn hold(&self) -> NativeHardwareHold {
        some(native_hardware_hold(self.any()), "hardware-hold-readable")
    }
    pub fn client(&self) -> NativeClient {
        some(native::capture(self.any()), "fixture-live-window")
    }
    pub fn press(&self) -> HeldProbeHandle {
        some(held_press(HeldProbeKeySpec::ESCAPE), "held-press-object")
    }
    pub fn send(
        &self,
        press: &HeldProbeHandle,
        phase: HeldProbePhase,
        target: HeldProbeTarget<'_>,
    ) -> Result<(), HeldProbeError> {
        inject_held_press(self.any(), press, phase, target)
    }
    /// Run `f` after `delay` of real time, on its own native turn outside any
    /// GPUI update; the main run loop (and the SDK repeat timer) runs meanwhile.
    pub fn after(&self, delay: Duration, f: impl FnOnce(Probe) + 'static) {
        let probe = self.clone();
        self.app
            .spawn(async move |cx| {
                cx.background_executor().timer(delay).await;
                native::next_native_turn(move || f(probe));
            })
            .detach();
    }
    /// Wait until `client` (or the window's current one) is first responder,
    /// its window is active and GPUI has text focus, then run `f`.
    pub fn when_ready(&self, label: &'static str, tries: u32, f: impl FnOnce(Probe) + 'static) {
        let state = self.client().text_readiness();
        eprintln!("HELD_PRESS_PROBE_SETUP: {label} try={tries} readiness={state:?}");
        if state == (true, true, true) {
            f(self.clone());
            return;
        }
        check(tries < 40, label);
        self.after(Duration::from_millis(100), move |probe| {
            probe.when_ready(label, tries + 1, f)
        });
    }
}

pub fn run() {
    herdr_uikit::application().run(|cx: &mut App| {
        eprintln!(
            "HELD_PRESS_PROBE_SCOPE: injected presses through actual selectors; iPadOS delivery order UNVERIFIED"
        );
        let keys = Rc::new(RefCell::new(Keys::default()));
        let captured = keys.clone();
        let handle = require(
            cx.open_window(WindowOptions::default(), |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    Host {
                        focus,
                        keys: captured,
                    }
                })
            }),
            "fixture-open-window",
        );
        let app = require(
            handle.update(cx, |_, window, cx| window.to_async(cx)),
            "fixture-context",
        );
        let probe = Probe { handle, app, keys };
        require(
            handle.update(cx, move |_, window, _| {
                window.on_next_frame(move |_, _| {
                    native::next_native_turn(move || {
                        probe.when_ready("fixture-active-text-responder", 0, cases::start);
                    });
                });
            }),
            "fixture-register-launch-frame",
        );
    });
}
