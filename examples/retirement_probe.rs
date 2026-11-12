//! UIApplicationMain regression for the public retirement API. Build with
//! --features test-support. Event/selector injection is explicitly not OS IME
//! routing evidence. Failures are reported without unwinding through UIKit.
#[cfg(all(target_os = "ios", feature = "test-support"))]
mod probe {
    use gpui::{
        App, AsyncWindowContext, Bounds, Context, ElementInputHandler, EntityInputHandler,
        FocusHandle, KeyDownEvent, Pixels, Point, UTF16Selection, Window, WindowHandle,
        WindowOptions, canvas, div, prelude::*, rgb,
    };
    use herdr_uikit::{
        NativeEditAvailability, TextInputRetirement, TextInputRetirementError,
        TextInputRetirementStatus, retire_text_input_session,
        retirement_probe::{self as native, NativeClient},
        set_native_edit_availability, text_input_session_state,
    };
    use std::{cell::RefCell, ops::Range, rc::Rc};

    fn check(ok: bool, name: &str) {
        if !ok {
            eprintln!("RETIREMENT_PROBE_ASSERT: {name}");
            std::process::exit(2);
        }
        eprintln!("RETIREMENT_PROBE_CHECK: {name}");
    }
    fn require<T, E: std::fmt::Debug>(value: Result<T, E>, name: &str) -> T {
        match value {
            Ok(value) => value,
            Err(error) => {
                eprintln!("RETIREMENT_PROBE_ASSERT: {name}: {error:?}");
                std::process::exit(2);
            }
        }
    }
    fn capture(handle: WindowHandle<Probe>) -> NativeClient {
        match native::capture(handle.into()) {
            Some(client) => client,
            None => {
                check(false, "live-native-client");
                unreachable!()
            }
        }
    }
    #[derive(Default)]
    struct Record {
        launch: usize,
        slot: usize,
        sentinel: usize,
        busy: usize,
        edits: Vec<String>,
        commits: Vec<String>,
        receipt: Option<TextInputRetirement>,
    }
    struct Probe {
        focus: FocusHandle,
        marked: String,
        text: bool,
        record: Rc<RefCell<Record>>,
    }
    impl Probe {
        fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
            let handle = window.window_handle();
            match event.keystroke.key.as_str() {
                "f6" => {
                    let weak = cx.entity().downgrade();
                    let mut app = window.to_async(cx);
                    let record = self.record.clone();
                    let old = native::capture(handle);
                    let receipt = require(
                        retire_text_input_session(handle, move |_| {
                            require(
                                app.update(|_, cx| {
                                    weak.update(cx, |view, _| {
                                        view.marked.clear();
                                        view.record.borrow_mut().slot += 1;
                                    })
                                })
                                .and_then(|result| result),
                                "slot-weak-update-after-app-borrow",
                            );
                            native::key(handle, "f7", false);
                            check(
                                record.borrow().sentinel == 1,
                                "public-api-callback-slot-restored",
                            );
                        }),
                        "slot-public-retire",
                    );
                    check(
                        receipt.status() == TextInputRetirementStatus::Pending,
                        "slot-pending-inside-handler",
                    );
                    if let Some(old) = old {
                        old.insert("same");
                        old.unmark();
                    }
                    check(
                        self.record.borrow().commits.is_empty(),
                        "old-before-replacement-blocked",
                    );
                    self.record.borrow_mut().receipt = Some(receipt);
                }
                "f7" => self.record.borrow_mut().sentinel += 1,
                "f8" => {
                    let before = text_input_session_state(handle);
                    let result = retire_text_input_session(handle, |_| {
                        check(false, "busy-callback-must-not-run")
                    });
                    check(
                        matches!(
                            result,
                            Err(TextInputRetirementError::NativeCallbackInFlight)
                        ),
                        "native-callback-refused-explicitly",
                    );
                    check(
                        text_input_session_state(handle) == before,
                        "busy-refusal-has-no-side-effect",
                    );
                    self.record.borrow_mut().busy += 1;
                }
                key if event.keystroke.modifiers.platform => {
                    self.record.borrow_mut().edits.push(key.into())
                }
                _ => {}
            }
            cx.stop_propagation();
        }
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity();
            let focus = self.focus.clone();
            div()
                .size_full()
                .bg(rgb(0x1f3a5f))
                .text_color(rgb(0xffffff))
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key))
                .child("SDK public retirement probe — injected events, OS routing unverified")
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, (), window, cx| {
                            window.handle_input(
                                &focus,
                                ElementInputHandler::new(bounds, entity),
                                cx,
                            );
                        },
                    )
                    .size_full(),
                )
        }
    }
    impl EntityInputHandler for Probe {
        fn text_for_range(
            &mut self,
            range: Range<usize>,
            adjusted: &mut Option<Range<usize>>,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<String> {
            let units: Vec<_> = self.marked.encode_utf16().collect();
            let range = range.start.min(units.len())..range.end.min(units.len());
            *adjusted = Some(range.clone());
            String::from_utf16(units.get(range)?).ok()
        }
        fn selected_text_range(
            &mut self,
            _: bool,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<UTF16Selection> {
            let end = self.marked.encode_utf16().count();
            Some(UTF16Selection {
                range: end..end,
                reversed: false,
            })
        }
        fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
            (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
        }
        fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
            self.marked.clear();
        }
        fn replace_text_in_range(
            &mut self,
            _: Option<Range<usize>>,
            text: &str,
            _: &mut Window,
            _: &mut Context<Self>,
        ) {
            self.marked.clear();
            if !text.is_empty() {
                self.record.borrow_mut().commits.push(text.into());
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
            self.marked = text.into();
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
        fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
            self.text
        }
    }
    fn frame(mut app: AsyncWindowContext, f: impl FnOnce() + 'static) {
        app.on_next_frame(move |_, _| native::next_native_turn(f));
    }
    fn availability(client: &NativeClient, expected: [bool; 4], drawing: bool, name: &str) {
        check(
            client.availability(drawing) == Some((expected, expected)),
            name,
        );
    }
    fn ready(
        handle: WindowHandle<Probe>,
        mut app: AsyncWindowContext,
        record: Rc<RefCell<Record>>,
        tries: usize,
    ) {
        if !capture(handle).first_responder() {
            check(tries < 30, "native-responder-ready-within-30-frames");
            let next = app.clone();
            frame(app, move || ready(handle, next, record, tries + 1));
            return;
        }
        check(record.borrow().launch == 1, "launch-callback-once");
        let old = capture(handle);
        availability(&old, [true; 4], false, "defaults-text-selectors");
        availability(&old, [true; 4], true, "defaults-drawing-selectors");
        check(
            set_native_edit_availability(
                handle.into(),
                Some(NativeEditAvailability {
                    copy: true,
                    cut: false,
                    paste: true,
                    select_all: false,
                }),
            ),
            "publish-availability",
        );
        availability(
            &old,
            [true, false, true, false],
            false,
            "override-text-selectors",
        );
        availability(
            &old,
            [true, false, true, false],
            true,
            "override-drawing-selectors",
        );
        old.edit_actions();
        check(
            record.borrow().edits == ["c", "v"],
            "only-available-native-actions-delivered",
        );
        old.mark("same");
        native::key(handle.into(), "f8", true);
        check(record.borrow().busy == 1, "native-busy-path-reached");
        native::key(handle.into(), "f6", false);
        check(record.borrow().slot == 1, "public-slot-callback-once");
        check(
            matches!(
                record
                    .borrow()
                    .receipt
                    .as_ref()
                    .map(TextInputRetirement::status),
                Some(TextInputRetirementStatus::Replaced { .. })
            ),
            "public-slot-receipt-replaced",
        );
        let new = capture(handle);
        check(
            new.identity() != old.identity(),
            "immutable-new-native-origin",
        );
        check(!old.attached(), "old-native-detached");
        availability(&old, [false; 4], false, "retired-old-selectors-disabled");
        old.edit_actions();
        old.insert("same");
        old.mark("same");
        old.unmark();
        old.delete();
        check(
            record.borrow().commits.is_empty(),
            "old-after-replacement-blocked",
        );
        check(
            record.borrow().edits == ["c", "v"],
            "retired-old-actions-blocked",
        );
        availability(
            &new,
            [true, false, true, false],
            false,
            "replacement-preserves-window-availability",
        );
        new.insert("a");
        new.insert("same");
        new.mark("日本😀");
        new.unmark();
        new.unmark();
        check(
            record.borrow().commits == ["a", "same", "日本😀"],
            "fresh-text-composition-once",
        );
        require(
            handle.update(&mut app, |view, _, cx| {
                view.text = false;
                cx.notify();
            }),
            "enter-key-only-mode",
        );
        let next = app.clone();
        frame(app, move || key_only(handle, next, record, 0));
    }
    fn key_only(
        handle: WindowHandle<Probe>,
        mut app: AsyncWindowContext,
        record: Rc<RefCell<Record>>,
        tries: usize,
    ) {
        let client = capture(handle);
        // Focus changes enqueue the platform's responder update. A frame
        // callback is not an acknowledgement that this native work ran.
        if client.first_responder() {
            check(tries < 30, "key-only-responder-resigned-within-30-frames");
            let next = app.clone();
            frame(app, move || key_only(handle, next, record, tries + 1));
            return;
        }
        check(true, "key-only-text-responder-resigned");
        check(
            set_native_edit_availability(handle.into(), None),
            "restore-default-availability",
        );
        availability(&client, [false; 4], false, "key-only-defaults-disabled");
        availability(
            &client,
            [false; 4],
            true,
            "key-only-drawing-defaults-disabled",
        );
        require(
            handle.update(&mut app, |view, window, cx| {
                view.text = true;
                view.focus.focus(window, cx);
                cx.notify();
            }),
            "restore-text-focus",
        );
        let next = app.clone();
        frame(app, move || close(handle, next, record, 0));
    }
    fn close(
        handle: WindowHandle<Probe>,
        mut app: AsyncWindowContext,
        record: Rc<RefCell<Record>>,
        tries: usize,
    ) {
        let client = capture(handle);
        if !client.first_responder() {
            check(tries < 30, "restored-focus-within-30-frames");
            let next = app.clone();
            frame(app, move || close(handle, next, record, tries + 1));
            return;
        }
        availability(&client, [true; 4], false, "restored-focus-defaults");
        let receipt = require(
            retire_text_input_session(handle.into(), move |_| {
                require(
                    handle.update(&mut app, |_, window, _| window.remove_window()),
                    "close-from-public-callback",
                );
            }),
            "close-public-retire",
        );
        check(
            receipt.status() == TextInputRetirementStatus::WindowClosed,
            "closed-owner-no-replacement",
        );
        availability(
            &client,
            [false; 4],
            false,
            "weak-closed-owner-selectors-disabled",
        );
        client.edit_actions();
        client.insert("late-close");
        check(
            record.borrow().commits == ["a", "same", "日本😀"],
            "closed-owner-no-delivery",
        );
        check(
            !set_native_edit_availability(handle.into(), Some(NativeEditAvailability::default())),
            "closed-owner-availability-rejected",
        );
        eprintln!(
            "RETIREMENT_PROBE_PASS: public-launch slot native-selectors focus close; OS retarget/loss UNVERIFIED"
        );
        std::process::exit(0);
    }
    pub fn run() {
        herdr_uikit::application().run(|cx: &mut App| {
            let record = Rc::new(RefCell::new(Record::default()));
            let captured = record.clone();
            let handle = require(
                cx.open_window(WindowOptions::default(), |window, cx| {
                    cx.new(|cx| {
                        let focus = cx.focus_handle();
                        focus.focus(window, cx);
                        Probe {
                            focus,
                            marked: String::new(),
                            text: true,
                            record: captured,
                        }
                    })
                }),
                "open-real-uiwindow",
            );
            let (mut app, weak) = require(
                handle.update(cx, |_, window, cx| {
                    (window.to_async(cx), cx.entity().downgrade())
                }),
                "capture-weak-context",
            );
            let next = app.clone();
            let retired = record.clone();
            let receipt = require(
                retire_text_input_session(handle.into(), move |_| {
                    require(
                        app.update(|_, cx| {
                            weak.update(cx, |view, _| {
                                view.marked.clear();
                                view.record.borrow_mut().launch += 1;
                            })
                        })
                        .and_then(|result| result),
                        "launch-weak-update-after-app-borrow",
                    );
                    frame(next.clone(), move || ready(handle, next, retired, 0));
                }),
                "launch-public-retire",
            );
            check(
                receipt.status() == TextInputRetirementStatus::Pending,
                "launch-pending-inside-app-borrow",
            );
            check(record.borrow().launch == 0, "launch-not-drained-early");
        });
    }
}
fn main() {
    #[cfg(all(target_os = "ios", feature = "test-support"))]
    probe::run();
}
