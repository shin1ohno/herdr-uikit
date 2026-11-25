//! The fixture's cases, in order. The premises come first (the field takes
//! first responder over a GPUI window that keeps drawing, GPUI's views are
//! not first responder and not on the field's responder chain, composition
//! and Return); each later case first checks the field opened and only then
//! the one path it guards, so a mutation of that path fails at the case's own
//! assertion.
#![allow(unsafe_code, reason = "sends UIKit delegate selectors to the field")]

use super::{LABEL_TEXT, Log, Probe, check, native, record};
use gpui::{Bounds, point, px, size};
use herdr_uikit::{
    NativeInlineDismissReason as Reason, NativeInlineFieldError, NativeInlineFieldEvent as Event,
    NativeInlineFieldHandle, NativeInlineFieldRequest, NativeInlineFieldState, NativeSearchError,
    NativeSearchPresentation, NativeSearchRequest, NativeSearchResults, NativeSearchStyle,
    RestoredResponder, native_inline_field_press, native_inline_field_state,
    present_native_inline_field, present_native_search,
};
use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_ui_kit::{UITextField, UIWindow};

const FIELD_CLASS: &str = "HerdrNativeInlineField";
const ESCAPE: isize = 0x29;

fn request(probe: &Probe, allow_empty: bool) -> NativeInlineFieldRequest {
    NativeInlineFieldRequest {
        frame: probe.label.get(),
        text: LABEL_TEXT.into(),
        placeholder: if allow_empty {
            "Automatic name".into()
        } else {
            LABEL_TEXT.into()
        },
        accessibility_label: format!("Rename space {LABEL_TEXT}"),
        allow_empty,
        empty_hint: "Enter a name, or press Escape to cancel.".into(),
        length_hint: "Names are limited to 160 characters.".into(),
        ..NativeInlineFieldRequest::default()
    }
}

fn open(probe: &Probe, allow_empty: bool) -> (NativeInlineFieldHandle, Log) {
    match present_native_inline_field(probe.any(), request(probe, allow_empty)) {
        Ok((handle, events)) => (handle, Log::new(events)),
        Err(error) => {
            record(format!("present refused: {error:?}"));
            check(false, "present-accepted");
            std::process::exit(2);
        }
    }
}

fn state(probe: &Probe) -> NativeInlineFieldState {
    match native_inline_field_state(probe.any()) {
        Some(state) => state,
        None => {
            check(false, "qa-state-readable");
            std::process::exit(2);
        }
    }
}

fn field(window: &UIWindow) -> Option<Retained<UITextField>> {
    native::find_class(window, FIELD_CLASS).and_then(|view| view.downcast::<UITextField>().ok())
}

/// Waits for `Opened`, then checks the field owns first responder, neither
/// GPUI view does, and neither is on the field's responder chain.
async fn opened(probe: &Probe, window: &UIWindow, log: &mut Log, label: &str) {
    let ok = probe.wait(5_000, |_| log.has(&Event::Opened)).await;
    let s = state(probe);
    record(format!("{label}: state {s:?}"));
    check(ok, &format!("{label}-opened"));
    check(
        s.field_first_responder,
        &format!("{label}-field-first-responder"),
    );
    check(
        !s.gpui_text_first_responder && !s.gpui_drawing_first_responder,
        &format!("{label}-gpui-views-not-first-responder"),
    );
    let field = field(window);
    let text_view = native::gpui_text_view(window);
    let drawing = window.rootViewController().and_then(|r| r.viewIfLoaded());
    let in_chain = field.as_ref().is_some_and(|field| {
        text_view
            .as_ref()
            .is_some_and(|v| native::chain_contains(field, AsRef::<AnyObject>::as_ref(&**v)))
            || drawing
                .as_ref()
                .is_some_and(|v| native::chain_contains(field, AsRef::<AnyObject>::as_ref(&**v)))
    });
    if let Some(field) = &field {
        record(format!("{label}: field chain {:?}", native::chain(field)));
    }
    check(
        field.is_some() && !in_chain,
        &format!("{label}-gpui-views-not-in-field-chain"),
    );
}

/// Drops the handle and waits for the restore.
async fn finish(
    probe: &Probe,
    handle: NativeInlineFieldHandle,
    mut log: Log,
    label: &str,
) -> (Option<RestoredResponder>, Vec<Event>) {
    drop(handle);
    probe
        .wait(5_000, |_| {
            log.count(|e| matches!(e, Event::InputRestored(_))) == 1
        })
        .await;
    let restored = log.drain().iter().find_map(|e| match e {
        Event::InputRestored(r) => Some(*r),
        _ => None,
    });
    record(format!("{label}: restored {restored:?}"));
    (restored, log.seen.clone())
}

/// The field's delegate answer to Return, as the keyboard's Return asks it.
fn press_return(field: &UITextField) -> bool {
    // SAFETY: UITextField's delegate getter and UITextFieldDelegate's Return
    // selector, with the field itself.
    unsafe {
        let delegate: Option<Retained<AnyObject>> = msg_send![field, delegate];
        match delegate {
            Some(delegate) => msg_send![&*delegate, textFieldShouldReturn: field],
            None => true,
        }
    }
}

fn set_text(field: &UITextField, text: &str) {
    field.setText(Some(&objc2_foundation::NSString::from_str(text)));
}

/// Escape through the field's own press decision (`native_inline_field_press`):
/// XCUITest's Escape never reached a presented field, and an injected press
/// object cannot go on to UIKit's keyboard.
fn press_escape(probe: &Probe) -> bool {
    native_inline_field_press(probe.any(), &[ESCAPE])
}

pub(super) async fn run(probe: Probe) {
    probe.sleep(500).await;
    let Some(window) = native::gpui_window(probe.mtm) else {
        check(false, "gpui-window");
        return;
    };
    if let Some(root) = window.rootViewController().and_then(|r| r.viewIfLoaded()) {
        native::phase_label(&root, probe.mtm);
    }
    native::set_phase("starting");
    let drawn = probe
        .wait(10_000, |p| p.label.get().size.width > px(0.0))
        .await;
    record(format!("label bounds {:?}", probe.label.get()));
    check(drawn, "label-drawn");

    p1_premises(&probe, &window).await;
    p2_composition(&probe, &window).await;
    c1_return_commits_once(&probe, &window).await;
    c2_unchanged_and_empty(&probe, &window).await;
    c3_escape(&probe, &window).await;
    c4_set_frame_and_clip(&probe, &window).await;
    c5_shared_gate(&probe, &window).await;
    xcuitest_cases(&probe, &window).await;
    c6_window_close(&probe).await;
}

/// R1 premises: the field is first responder over a GPUI window that keeps
/// drawing; GPUI's views are not first responder and not on the field's
/// responder chain; nothing typed reaches GPUI; the box covers the label.
async fn p1_premises(probe: &Probe, window: &UIWindow) {
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "p1").await;
    let s = state(probe);
    check(
        s.text == LABEL_TEXT && s.phase == "Presented",
        "p1-field-shows-the-name",
    );
    let label = probe.label.get();
    let (x, y, w, h) = s.frame;
    let expected = (
        f32::from(label.origin.x) - 4.0,
        f32::from(label.origin.y) - 2.0,
        (f32::from(label.size.width) + 8.0).max(120.0),
        f32::from(label.size.height) + 4.0,
    );
    record(format!("p1: frame {:?} expected {expected:?}", s.frame));
    check(
        (x - expected.0).abs() <= 1.0
            && (y - expected.1).abs() <= 1.0
            && (w - expected.2).abs() <= 1.0
            && (h - expected.3).abs() <= 1.0,
        "p1-frame-matches-label-within-1pt",
    );
    // GPUI keeps drawing under the clear controller.
    let paints = probe.paints.get();
    probe.update(|_, window, _| window.refresh());
    let drew = probe.wait(2_000, |p| p.paints.get() > paints).await;
    record(format!("p1: paints {paints} -> {}", probe.paints.get()));
    check(drew, "p1-gpui-keeps-drawing-under-the-field");
    let sink = probe.sink();
    if let Some(field) = field(window) {
        native::insert_text(&field, "x");
    }
    probe.sleep(200).await;
    let s = state(probe);
    record(format!(
        "p1: after insert {:?} sink {:?}",
        s.text,
        probe.sink()
    ));
    check(
        probe.sink().text == sink.text && probe.sink().key_down == sink.key_down,
        "p1-typed-text-stays-in-the-field",
    );
    check(s.text == "x", "p1-selected-text-replaced-on-type");
    let _ = handle.dismiss();
    let dismissed = probe
        .wait(3_000, |_| log.has(&Event::Dismissed(Reason::Requested)))
        .await;
    check(dismissed, "p1-dismiss-requested");
    let (restored, events) = finish(probe, handle, log, "p1").await;
    check(
        matches!(
            restored,
            Some(RestoredResponder::Text | RestoredResponder::Drawing)
        ),
        "drop-restores-text-responder",
    );
    check(
        !events.iter().any(|e| matches!(e, Event::Committed(_))),
        "p1-nothing-committed",
    );
}

/// Composition: Return and Escape while marked do not close; whether UIKit
/// asks `shouldChangeCharactersInRange:` while marked is recorded.
async fn p2_composition(probe: &Probe, window: &UIWindow) {
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "p2").await;
    let Some(field) = field(window) else {
        check(false, "p2-field-found");
        return;
    };
    // Committed text before the composition: a Return that ignored the
    // composition would commit "abc".
    set_text(&field, "abc");
    native::set_marked(&field, "ぷろ");
    probe.sleep(200).await;
    let s = state(probe);
    record(format!("p2: marked {s:?}"));
    check(s.marked, "p2-field-marked");
    let returned = press_return(&field);
    press_escape(probe);
    probe.sleep(500).await;
    let s = state(probe);
    record(format!("p2: after Return/Escape while marked {s:?}"));
    check(
        !returned
            && s.phase == "Presented"
            && log.count(|e| matches!(e, Event::Committed(_) | Event::Dismissed(_))) == 0,
        "return-and-escape-while-marked-keep-open",
    );
    record(format!(
        "p2: shouldChangeCharactersInRange while marked: {}",
        s.should_change_while_marked
    ));
    native::unmark(&field);
    probe.sleep(200).await;
    let returned = press_return(&field);
    let committed = probe
        .wait(3_000, |_| log.has(&Event::Dismissed(Reason::Committed)))
        .await;
    let order = log.drain().to_vec();
    record(format!("p2: after commit {order:?}"));
    check(
        !returned && committed && order.contains(&Event::Committed("abcぷろ".into())),
        "p2-composition-commits-after-unmark",
    );
    finish(probe, handle, log, "p2").await;
}

/// Return commits once: `Committed` then `Dismissed(Committed)`; a second
/// Return and an outside dismissal add nothing.
async fn c1_return_commits_once(probe: &Probe, window: &UIWindow) {
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "c1").await;
    let Some(field) = field(window) else {
        check(false, "c1-field-found");
        return;
    };
    set_text(&field, "  build  ");
    press_return(&field);
    press_return(&field);
    let _ = handle.dismiss();
    probe
        .wait(3_000, |_| log.has(&Event::Dismissed(Reason::Committed)))
        .await;
    let (_, events) = finish(probe, handle, log, "c1").await;
    record(format!("c1: {events:?}"));
    let commits = events
        .iter()
        .filter(|e| matches!(e, Event::Committed(_)))
        .count();
    let dismissals = events
        .iter()
        .filter(|e| matches!(e, Event::Dismissed(_)))
        .count();
    let committed = events
        .iter()
        .position(|e| *e == Event::Committed("build".into()));
    let dismissed = events
        .iter()
        .position(|e| *e == Event::Dismissed(Reason::Committed));
    check(
        commits == 1
            && dismissals == 1
            && committed.is_some_and(|c| dismissed.is_some_and(|d| c < d)),
        "return-commits-then-dismisses",
    );
    check(
        matches!(events.last(), Some(Event::InputRestored(_))),
        "order-committed-dismissed-restored",
    );
}

/// The name unchanged closes without `Committed`; empty without
/// `allow_empty` keeps the field with the hint; with it commits "".
async fn c2_unchanged_and_empty(probe: &Probe, window: &UIWindow) {
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "c2u").await;
    if let Some(field) = field(window) {
        press_return(&field);
    }
    let unchanged = probe
        .wait(3_000, |_| log.has(&Event::Dismissed(Reason::Unchanged)))
        .await;
    let (_, events) = finish(probe, handle, log, "c2u").await;
    check(
        unchanged && !events.iter().any(|e| matches!(e, Event::Committed(_))),
        "unchanged-sends-nothing",
    );

    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "c2e").await;
    if let Some(field) = field(window) {
        set_text(&field, "  ");
        press_return(&field);
    }
    probe.sleep(500).await;
    let s = state(probe);
    record(format!("c2e: {s:?}"));
    check(
        s.phase == "Presented"
            && s.hint == "empty"
            && log.count(|e| matches!(e, Event::Committed(_) | Event::Dismissed(_))) == 0,
        "empty-return-shows-hint",
    );
    let _ = handle.dismiss();
    finish(probe, handle, log, "c2e").await;

    let (handle, mut log) = open(probe, true);
    opened(probe, window, &mut log, "c2a").await;
    if let Some(field) = field(window) {
        set_text(&field, "");
        press_return(&field);
    }
    let committed = probe
        .wait(3_000, |_| log.has(&Event::Committed(String::new())))
        .await;
    finish(probe, handle, log, "c2a").await;
    check(committed, "empty-with-allow-commits-empty");
}

/// Escape outside composition closes with `Escape`, nothing committed.
async fn c3_escape(probe: &Probe, window: &UIWindow) {
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "c3").await;
    if let Some(field) = field(window) {
        set_text(&field, "changed");
        press_escape(probe);
    }
    let escaped = probe
        .wait(3_000, |_| log.has(&Event::Dismissed(Reason::Escape)))
        .await;
    let (_, events) = finish(probe, handle, log, "c3").await;
    check(
        escaped && !events.iter().any(|e| matches!(e, Event::Committed(_))),
        "escape-dismisses-not-while-marked",
    );
}

/// `set_frame` moves the box within a frame; outside its clip it hides and
/// keeps editing.
async fn c4_set_frame_and_clip(probe: &Probe, window: &UIWindow) {
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "c4").await;
    let moved = Bounds::new(point(px(320.0), px(480.0)), size(px(160.0), px(20.0)));
    let _ = handle.set_frame(moved, None);
    probe.sleep(100).await;
    let s = state(probe);
    record(format!("c4: moved {s:?}"));
    let (x, y, w, h) = s.frame;
    check(
        (x - 316.0).abs() <= 1.0
            && (y - 478.0).abs() <= 1.0
            && (w - 168.0).abs() <= 1.0
            && (h - 24.0).abs() <= 1.0
            && !s.hidden,
        "set-frame-moves-container",
    );
    let clip = Bounds::new(point(px(0.0), px(0.0)), size(px(600.0), px(300.0)));
    let _ = handle.set_frame(moved, Some(clip));
    probe.sleep(100).await;
    let s = state(probe);
    record(format!("c4: clipped {s:?}"));
    check(
        s.hidden && s.field_first_responder && s.phase == "Presented",
        "hidden-outside-clip-keeps-editing",
    );
    let _ = handle.set_frame(moved, None);
    probe.sleep(100).await;
    check(!state(probe).hidden, "visible-again-inside-clip");
    let _ = handle.dismiss();
    finish(probe, handle, log, "c4").await;
}

fn search_request() -> NativeSearchRequest {
    NativeSearchRequest {
        anchor: Bounds::new(point(px(40.0), px(40.0)), size(px(24.0), px(24.0))),
        presentation: NativeSearchPresentation::Popover,
        title: "Search".into(),
        cancel_label: "Cancel".into(),
        cancel_accessibility_label: "Cancel search".into(),
        placeholder: "Name".into(),
        query: String::new(),
        width: 360.0,
        height: 420.0,
        style: NativeSearchStyle::default(),
        results: NativeSearchResults::default(),
    }
}

/// The search panel and the field share one gate: whichever comes second
/// is `AlreadyPresented`.
async fn c5_shared_gate(probe: &Probe, window: &UIWindow) {
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "c5").await;
    let search = present_native_search(probe.any(), search_request());
    check(
        matches!(search, Err(NativeSearchError::AlreadyPresented)),
        "gate-shared-search-refused-while-field-up",
    );
    let _ = handle.dismiss();
    finish(probe, handle, log, "c5").await;
    match present_native_search(probe.any(), search_request()) {
        Ok((search, mut events)) => {
            let inline = present_native_inline_field(probe.any(), request(probe, false));
            check(
                matches!(inline, Err(NativeInlineFieldError::AlreadyPresented)),
                "gate-shared-field-refused-while-search-up",
            );
            let _ = search.dismiss();
            drop(search);
            probe
                .wait(5_000, |_| {
                    std::iter::from_fn(|| events.try_next())
                        .any(|e| matches!(e, herdr_uikit::NativeSearchEvent::InputRestored(_)))
                })
                .await;
        }
        Err(error) => {
            record(format!("c5: search refused {error:?}"));
            check(false, "c5-search-opens");
        }
    }
    probe.sleep(500).await;
}

/// Real input from XCUITest: typing and Return, an outside tap that
/// reaches nothing else, and Japanese romaji (recorded: whether the
/// Simulator composes is not asserted).
async fn xcuitest_cases(probe: &Probe, window: &UIWindow) {
    native::set_phase("await-tap");
    let tapped = probe.wait(300_000, |p| p.sink().taps > 0).await;
    check(tapped, "xcuitest-attached");
    probe.sleep(2_000).await;

    // s1: the test taps the field, types "abc" over the selected name and
    // presses Return.
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "s1").await;
    native::set_phase("s1-open");
    let sink = probe.sink();
    let committed = probe
        .wait(150_000, |_| {
            log.count(|e| matches!(e, Event::Committed(name) if name.ends_with("abc"))) == 1
        })
        .await;
    record(format!("s1: {:?} sink {:?}", log.drain(), probe.sink()));
    check(committed, "s1-typed-name-committed");
    check(
        probe.sink().text == sink.text && probe.sink().key_down == sink.key_down,
        "s1-no-key-reached-gpui",
    );
    finish(probe, handle, log, "s1").await;
    probe.sleep(3_000).await;

    // s2: a tap outside the field closes it once and reaches nothing else.
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "s2").await;
    native::set_phase("s2-open");
    let taps = probe.sink().taps;
    // The test taps about 12 s after s1's Return; bounded well inside the
    // harness's wait, so a missing dismissal fails here.
    let outside = probe
        .wait(45_000, |_| log.has(&Event::Dismissed(Reason::OutsideTap)))
        .await;
    probe.sleep(1_500).await;
    let s = state(probe);
    record(format!(
        "s2: {:?} sink taps {taps}->{} {s:?}",
        log.drain(),
        probe.sink().taps
    ));
    check(outside, "outside-tap-dismissed");
    check(
        log.count(|e| matches!(e, Event::Dismissed(_))) == 1 && probe.sink().taps == taps,
        "outside-tap-dismisses-once-and-sink-sees-zero",
    );
    finish(probe, handle, log, "s2").await;
    probe.sleep(3_000).await;

    // s3: Control-Space to Japanese, "puro", Return twice; recorded.
    let (handle, mut log) = open(probe, false);
    opened(probe, window, &mut log, "s3").await;
    native::set_phase("s3-open");
    let mut marked_seen = false;
    let mut language = None;
    let closed = probe
        .wait(150_000, |p| {
            if let Some(s) = native_inline_field_state(p.any()) {
                marked_seen |= s.marked;
                if s.primary_language.is_some() {
                    language.clone_from(&s.primary_language);
                }
            }
            log.count(|e| matches!(e, Event::Dismissed(_))) > 0
        })
        .await;
    let events = log.drain().to_vec();
    record(format!(
        "s3: Japanese romaji: primary_language={language:?} marked_seen={marked_seen} closed={closed} events={events:?}"
    ));
    let s = state(probe);
    record(format!(
        "s3: shouldChangeCharactersInRange while marked: {}",
        s.should_change_while_marked
    ));
    let _ = handle.dismiss();
    finish(probe, handle, log, "s3").await;
    native::set_phase("s-done");
    probe.sleep(2_000).await;
}

/// Closing the window ends an open session with one `WindowClosed` and a
/// `WindowClosed` restore. Last: the fixture has no window afterwards.
async fn c6_window_close(probe: &Probe) {
    let (handle, mut log) = open(probe, false);
    let up = probe.wait(5_000, |_| log.has(&Event::Opened)).await;
    check(up, "c6-opened");
    probe.update(|_, window, _| window.remove_window());
    let closed = probe
        .wait(5_000, |_| {
            log.has(&Event::InputRestored(RestoredResponder::WindowClosed))
        })
        .await;
    let events = log.drain().to_vec();
    record(format!("c6: {events:?}"));
    let dismissals = events
        .iter()
        .filter(|e| matches!(e, Event::Dismissed(_)))
        .count();
    check(
        closed && dismissals == 1 && events.contains(&Event::Dismissed(Reason::WindowClosed)),
        "window-close-dismisses-once",
    );
    drop(handle);
    native::set_phase("done");
    eprintln!("INLINE_PASS: native inline field session, gate and restore");
}
