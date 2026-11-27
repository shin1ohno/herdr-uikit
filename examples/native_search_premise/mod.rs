#[allow(
    dead_code,
    reason = "shared with the native search probe; this probe uses a subset"
)]
mod native;

use gpui::{
    AnyWindowHandle, App, AsyncWindowContext, Bounds, Context, ElementInputHandler,
    EntityInputHandler, FocusHandle, KeyDownEvent, KeyUpEvent, Pixels, Point, UTF16Selection,
    Window, WindowHandle, WindowOptions, canvas, div, prelude::*, rgb,
};
use herdr_uikit::retirement_probe::{self as sdk};
use objc2::MainThreadMarker;
use std::{cell::RefCell, ops::Range, rc::Rc, time::Duration};

/// What reached GPUI's input handler and key listeners.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Sink {
    text: u32,
    marked: u32,
    unmark: u32,
    key_down: u32,
    key_up: u32,
    taps: u32,
    last: String,
}

fn check(ok: bool, name: &str) {
    if ok {
        eprintln!("PREMISE_CHECK: {name}");
    } else {
        eprintln!("PREMISE_ASSERT: {name}");
        native::set_phase("failed");
        std::process::exit(2);
    }
}

fn record(text: String) {
    eprintln!("PREMISE_RECORD: {text}");
}

struct Host {
    focus: FocusHandle,
    sink: Rc<RefCell<Sink>>,
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
        div()
            .size_full()
            .bg(rgb(0x203040))
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
    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.sink.borrow_mut().unmark += 1;
    }
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
struct Probe {
    handle: WindowHandle<Host>,
    app: AsyncWindowContext,
    sink: Rc<RefCell<Sink>>,
    mtm: MainThreadMarker,
}

impl Probe {
    fn any(&self) -> AnyWindowHandle {
        self.handle.into()
    }
    fn sink(&self) -> Sink {
        self.sink.borrow().clone()
    }
    fn after(&self, delay: Duration, f: impl FnOnce(Probe) + 'static) {
        let probe = self.clone();
        self.app
            .spawn(async move |cx| {
                cx.background_executor().timer(delay).await;
                sdk::next_native_turn(move || f(probe));
            })
            .detach();
    }
    /// Polls `ready` every 100 ms; runs `then` with whether it held in time.
    fn wait(
        &self,
        label: &'static str,
        tries: u32,
        ready: impl Fn(&Probe) -> bool + 'static,
        then: impl FnOnce(Probe, bool) + 'static,
    ) {
        if ready(self) {
            then(self.clone(), true);
            return;
        }
        if tries == 0 {
            record(format!("{label}: timed out"));
            then(self.clone(), false);
            return;
        }
        self.after(Duration::from_millis(100), move |probe| {
            probe.wait(label, tries - 1, ready, then)
        });
    }
    fn window(&self) -> objc2::rc::Retained<objc2_ui_kit::UIWindow> {
        match native::gpui_window(self.mtm) {
            Some(window) => window,
            None => {
                check(false, "gpui-window-found");
                std::process::exit(2);
            }
        }
    }
}

pub fn run() {
    herdr_uikit::application().run(|cx: &mut App| {
        eprintln!(
            "PREMISE_SCOPE: UIKit routing on a Simulator; iPadOS hardware keyboard and IME UNVERIFIED"
        );
        let Some(mtm) = MainThreadMarker::new() else {
            check(false, "main-thread");
            return;
        };
        native::ensure_class();
        let sink = Rc::new(RefCell::new(Sink::default()));
        let captured = sink.clone();
        let handle = match cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| {
                let focus = cx.focus_handle();
                focus.focus(window, cx);
                Host {
                    focus,
                    sink: captured,
                }
            })
        }) {
            Ok(handle) => handle,
            Err(error) => {
                eprintln!("PREMISE_ASSERT: open-window {error:?}");
                std::process::exit(2);
            }
        };
        let app = match handle.update(cx, |_, window, cx| window.to_async(cx)) {
            Ok(app) => app,
            Err(error) => {
                eprintln!("PREMISE_ASSERT: window-context {error:?}");
                std::process::exit(2);
            }
        };
        let probe = Probe {
            handle,
            app,
            sink,
            mtm,
        };
        probe.after(Duration::from_millis(500), start);
    });
}

fn start(probe: Probe) {
    let window = probe.window();
    if let Some(root) = window
        .rootViewController()
        .and_then(|root| root.viewIfLoaded())
    {
        native::phase_label(&root, probe.mtm);
    }
    native::set_phase("starting");
    record(format!("window-width={}", native::width(&window)));
    record(format!(
        "input-modes={:?}",
        native::input_mode_languages(probe.mtm)
    ));
    probe.wait(
        "gpui-text-ready",
        100,
        |probe| {
            sdk::capture(probe.any())
                .is_some_and(|client| client.text_readiness() == (true, true, true))
        },
        |probe, ready| {
            check(ready, "gpui-text-view-first-responder-before");
            positive_control(probe);
        },
    );
}

/// Before presenting: the same probes reach GPUI, so a zero later is meaningful.
fn positive_control(probe: Probe) {
    let window = probe.window();
    let responder = native::first_responder(&window);
    let first = responder.as_ref().map(|view| native::class_name(view));
    record(format!("before: first-responder={first:?}"));
    check(
        first
            .as_deref()
            .is_some_and(|name| name.contains(native::GPUI_CLASSES[1])),
        "control-first-responder-is-gpui-text-view",
    );
    if let Some(responder) = &responder {
        native::insert_text(responder, "A");
    }
    let sink = probe.sink();
    record(format!("control: sink={sink:?}"));
    check(
        sink.text == 1 && sink.last == "A",
        "control-insert-text-reaches-gpui",
    );
    // Handshake: XCUITest taps the window once it is running.
    native::set_phase("await-tap");
    probe.wait(
        "xcuitest-tap",
        3000,
        |probe| probe.sink().taps > 0,
        |probe, tapped| {
            check(tapped, "xcuitest-attached");
            hardware_control(probe);
        },
    );
}

fn hardware_control(probe: Probe) {
    native::set_phase("await-keys-gpui");
    let before = probe.sink();
    probe.wait(
        "control-hardware-key",
        600,
        move |probe| probe.sink().key_down > before.key_down || probe.sink().text > before.text,
        |probe, typed| {
            record(format!(
                "control: XCUITest key reached GPUI={typed} sink={:?}; \
                 XCUITest delivers typed text through insertText:",
                probe.sink()
            ));
            check(typed, "control-xcuitest-key-reaches-gpui");
            present_case(probe, false);
        },
    );
}

fn present_case(probe: Probe, sheet: bool) {
    let window = probe.window();
    let Some(root) = window.rootViewController() else {
        check(false, "root-controller");
        return;
    };
    let Some(source) = root.viewIfLoaded() else {
        check(false, "root-view");
        return;
    };
    let label = if sheet { "sheet" } else { "popover" };
    record(format!("{label}: window-width={}", native::width(&window)));
    let became = Rc::new(std::cell::Cell::new(None));
    let flag = became.clone();
    let presented = native::present(
        &root,
        &source,
        sheet,
        move |ok| flag.set(Some(ok)),
        probe.mtm,
    );
    let presented = Rc::new(presented);
    probe.wait(
        "presentation-completion",
        50,
        move |_| became.get().is_some(),
        move |probe, done| {
            check(done, &format!("{label}-presented"));
            probe.after(Duration::from_millis(600), move |probe| {
                inspect(probe, presented, sheet);
            });
        },
    );
}

fn inspect(probe: Probe, presented: Rc<native::Presented>, sheet: bool) {
    let label = if sheet { "sheet" } else { "popover" };
    let window = probe.window();
    let style = presented.controller.modalPresentationStyle().0;
    let presenting = presented
        .controller
        .presentingViewController()
        .map(|c| native::class_name(&c));
    record(format!("{label}: style={style} presenting={presenting:?}"));
    check(
        presented.field.isFirstResponder(),
        &format!("{label}-field-first-responder"),
    );
    let (drawing, text) = native::gpui_first_responders(&window);
    check(
        !drawing && !text,
        &format!("{label}-gpui-views-not-first-responder"),
    );
    let first = native::first_responder(&window);
    let first_name = first.as_ref().map(|view| native::class_name(view));
    record(format!("{label}: first-responder={first_name:?}"));
    check(
        first.as_ref().is_some_and(|view| {
            objc2::rc::Retained::as_ptr(view).cast::<()>()
                == objc2::rc::Retained::as_ptr(&presented.field).cast::<()>()
        }),
        &format!("{label}-hierarchy-first-responder-is-field"),
    );
    let chain = native::chain(&presented.field);
    record(format!("{label}: chain={chain:?}"));
    check(
        !chain.iter().any(|name| native::is_gpui_class(name)),
        &format!("{label}-chain-excludes-gpui-views"),
    );
    let root = window.rootViewController();
    let root_in_chain = root
        .as_ref()
        .is_some_and(|root| native::chain_contains(&presented.field, root));
    let root_class = root.as_ref().map(|root| native::class_name(root));
    record(format!(
        "{label}: presenting root controller in chain={root_in_chain} class={root_class:?} \
         (Apple: a presented controller's next responder is its presenter)"
    ));
    check(
        root_class.as_deref() == Some("UIViewController"),
        &format!("{label}-root-controller-is-plain-uiviewcontroller"),
    );
    // Selector probes, through UIKit's own first-responder resolution.
    let before = probe.sink();
    let Some(responder) = first else {
        check(false, &format!("{label}-first-responder-present"));
        return;
    };
    native::insert_text(&responder, "x");
    native::set_marked(&responder, "にほんご");
    let marked = native::marked(&presented.field);
    native::unmark(&responder);
    native::pasteboard("PASTE1", probe.mtm);
    let pasted = native::send_to_first_responder(probe.mtm, native::paste_sel(), None);
    let dictation = native::responds(&presented.field, native::dictation_sel());
    let placeholder = native::responds(&presented.field, native::dictation_placeholder_sel());
    let dictation_end =
        native::send_to_first_responder(probe.mtm, native::dictation_end_sel(), None);
    let text = native::field_text(&presented.field);
    let after = probe.sink();
    record(format!(
        "{label}: selectors field-text={text:?} marked-during={marked} paste-sent={pasted} \
         field-responds insertDictationResult:={dictation} insertDictationResultPlaceholder={placeholder} \
         dictationRecordingDidEnd-sent-to-first-responder={dictation_end} sink-before={before:?} sink-after={after:?}"
    ));
    check(
        text.starts_with('x') && text.contains("にほんご"),
        &format!("{label}-insert-and-marked-reach-field"),
    );
    check(marked, &format!("{label}-field-had-marked-text"));
    check(pasted, &format!("{label}-paste-action-has-a-target"));
    check(
        after == before,
        &format!("{label}-text-selectors-never-reach-gpui"),
    );
    // UITextField applies paste: after reading the pasteboard; poll for it.
    let field = presented.field.clone();
    probe.wait(
        "paste-applied",
        30,
        move |_| native::field_text(&field).contains("PASTE1"),
        move |probe, applied| {
            let after = probe.sink();
            record(format!(
                "{label}: paste field-text={:?} sink-after={after:?}",
                native::field_text(&presented.field)
            ));
            check(applied, &format!("{label}-paste-reaches-field"));
            check(
                after == before,
                &format!("{label}-paste-never-reaches-gpui"),
            );
            type_keys(probe, presented, sheet);
        },
    );
}

fn type_keys(probe: Probe, presented: Rc<native::Presented>, sheet: bool) {
    let label = if sheet { "sheet" } else { "popover" };
    native::clear(&presented.field);
    let presses_before = native::field_presses();
    let before = probe.sink();
    native::set_phase(if sheet {
        "await-keys-sheet"
    } else {
        "await-keys-popover"
    });
    let field = presented.field.clone();
    probe.wait(
        "typed-keys",
        600,
        move |_| native::field_text(&field).ends_with('#'),
        move |probe, typed| {
            let text = native::field_text(&presented.field);
            let presses = native::field_presses() - presses_before;
            let after = probe.sink();
            record(format!(
                "{label}: typed field-text={text:?} field-presses={presses} sink-before={before:?} sink-after={after:?}"
            ));
            check(typed, &format!("{label}-typed-keys-arrived"));
            check(text.starts_with("pro") && text.contains("UCLIP"), &format!("{label}-hardware-keys-and-command-v-reach-field"));
            check(presses > 0, &format!("{label}-presses-reach-field"));
            check(after == before, &format!("{label}-hardware-keys-never-reach-gpui"));
            if sheet {
                japanese(probe, presented);
            } else {
                native::dismiss(&presented, || {});
                probe.after(Duration::from_millis(900), |probe| {
                    let window = probe.window();
                    native::request_narrow(&window);
                    probe.after(Duration::from_millis(1500), |probe| {
                        let width = native::width(&probe.window());
                        record(format!("narrow request: window-width={width} (sheet below 352)"));
                        present_case(probe, true);
                    });
                });
            }
        },
    );
}

/// Records, without asserting, whether a Japanese input mode composes.
fn japanese(probe: Probe, presented: Rc<native::Presented>) {
    native::clear(&presented.field);
    native::prefer_japanese(true);
    presented.field.resignFirstResponder();
    let again = presented.field.becomeFirstResponder();
    let before = probe.sink();
    record(format!(
        "ja: re-acquired={again} modes={:?}",
        native::input_mode_languages(probe.mtm)
    ));
    native::set_phase("await-ja");
    let field = presented.field.clone();
    let seen = Rc::new(std::cell::Cell::new(false));
    let saw = seen.clone();
    probe.wait(
        "ja-composition",
        200,
        move |_| {
            if native::marked(&field) {
                saw.set(true);
            }
            native::field_text(&field).ends_with('#')
        },
        move |probe, _| {
            let text = native::field_text(&presented.field);
            let after = probe.sink();
            record(format!(
                "ja: field-text={text:?} marked-seen={} sink-unchanged={}",
                seen.get(),
                after == before
            ));
            check(after == before, "ja-input-never-reaches-gpui");
            native::dismiss(&presented, || {});
            native::set_phase("done");
            eprintln!("PREMISE_PASS: popover and sheet field own first responder; GPUI views outside the chain");
        },
    );
}
