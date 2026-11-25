#[allow(
    dead_code,
    reason = "shared with the premise probe; this fixture uses a subset"
)]
#[path = "../native_search_premise/native.rs"]
mod native;

mod cases;

use gpui::{
    AnyWindowHandle, App, AsyncApp, Bounds, Context, ElementInputHandler, EntityInputHandler,
    FocusHandle, KeyDownEvent, KeyUpEvent, Pixels, Point, UTF16Selection, Window, WindowHandle,
    WindowOptions, canvas, div, prelude::*, px, rgb,
};
use herdr_uikit::{NativeInlineFieldEvent, NativeInlineFieldEvents};
use objc2::MainThreadMarker;
use std::{
    cell::{Cell, RefCell},
    ops::Range,
    rc::Rc,
    time::Duration,
};

/// Where the label is drawn, in the drawing view's points.
pub(super) const LABEL_ORIGIN: (f32, f32) = (200.0, 300.0);
pub(super) const LABEL_TEXT: &str = "session-search";

/// What reached GPUI.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Sink {
    pub text: u32,
    pub marked: u32,
    pub key_down: u32,
    pub key_up: u32,
    pub taps: u32,
    pub last: String,
}

pub(super) fn check(ok: bool, name: &str) {
    if ok {
        eprintln!("INLINE_CHECK: {name}");
    } else {
        eprintln!("INLINE_ASSERT: {name}");
        native::set_phase("failed");
        std::process::exit(2);
    }
}

pub(super) fn record(text: String) {
    eprintln!("INLINE_RECORD: {text}");
}

pub(super) struct Host {
    pub focus: FocusHandle,
    sink: Rc<RefCell<Sink>>,
    label: Rc<Cell<Bounds<Pixels>>>,
    paints: Rc<Cell<u32>>,
}

impl Host {
    fn down(&mut self, _: &KeyDownEvent, _: &mut Window, _: &mut Context<Self>) {
        self.sink.borrow_mut().key_down += 1;
    }
    fn up(&mut self, _: &KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.sink.borrow_mut().key_up += 1;
    }
    fn tap(&mut self, _: &gpui::MouseDownEvent, _: &mut Window, _: &mut Context<Self>) {
        self.sink.borrow_mut().taps += 1;
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let focus = self.focus.clone();
        let label = self.label.clone();
        let paints = self.paints.clone();
        div()
            .size_full()
            .bg(rgb(0x1c1c22))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::down))
            .on_key_up(cx.listener(Self::up))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::tap))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(px(LABEL_ORIGIN.0))
                    .top(px(LABEL_ORIGIN.1))
                    .text_size(px(14.0))
                    .text_color(rgb(0xd8dee9))
                    .child(LABEL_TEXT)
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                label.set(bounds);
                                paints.set(paints.get() + 1);
                            },
                            |_, (), _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
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
        text: &str,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let mut sink = self.sink.borrow_mut();
        sink.text += 1;
        sink.last = text.into();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let mut sink = self.sink.borrow_mut();
        sink.marked += 1;
        sink.last = text.into();
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

#[derive(Clone)]
pub(super) struct Probe {
    pub handle: WindowHandle<Host>,
    pub cx: AsyncApp,
    pub sink: Rc<RefCell<Sink>>,
    pub label: Rc<Cell<Bounds<Pixels>>>,
    /// The label's prepaints: GPUI drew a frame.
    pub paints: Rc<Cell<u32>>,
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
}

/// The events a session delivered so far, drained as they arrive.
pub(super) struct Log {
    pub events: NativeInlineFieldEvents,
    pub seen: Vec<NativeInlineFieldEvent>,
}

impl Log {
    pub fn new(events: NativeInlineFieldEvents) -> Self {
        Self {
            events,
            seen: Vec::new(),
        }
    }
    pub fn drain(&mut self) -> &[NativeInlineFieldEvent] {
        while let Some(event) = self.events.try_next() {
            record(format!("event {event:?}"));
            self.seen.push(event);
        }
        &self.seen
    }
    pub fn has(&mut self, event: &NativeInlineFieldEvent) -> bool {
        self.drain().contains(event)
    }
    pub fn count(&mut self, matches: impl Fn(&NativeInlineFieldEvent) -> bool) -> usize {
        self.drain().iter().filter(|event| matches(event)).count()
    }
}

pub fn run() {
    herdr_uikit::application().run(|cx: &mut App| {
        eprintln!(
            "INLINE_SCOPE: native inline field and gate on a Simulator; iPadOS hardware and IME UNVERIFIED"
        );
        let Some(mtm) = MainThreadMarker::new() else {
            check(false, "main-thread");
            return;
        };
        let sink = Rc::new(RefCell::new(Sink::default()));
        let label = Rc::new(Cell::new(Bounds::default()));
        let paints = Rc::new(Cell::new(0));
        let (captured, drawn, painted) = (sink.clone(), label.clone(), paints.clone());
        let handle = match cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                Host {
                    focus,
                    sink: captured,
                    label: drawn,
                    paints: painted,
                }
            })
        }) {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("INLINE_ASSERT: open-window {error:?}");
                std::process::exit(2);
            }
        };
        let probe = Probe {
            handle,
            cx: cx.to_async(),
            sink,
            label,
            paints,
            mtm,
        };
        cx.spawn(async move |_| cases::run(probe).await).detach();
    });
}
