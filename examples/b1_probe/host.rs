use gpui::{
    AsyncWindowContext, Bounds, Context, ElementInputHandler, EntityInputHandler, FocusHandle,
    KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, Pixels, Point, UTF16Selection, Window,
    WindowHandle, canvas, div, prelude::*, rgb,
};
use herdr_uikit::{
    NativeRetirementRequest, NativeRetirementRequestError as Error,
    NativeRetirementRequestStatus as Status, PrepareDecision, TextInputRetirementError,
    native_edit_action, native_input_origin, request_native_edit_retirement,
    retire_text_input_session, retirement_probe as native,
};
use std::{cell::RefCell, ops::Range, rc::Rc};

pub(super) fn check(ok: bool, name: &str) {
    if !ok {
        eprintln!("B1_PROBE_ASSERT: {name}");
        std::process::exit(2);
    }
    eprintln!("B1_PROBE_CHECK: {name}");
}
pub(super) fn require<T, E: std::fmt::Debug>(result: Result<T, E>, name: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => {
            eprintln!("B1_PROBE_ASSERT: {name}: {error:?}");
            std::process::exit(2);
        }
    }
}
pub(super) fn capture(handle: WindowHandle<Host>) -> native::NativeClient {
    match native::capture(handle.into()) {
        Some(client) => client,
        None => {
            check(false, "fixture-live-window");
            unreachable!()
        }
    }
}
pub(super) fn origin(handle: WindowHandle<Host>) -> herdr_uikit::NativeInputOrigin {
    match native_input_origin(handle.into()) {
        Some(origin) => origin,
        None => {
            check(false, "fixture-current-origin");
            unreachable!()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Job {
    Idle,
    Proceed,
    Fallback,
    InterruptQuery,
    Cancel,
}

pub(super) struct Record {
    pub job: Job,
    pub events: Vec<&'static str>,
    pub commits: Vec<String>,
    pub receipt: Option<NativeRetirementRequest>,
    pub prepare: usize,
    pub completed: usize,
    pub owner_valid: bool,
    pub acknowledged: bool,
    pub refusal_on_replace: bool,
    pub refusal_on_query: bool,
    pub refused: usize,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            job: Job::Idle,
            events: Vec::new(),
            commits: Vec::new(),
            receipt: None,
            prepare: 0,
            completed: 0,
            owner_valid: true,
            acknowledged: false,
            refusal_on_replace: false,
            refusal_on_query: false,
            refused: 0,
        }
    }
}
pub(super) struct Host {
    pub focus: FocusHandle,
    pub marked: String,
    pub record: Rc<RefCell<Record>>,
}

impl Host {
    fn key(&mut self, _: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if native_edit_action().is_none() {
            return;
        }
        let job = self.record.borrow().job;
        self.record.borrow_mut().events.push("down");
        if job != Job::Idle {
            let handle = window.window_handle();
            let expected = native_input_origin(handle).unwrap_or_else(|| {
                check(false, "edit-origin");
                unreachable!()
            });
            check(
                matches!(
                    retire_text_input_session(handle, |_| check(false, "legacy-busy-callback")),
                    Err(TextInputRetirementError::NativeCallbackInFlight)
                ),
                "legacy-busy-unchanged",
            );
            let weak = cx.entity().downgrade();
            let mut app = window.to_async(cx);
            let record = self.record.clone();
            let receipt = require(
                request_native_edit_retirement(handle, expected, move |prepared| {
                    let events = record.borrow().events.clone();
                    let down = events.iter().position(|e| *e == "down");
                    let up = events.iter().position(|e| *e == "up");
                    let modifiers = events.iter().position(|e| *e == "modifiers");
                    let sample = events.iter().rposition(|e| *e == "sample");
                    check(
                        matches!((down, up, modifiers, sample), (Some(d), Some(u), Some(m), Some(s)) if d < u && u < m && m < s),
                        "edit-whole-tail-before-prepare",
                    );
                    check(
                        native_edit_action().is_none(),
                        "native-origin-restored-before-prepare",
                    );
                    check(
                        record.borrow().receipt.as_ref().is_some_and(|r| {
                            r.key() == &prepared.key && r.status() == Status::Preparing
                        }),
                        "prepare-key-and-status",
                    );
                    record.borrow_mut().prepare += 1;
                    require(
                        app.update(|_, cx| weak.update(cx, |view, _| view.marked.clear()))
                            .and_then(|x| x),
                        "native-prepare-no-borrow",
                    );
                    let before = record
                        .borrow()
                        .events
                        .iter()
                        .filter(|e| **e == "modifiers")
                        .count();
                    check(native::callback_sentinel(handle), "sentinel-window-live");
                    check(
                        record
                            .borrow()
                            .events
                            .iter()
                            .filter(|e| **e == "modifiers")
                            .count()
                            == before + 1,
                        "public-api-callback-slot-restored",
                    );
                    if job == Job::Cancel {
                        check(
                            prepared.owner_cancelled && !record.borrow().owner_valid,
                            "cancel-ticket-terminal-order",
                        );
                        return PrepareDecision::Decline;
                    }
                    check(
                        !prepared.owner_cancelled && record.borrow().owner_valid,
                        "owner-ticket-before-proceed",
                    );
                    let key = prepared.key;
                    PrepareDecision::Proceed(Box::new(move |outcome| {
                        check(outcome.key == key, "terminal-request-key");
                        check(
                            record
                                .borrow()
                                .receipt
                                .as_ref()
                                .is_some_and(|r| r.status() == outcome.status),
                            "terminal-published-before-callback",
                        );
                        let ready = match &outcome.status {
                            Status::Replaced {
                                successor,
                                first_responder,
                                ..
                            } => {
                                *first_responder
                                    && native_input_origin(handle).as_ref() == Some(successor)
                            }
                            _ => false,
                        };
                        check(ready, "replacement-responder-acquired-before-input");
                        require(
                            app.update(|_, cx| {
                                weak.update(cx, |view, _| {
                                    let mut r = view.record.borrow_mut();
                                    r.completed += 1;
                                    if r.owner_valid && !r.acknowledged {
                                        r.acknowledged = true;
                                    }
                                })
                            })
                            .and_then(|x| x),
                            "terminal-owner-revalidation",
                        );
                    }))
                }),
                "native-edit-request-accepted",
            );
            check(
                matches!(
                    receipt.status(),
                    Status::Queued {
                        old_fenced_at_submission: false
                    }
                ),
                "queued-tail-live",
            );
            self.record.borrow_mut().receipt = Some(receipt.clone());
            if job == Job::Cancel {
                self.record.borrow_mut().owner_valid = false;
                check(
                    receipt.mark_owner_cancelled()
                        == herdr_uikit::NativeRetirementCancellation::Marked,
                    "cancel-owner-before-flag",
                );
            }
            if job == Job::InterruptQuery {
                // A fresh actual selector is not a continuation, even on old.
                if let Some(old) = native::capture(handle) {
                    let _ = old.selected_range_utf16();
                }
            }
        }
        if job != Job::Fallback {
            cx.stop_propagation();
        }
    }
    fn up(&mut self, _: &KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.record.borrow_mut().events.push("up");
    }
    fn modifiers(&mut self, _: &ModifiersChangedEvent, _: &mut Window, _: &mut Context<Self>) {
        self.record.borrow_mut().events.push("modifiers");
    }
    fn refuse(&mut self, window: &mut Window) {
        let handle = window.window_handle();
        let expected = native_input_origin(handle).unwrap_or_else(|| {
            check(false, "refusal-origin");
            unreachable!()
        });
        let before = herdr_uikit::text_input_session_state(handle);
        let result = request_native_edit_retirement(handle, expected, |_| {
            check(false, "unsupported-prepare");
            PrepareDecision::Decline
        });
        check(
            matches!(result, Err(Error::UnsupportedNativeEntry)),
            "unsupported-text-query-entry-no-retirement",
        );
        check(
            herdr_uikit::text_input_session_state(handle) == before,
            "refusal-no-fence-or-slot",
        );
        self.record.borrow_mut().refused += 1;
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
            .on_key_down(cx.listener(Self::key))
            .on_key_up(cx.listener(Self::up))
            .on_modifiers_changed(cx.listener(Self::modifiers))
            .child("SDK B1 injected native fixture; OS routing unverified")
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
        let units: Vec<_> = self.marked.encode_utf16().collect();
        let range = range.start.min(units.len())..range.end.min(units.len());
        *adjusted = Some(range.clone());
        String::from_utf16(units.get(range)?).ok()
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.record.borrow_mut().events.push("sample");
        let refuse = self.record.borrow().refusal_on_query;
        if refuse {
            self.record.borrow_mut().refusal_on_query = false;
            self.refuse(window);
        }
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
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if self.record.borrow().refusal_on_replace {
            self.refuse(window);
        }
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
}

pub(super) fn frame(mut app: AsyncWindowContext, f: impl FnOnce() + 'static) {
    app.on_next_frame(move |_, _| native::next_native_turn(f));
}
pub(super) fn task(app: &AsyncWindowContext, f: impl FnOnce() + 'static) {
    app.spawn(async move |_| f()).detach();
}
