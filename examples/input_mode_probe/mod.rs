#[allow(
    dead_code,
    reason = "shared with the premise probe; this fixture uses a subset"
)]
#[path = "../native_search_premise/native.rs"]
mod native;

mod cases;
mod uikit;

use gpui::{
    AnyWindowHandle, App, AsyncApp, Bounds, Context, ElementInputHandler, EntityInputHandler,
    FocusHandle, ModifiersChangedEvent, Pixels, Point, UTF16Selection, Window, WindowHandle,
    WindowOptions, canvas, div, point, prelude::*, px, rgb, size,
};
use objc2::MainThreadMarker;
use std::{cell::RefCell, ops::Range, rc::Rc, time::Duration};

/// The caret the handler reports before a case moves it (the probe's).
pub(super) const CARET: (f32, f32, f32, f32) = (300.0, 200.0, 2.0, 24.0);

/// What reached GPUI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Sink {
    pub taps: u32,
    pub control_downs: u32,
    pub marked: String,
    pub commits: Vec<String>,
}

pub(super) fn check(ok: bool, name: &str) {
    if ok {
        eprintln!("INPUTMODE_CHECK: {name}");
    } else {
        eprintln!("INPUTMODE_ASSERT: {name}");
        native::set_phase("failed");
        std::process::exit(2);
    }
}

pub(super) fn record(text: String) {
    eprintln!("INPUTMODE_RECORD: {text}");
}

/// A terminal-like text element: an empty document, the selection at 0..0,
/// and a caret the cases move.
pub(super) struct Host {
    pub focus: FocusHandle,
    pub caret: Bounds<Pixels>,
    pub accepts: bool,
    sink: Rc<RefCell<Sink>>,
    control: bool,
}

impl Host {
    fn modifiers(&mut self, event: &ModifiersChangedEvent, _: &mut Window, _: &mut Context<Self>) {
        let mut sink = self.sink.borrow_mut();
        if event.modifiers.control && !self.control {
            sink.control_downs += 1;
        }
        self.control = event.modifiers.control;
    }
    fn tap(&mut self, _: &gpui::MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.sink.borrow_mut().taps += 1;
        // A tap on a focused terminal asks for the keyboard again.
        self.focus.focus(window, cx);
        window.request_virtual_keyboard();
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
            .on_modifiers_changed(cx.listener(Self::modifiers))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::tap))
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
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let marked: Vec<u16> = self.sink.borrow().marked.encode_utf16().collect();
        let range = range.start.min(marked.len())..range.end.min(marked.len());
        *adjusted = Some(range.clone());
        String::from_utf16(marked.get(range)?).ok()
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.sink.borrow().marked.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let sink = self.sink.borrow();
        (!sink.marked.is_empty()).then(|| 0..sink.marked.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.sink.borrow_mut().marked.clear();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let mut sink = self.sink.borrow_mut();
        sink.marked.clear();
        if !text.is_empty() {
            sink.commits.push(text.into());
        }
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.sink.borrow_mut().marked = text.into();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(self.caret)
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.accepts
    }
}

#[derive(Clone)]
pub(super) struct Probe {
    pub handle: WindowHandle<Host>,
    pub cx: AsyncApp,
    pub sink: Rc<RefCell<Sink>>,
    pub mtm: MainThreadMarker,
}

impl Probe {
    pub fn any(&self) -> AnyWindowHandle {
        self.handle.into()
    }
    pub fn sink(&self) -> Sink {
        self.sink.borrow().clone()
    }
    pub async fn sleep(&self, ms: u64) {
        self.cx
            .background_executor()
            .timer(Duration::from_millis(ms))
            .await;
    }
    /// Polls `ready` every 50 ms for up to `ms`.
    pub async fn wait(&self, ms: u64, mut ready: impl FnMut(&Probe) -> bool) -> bool {
        for _ in 0..(ms / 50).max(1) {
            if ready(self) {
                return true;
            }
            self.sleep(50).await;
        }
        ready(self)
    }
    /// Runs `f` inside a GPUI update of the fixture's window.
    pub fn update(&self, f: impl FnOnce(&mut Host, &mut Window, &mut Context<Host>)) {
        let mut cx = self.cx.clone();
        if self.handle.update(&mut cx, f).is_err() {
            check(false, "fixture-window-update");
        }
    }
    /// Moves the caret the handler reports and asks GPUI to report it to the
    /// platform (`update_ime_position`), as a terminal does after output.
    pub fn move_caret(&self, x: f32, y: f32) {
        self.update(|host, window, cx| {
            host.caret = Bounds::new(point(px(x), px(y)), size(px(CARET.2), px(CARET.3)));
            window.invalidate_character_coordinates();
            cx.notify();
        });
    }
    /// The SDK's QA state of the fixture's window.
    pub fn state(&self) -> herdr_uikit::InputModeIndicatorState {
        match herdr_uikit::input_mode_indicator_state(self.any()) {
            Some(state) => state,
            None => {
                check(false, "qa-state-readable");
                std::process::exit(2);
            }
        }
    }
}

pub fn run() {
    herdr_uikit::application().run(|cx: &mut App| {
        eprintln!(
            "INPUTMODE_SCOPE: system input mode indicator on a Simulator; iPadOS hardware, hardware keyboard and IME UNVERIFIED"
        );
        let Some(mtm) = MainThreadMarker::new() else {
            check(false, "main-thread");
            return;
        };
        let sink = Rc::new(RefCell::new(Sink::default()));
        let captured = sink.clone();
        let handle = match cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                Host {
                    focus,
                    caret: Bounds::new(
                        point(px(CARET.0), px(CARET.1)),
                        size(px(CARET.2), px(CARET.3)),
                    ),
                    accepts: true,
                    sink: captured,
                    control: false,
                }
            })
        }) {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("INPUTMODE_ASSERT: open-window {error:?}");
                std::process::exit(2);
            }
        };
        let probe = Probe {
            handle,
            cx: cx.to_async(),
            sink,
            mtm,
        };
        cx.spawn(async move |_| cases::run(probe).await).detach();
    });
}
