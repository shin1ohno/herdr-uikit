//! UIApplicationMain fixture for `NativeClient::cut_and_observe_selection`:
//! one actual Cut joined to the same client's first selection query. Build with
//! --features test-support. Injection is not OS IME routing evidence and does
//! not certify any consumer gate. Launch arguments: none (L1), `--main-only`
//! (restores and the N2 mutant), `--explicit-policy` (L6).
#[cfg(all(target_os = "ios", feature = "test-support"))]
#[allow(
    clippy::result_large_err,
    reason = "the SDK result carries its frozen observation"
)]
mod probe {
    use gpui::{
        App, AsyncWindowContext, Bounds, ClipboardItem, Context, ElementInputHandler,
        EntityInputHandler, FocusHandle, KeyDownEvent, Pixels, Point, TextInputConfiguration,
        UTF16Selection, Window, WindowHandle, WindowOptions, canvas, div, prelude::*, rgb,
    };
    use herdr_uikit::retirement_probe::{
        self as native, NativeClient, NativeCutCounter as C, NativeCutObservationTrace,
        NativeCutPhase as P, NativeCutProbeError, NativeCutProbeResult, NativeCutRefusal as R,
        NativeQuery,
    };
    use herdr_uikit::{NativeEditAvailability, retire_text_input_session};
    use std::{cell::RefCell, ops::Range};

    // M1 (`p13-cut-first-before-next-turn`) flips only this flag.
    const NEXT_TURN: bool = false;

    fn check(ok: bool, name: &str) {
        if !ok {
            eprintln!("CUT_QUERY_PROBE_ASSERT: {name}");
            std::process::exit(2);
        }
        eprintln!("CUT_QUERY_PROBE_CHECK: {name}");
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Callback {
        RenderTree,
        CanvasPrepaint,
        CanvasPaint,
        KeyDown,
        CutDelete,
        Selection,
        Marked,
        Length,
        Text,
        Accepts,
        Configuration,
        Bounds,
        CharacterIndex,
        Replace,
        ReplaceMark,
        Select,
        Unmark,
        Setup,
    }
    #[derive(Clone, Copy, Debug, Default)]
    struct CallbackCounts {
        enters: [u16; 18],
        exits: [u16; 18],
        down_returned: [u16; 18],
        violation: bool,
    }
    thread_local! {
        static CLIENT: RefCell<Option<NativeClient>> = const { RefCell::new(None) };
        static COUNTS: RefCell<CallbackCounts> = RefCell::new(CallbackCounts::default());
        static LOAN: RefCell<Option<NativeCutProbeResult>> = const { RefCell::new(None) };
    }
    // The single legal-phase table (matrix-4 §3).
    fn legal(callback: Callback, phase: P) -> bool {
        use Callback::*;
        match callback {
            RenderTree | CanvasPrepaint | CanvasPaint | Accepts | Configuration => {
                matches!(phase, P::AcceptedCut | P::DownReturned | P::OriginRestored)
            }
            KeyDown | CutDelete => phase == P::AcceptedCut,
            Selection | Marked | Length | Text => {
                matches!(phase, P::SnapshotEntered | P::FirstGetterEntered)
            }
            Bounds | CharacterIndex | Replace | ReplaceMark | Select | Unmark | Setup => false,
        }
    }
    struct Scope(Option<Callback>);
    impl Scope {
        fn new(callback: Callback) -> Self {
            let phase = CLIENT.with_borrow(|client| client.as_ref()?.cut_phase());
            let Some(phase) = phase else {
                return Self(None);
            };
            COUNTS.with_borrow_mut(|counts| {
                let i = callback as usize;
                counts.enters[i] += 1;
                counts.down_returned[i] += u16::from(phase == P::DownReturned);
                counts.violation |= !legal(callback, phase);
            });
            Self(Some(callback))
        }
    }
    impl Drop for Scope {
        fn drop(&mut self) {
            if let Some(callback) = self.0 {
                COUNTS.with_borrow_mut(|counts| counts.exits[callback as usize] += 1);
            }
        }
    }
    fn balanced(counts: &CallbackCounts) -> bool {
        counts.enters == counts.exits
    }
    fn self_check() {
        use Callback::*;
        check(
            legal(RenderTree, P::DownReturned),
            "table-render-down-legal",
        );
        check(
            !legal(RenderTree, P::SelectorReturned),
            "table-render-late-violation",
        );
        check(legal(Accepts, P::DownReturned), "table-accepts-down-legal");
        check(
            !legal(Accepts, P::SnapshotEntered),
            "table-accepts-snapshot-violation",
        );
        let every = [
            P::Armed,
            P::AcceptedCut,
            P::SnapshotEntered,
            P::FirstResultStored,
        ];
        check(
            every.iter().all(|&phase| !legal(Replace, phase)),
            "table-replace-violation",
        );
        let mut exit_only = CallbackCounts::default();
        exit_only.exits[0] = 1;
        check(!balanced(&exit_only), "table-unbalanced-violation");
    }

    struct Probe {
        focus: FocusHandle,
        text: Vec<u16>,
        selection: Range<usize>,
        marked: Option<Range<usize>>,
        nil_selection: bool,
    }
    impl Probe {
        fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
            let _scope = Scope::new(Callback::KeyDown);
            match event.keystroke.key.as_str() {
                "x" if event.keystroke.modifiers.platform => self.cut_selected(cx),
                "f9" => {
                    let client = CLIENT.with_borrow(Clone::clone);
                    let result = client.map(|client| client.cut_and_observe_selection());
                    LOAN.set(result);
                }
                _ => return,
            }
            cx.stop_propagation();
        }
        fn cut_selected(&mut self, cx: &mut Context<Self>) {
            let _scope = Scope::new(Callback::CutDelete);
            let range = self.selection.clone();
            let cut: Vec<u16> = self.text.splice(range.clone(), []).collect();
            cx.write_to_clipboard(ClipboardItem::new_string(String::from_utf16_lossy(&cut)));
            self.selection = range.start..range.start;
            self.marked = self
                .marked
                .take()
                .map(|marked| marked.start..marked.end - cut.len());
            cx.notify();
        }
        fn replace(&mut self, range: Option<Range<usize>>, text: &str) -> Range<usize> {
            let range = range
                .or(self.marked.clone())
                .unwrap_or(self.selection.clone());
            let units: Vec<u16> = text.encode_utf16().collect();
            let end = range.start + units.len();
            self.text.splice(range.clone(), units);
            self.selection = end..end;
            range.start..end
        }
    }
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let _scope = Scope::new(Callback::RenderTree);
            let (entity, focus) = (cx.entity(), self.focus.clone());
            let prepaint = |_: Bounds<Pixels>, _: &mut Window, _: &mut App| {
                drop(Scope::new(Callback::CanvasPrepaint));
            };
            let paint = move |bounds, (), window: &mut Window, cx: &mut App| {
                let _scope = Scope::new(Callback::CanvasPaint);
                window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx);
            };
            div()
                .size_full()
                .bg(rgb(0x2f1f3a))
                .text_color(rgb(0xffffff))
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::key))
                .child("SDK Cut first-query probe — injected Cut, OS routing unverified")
                .child(canvas(prepaint, paint).size_full())
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
            let _scope = Scope::new(Callback::Text);
            let range = range.start.min(self.text.len())..range.end.min(self.text.len());
            *adjusted = Some(range.clone());
            String::from_utf16(self.text.get(range)?).ok()
        }
        fn selected_text_range(
            &mut self,
            _: bool,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<UTF16Selection> {
            let _scope = Scope::new(Callback::Selection);
            let range = self.selection.clone();
            (!self.nil_selection).then_some(UTF16Selection {
                range,
                reversed: false,
            })
        }
        fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
            let _scope = Scope::new(Callback::Marked);
            self.marked.clone()
        }
        fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
            let _scope = Scope::new(Callback::Unmark);
            self.marked = None;
        }
        fn replace_text_in_range(
            &mut self,
            range: Option<Range<usize>>,
            text: &str,
            _: &mut Window,
            _: &mut Context<Self>,
        ) {
            let _scope = Scope::new(Callback::Replace);
            self.replace(range, text);
            self.marked = None;
        }
        fn replace_and_mark_text_in_range(
            &mut self,
            range: Option<Range<usize>>,
            text: &str,
            selected: Option<Range<usize>>,
            _: &mut Window,
            _: &mut Context<Self>,
        ) {
            let _scope = Scope::new(Callback::ReplaceMark);
            let marked = self.replace(range, text);
            if let Some(selected) = selected {
                self.selection = marked.start + selected.start..marked.start + selected.end;
            }
            self.marked = Some(marked);
        }
        fn set_selected_text_range(
            &mut self,
            range: Range<usize>,
            _: &mut Window,
            _: &mut Context<Self>,
        ) {
            let _scope = Scope::new(Callback::Select);
            self.selection = range;
        }
        fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
            let _scope = Scope::new(Callback::Length);
            Some(self.text.len())
        }
        fn bounds_for_range(
            &mut self,
            _: Range<usize>,
            bounds: Bounds<Pixels>,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<Bounds<Pixels>> {
            let _scope = Scope::new(Callback::Bounds);
            Some(bounds)
        }
        fn character_index_for_point(
            &mut self,
            _: Point<Pixels>,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<usize> {
            let _scope = Scope::new(Callback::CharacterIndex);
            None
        }
        fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
            let _scope = Scope::new(Callback::Accepts);
            true
        }
        fn text_input_configuration(
            &mut self,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> TextInputConfiguration {
            let _scope = Scope::new(Callback::Configuration);
            TextInputConfiguration::default()
        }
    }

    fn frame(mut app: AsyncWindowContext, f: impl FnOnce() + 'static) {
        app.on_next_frame(move |_, _| native::next_native_turn(f));
    }
    fn client(handle: WindowHandle<Probe>) -> NativeClient {
        let client = native::capture(handle.into());
        check(client.is_some(), "live-native-client");
        client.unwrap_or_else(|| unreachable!())
    }
    // Ordering first, from the stored native Result; the deferred and early
    // negatives fail here by name only for their intended witness.
    fn order(trace: &NativeCutObservationTrace) {
        let seq = |phase| {
            trace
                .events
                .iter()
                .flatten()
                .find(|e| e.phase == phase && e.refusal.is_none())
        };
        let deferred = trace
            .events
            .iter()
            .flatten()
            .find(|e| e.refusal == Some(R::DeferredExecuted));
        let first = seq(P::FirstGetterEntered).map(|e| e.sequence);
        let before = deferred.is_some_and(|d| first.is_none_or(|first| d.sequence < first));
        let crossed = trace.counters[C::TurnCrossings as usize];
        eprintln!(
            "CUT_QUERY_PROBE_TRACE: refused={:#x} crossings={crossed} len={}",
            trace.refused, trace.len
        );
        let witness = trace.has_refusal(R::DeferredExecuted) && crossed >= 1 && before;
        let any_deferred =
            trace.has_refusal(R::DeferredScheduled) || trace.has_refusal(R::DeferredExecuted);
        check(!witness, "p13-cut-first-before-next-turn");
        check(
            !any_deferred && crossed == 0,
            "cut-deferred-without-intended-witness",
        );
        let early = trace.has_refusal(R::EarlyQuery);
        check(
            !(early && trace.counters[C::QueryEntries as usize] == 2),
            "p13-cut-first-no-earlier-native-query",
        );
        check(!early, "cut-early-query-without-intended-witness");
    }
    fn main_case(
        handle: WindowHandle<Probe>,
        mut app: AsyncWindowContext,
        result: NativeCutProbeResult,
        next: usize,
    ) {
        let counts = COUNTS.with_borrow(|counts| *counts);
        let observation = match &result {
            Ok(observation) => Some(observation),
            Err(failure) => failure.observation.as_ref(),
        };
        check(observation.is_some(), "cut-observation-present");
        order(&observation.unwrap_or_else(|| unreachable!()).trace);
        check(
            result.as_ref().is_ok_and(|o| o.selector_returned),
            &format!("cut-ok {:?}", result.as_ref().err().map(|f| f.reason)),
        );
        let observation = result.unwrap_or_else(|_| unreachable!());
        let expected = NativeQuery {
            client: observation.client,
            value: Some(5..5),
        };
        check(
            observation.first_selection == Ok(expected),
            "cut-first-selection-5..5",
        );
        check(
            !counts.violation && balanced(&counts),
            "fixture-callbacks-legal-balanced",
        );
        let (key, cut) = (
            counts.enters[Callback::KeyDown as usize],
            counts.enters[Callback::CutDelete as usize],
        );
        check((key, cut) == (1, 1), "one-actual-cut");
        let draws = [
            Callback::RenderTree,
            Callback::CanvasPrepaint,
            Callback::CanvasPaint,
        ];
        check(
            draws.iter().all(|&c| counts.down_returned[c as usize] >= 1),
            "tail-draw-at-down-returned",
        );
        let length = client(handle).document_length_utf16().map(|q| q.value);
        check(length == Ok(Some(11)), "cut-native-length-11");
        let state = handle.update(&mut app, |view, _, cx| {
            let clip = cx.read_from_clipboard().and_then(|item| item.text());
            (
                String::from_utf16_lossy(&view.text),
                view.marked.clone(),
                clip,
            )
        });
        let expected = (
            "pre:変中:post".to_owned(),
            Some(4..6),
            Some("😀".to_owned()),
        );
        check(state.ok() == Some(expected), "cut-text-marked-clipboard");
        step(handle, app, next);
    }
    fn arm_case(client: &NativeClient) {
        COUNTS.set(CallbackCounts::default());
        CLIENT.set(Some(client.clone()));
    }
    fn refused(result: NativeCutProbeResult, reason: NativeCutProbeError, name: &str) {
        check(
            matches!(result, Err(f) if f.reason == reason && !f.selector_sent),
            name,
        );
    }
    fn step(handle: WindowHandle<Probe>, mut app: AsyncWindowContext, n: usize) {
        let args: Vec<String> = std::env::args().collect();
        let main_only = args
            .iter()
            .any(|a| a == "--main-only" || a == "--explicit-policy");
        let next = app.clone();
        let client = client(handle);
        match n {
            0 if client.text_readiness() != (true, true, true) => {
                return frame(app, move || step(handle, next, 0));
            }
            0 => {
                self_check();
                let _setup = Scope::new(Callback::Setup);
                client.insert("pre::post");
                check(
                    client.select_utf16(4..4)
                        == Ok(NativeQuery {
                            client: client.identity(),
                            value: true,
                        }),
                    "setup-caret-4",
                );
                client.mark("変😀中");
                check(
                    client.select_utf16(5..7).is_ok_and(|q| q.value),
                    "setup-selected-5..7",
                );
                let policy = Some(NativeEditAvailability {
                    copy: true,
                    cut: true,
                    paste: true,
                    select_all: true,
                });
                let explicit = args.iter().any(|a| a == "--explicit-policy");
                check(
                    !explicit || herdr_uikit::set_native_edit_availability(handle.into(), policy),
                    "explicit-policy",
                );
            }
            1 if NEXT_TURN => {
                arm_case(&client);
                let started = client.cut_and_observe_selection_next_turn(move |result| {
                    main_case(handle, next, result, 2)
                });
                return check(started.is_ok(), "next-turn-started");
            }
            1 => {
                arm_case(&client);
                let result = client.cut_and_observe_selection();
                return main_case(handle, app, result, if main_only { 9 } else { 2 });
            }
            2 => {
                let set = handle.update(&mut app, |view, _, cx| {
                    view.nil_selection = true;
                    cx.notify();
                });
                check(set.is_ok(), "nil-selection-mode");
            }
            3 => {
                arm_case(&client);
                let result = client.cut_and_observe_selection();
                let nil = NativeQuery {
                    client: client.identity(),
                    value: None,
                };
                check(
                    result.is_ok_and(|o| o.first_selection == Ok(nil)),
                    "cut-nil-selection-preserved",
                );
                let cuts = COUNTS.with_borrow(|counts| counts.enters[Callback::CutDelete as usize]);
                check(cuts == 1, "nil-case-one-actual-cut");
                native::key(handle.into(), "f9", false);
                refused(
                    LOAN.take().unwrap_or_else(|| Ok(unreachable_result())),
                    NativeCutProbeError::CallbackLoanActive,
                    "loan-refused-before-send",
                );
                let off = Some(NativeEditAvailability {
                    copy: true,
                    cut: false,
                    paste: true,
                    select_all: true,
                });
                check(
                    herdr_uikit::set_native_edit_availability(handle.into(), off),
                    "disable-cut",
                );
                refused(
                    client.cut_and_observe_selection(),
                    NativeCutProbeError::NotEligible,
                    "disabled-refused-before-send",
                );
                check(
                    herdr_uikit::set_native_edit_availability(handle.into(), None),
                    "restore-policy",
                );
                let receipt = retire_text_input_session(handle.into(), |_| {});
                check(receipt.is_ok(), "retire-last");
            }
            4 => refused(
                client_old(),
                NativeCutProbeError::ClientRetired,
                "retired-refused-before-send",
            ),
            _ => {
                eprintln!(
                    "CUT_QUERY_PROBE_PASS: cut first-query; native routing, P13 S1-S5/B1/S/Z/B2 UNVERIFIED"
                );
                std::process::exit(0);
            }
        }
        frame(app, move || {
            step(handle, next, if n == 4 { 9 } else { n + 1 })
        });
    }
    fn unreachable_result() -> herdr_uikit::retirement_probe::NativeCutSelectionObservation {
        check(false, "loan-case-reached-listener");
        unreachable!()
    }
    fn client_old() -> NativeCutProbeResult {
        let old = CLIENT.with_borrow(Clone::clone);
        old.map_or_else(
            || Ok(unreachable_result()),
            |old| old.cut_and_observe_selection(),
        )
    }
    pub fn run() {
        herdr_uikit::application().run(|cx: &mut App| {
            let opened = cx.open_window(WindowOptions::default(), |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    let (text, selection, marked) = (Vec::new(), 0..0, None);
                    Probe {
                        focus,
                        text,
                        selection,
                        marked,
                        nil_selection: false,
                    }
                })
            });
            check(opened.is_ok(), "open-real-uiwindow");
            let Ok(handle) = opened else { return };
            let app = handle.update(cx, |_, window, cx| window.to_async(cx));
            check(app.is_ok(), "capture-async-window");
            let Ok(app) = app else { return };
            let next = app.clone();
            frame(app, move || step(handle, next, 0));
        });
    }
}
fn main() {
    #[cfg(all(target_os = "ios", feature = "test-support"))]
    probe::run();
}
