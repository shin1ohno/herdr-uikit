//! The fixture's cases, in order. Every case first checks the panel opened
//! (or, for C1, failed) and only then the one path it guards, so a mutation
//! of that path fails at the case's own assertion (review F8).
#![allow(
    unsafe_code,
    reason = "sends UIKit table delegate selectors to the panel"
)]

use super::{Log, Probe, check, native, record};
use gpui::{Bounds, point, px, size};
use herdr_uikit::{
    NativeSearchDismissReason as Reason, NativeSearchEvent as Event, NativeSearchFailure,
    NativeSearchHandle, NativeSearchItem, NativeSearchPresentation, NativeSearchRequest,
    NativeSearchResults, NativeSearchSection, NativeSearchStyle, RestoredResponder,
    native_search_state, present_native_search,
    retirement_probe::{
        self as sdk,
        held_press::{
            HeldProbeKeySpec, HeldProbePhase, HeldProbeTarget, held_press, inject_held_press,
        },
    },
};
use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSIndexPath;
use objc2_ui_kit::{UISheetPresentationControllerDetentIdentifierLarge, UIView, UIViewController};

fn results(ids: [u64; 2], prefix: &str) -> NativeSearchResults {
    NativeSearchResults {
        status: Some(format!("{prefix}: 2 spaces")),
        message: None,
        sections: vec![NativeSearchSection {
            title: "SPACES".into(),
            items: ids
                .iter()
                .map(|id| NativeSearchItem {
                    id: *id,
                    title: format!("{prefix}-{id}"),
                    detail: "pro-dev".into(),
                    dot: None,
                    accessibility_label: format!("Open {prefix}-{id}"),
                })
                .collect(),
        }],
        empty: None,
    }
}

fn request(presentation: NativeSearchPresentation) -> NativeSearchRequest {
    NativeSearchRequest {
        anchor: Bounds::new(point(px(40.0), px(40.0)), size(px(24.0), px(24.0))),
        presentation,
        title: "Search spaces and agents".into(),
        cancel_label: "Cancel".into(),
        cancel_accessibility_label: "Cancel search".into(),
        placeholder: "Name or host".into(),
        query: String::new(),
        width: 360.0,
        height: 420.0,
        style: NativeSearchStyle::default(),
        results: results([11, 12], "first"),
    }
}

fn open(probe: &Probe, presentation: NativeSearchPresentation) -> (NativeSearchHandle, Log) {
    match present_native_search(probe.any(), request(presentation)) {
        Ok((handle, events)) => (handle, Log::new(events)),
        Err(error) => {
            record(format!("present refused: {error:?}"));
            check(false, "present-accepted");
            std::process::exit(2);
        }
    }
}

fn state(probe: &Probe) -> herdr_uikit::NativeSearchState {
    match native_search_state(probe.any()) {
        Some(state) => state,
        None => {
            check(false, "qa-state-readable");
            std::process::exit(2);
        }
    }
}

/// Waits for `Opened`, then checks the field owns first responder and
/// neither GPUI view does (T-U1). Every case passes this before its own check.
async fn opened(probe: &Probe, log: &mut Log, label: &str) {
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
}

/// Drops the handle and waits for the restore (F6).
async fn finish(
    probe: &Probe,
    handle: NativeSearchHandle,
    mut log: Log,
    label: &str,
) -> Option<RestoredResponder> {
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
    restored
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
    let ready = probe
        .wait(10_000, |p| {
            sdk::capture(p.any()).is_some_and(|c| c.text_readiness() == (true, true, true))
        })
        .await;
    check(ready, "gpui-text-view-first-responder-before");

    c1_presenter_busy(&probe, &window).await;
    c2_responders_late_input_and_selection(&probe, &window).await;
    c3_held_key(&probe).await;
    xcuitest_cases(&probe).await;
    native::set_phase("done");
    eprintln!("PROBE_PASS: native search session, gate and restore");
}

/// F2/F8: a presenter that is already presenting is a common presentation
/// failure, reported as `Failed` (never `Opened` or `Dismissed`), and the gate
/// still opens.
async fn c1_presenter_busy(probe: &Probe, window: &objc2_ui_kit::UIWindow) {
    let Some(root) = window.rootViewController() else {
        check(false, "root-controller");
        return;
    };
    let blocker = UIViewController::new(probe.mtm);
    root.presentViewController_animated_completion(&blocker, false, None);
    probe.sleep(300).await;
    let (handle, mut log) = open(probe, NativeSearchPresentation::Popover);
    let failed = probe
        .wait(5_000, |_| {
            log.has(&Event::Failed(NativeSearchFailure::PresenterBusy))
        })
        .await;
    check(failed, "f2-presenter-busy-failed");
    check(
        log.count(|e| matches!(e, Event::Opened | Event::Dismissed(_))) == 0,
        "f8-failure-is-not-a-dismissal",
    );
    let restored = finish(probe, handle, log, "c1").await;
    check(restored.is_some(), "f2-failure-opens-the-gate");
    blocker.dismissViewControllerAnimated_completion(false, None);
    probe.sleep(500).await;
}

/// T-U2 (responder readers), F5 (late text and Edit), selection and query
/// callbacks, F6 (no restore before release; restore to the text view).
async fn c2_responders_late_input_and_selection(probe: &Probe, window: &objc2_ui_kit::UIWindow) {
    // GPUI's text view holds first responder (text mode) before the panel
    // takes it, so its own acquisition is armed: the readers below are what
    // keeps it from taking first responder back.
    probe.update(|host, window, cx| {
        host.focus.focus(window, cx);
        window.request_virtual_keyboard();
        window.refresh();
    });
    let text_first = probe
        .wait(5_000, |p| {
            native_search_state(p.any()).is_some_and(|s| s.gpui_text_first_responder)
        })
        .await;
    record(format!("c2: before open {:?}", state(probe)));
    check(text_first, "c2-gpui-text-first-before-open");
    let (handle, mut log) = open(probe, NativeSearchPresentation::Popover);
    opened(probe, &mut log, "c2").await;

    // T-U2: GPUI focus moving to a non-text and back, and GPUI asking for
    // the keyboard, must not take first responder from the field.
    probe.update(|_, window, cx| window.blur(cx));
    probe.sleep(600).await;
    let s = state(probe);
    check(
        s.field_first_responder && !s.gpui_drawing_first_responder,
        "t-u2-drawing-stays-off",
    );
    probe.update(|host, window, cx| {
        host.focus.focus(window, cx);
        window.request_virtual_keyboard();
        window.refresh();
    });
    probe.sleep(600).await;
    let s = state(probe);
    check(
        s.field_first_responder && !s.gpui_text_first_responder,
        "t-u2-text-stays-off",
    );
    // T-U2: the text view moving into its window again (UIKit rebuilding
    // the hierarchy) asks to acquire from didMoveToWindow while GPUI's focus
    // above wants text; the field keeps first responder.
    let rejoined = rejoin_text_view(window);
    probe.sleep(600).await;
    let s = state(probe);
    record(format!(
        "c2: text view rejoined {:?}, {s:?}",
        rejoined.is_some()
    ));
    check(
        rejoined.is_some() && s.field_first_responder && !s.gpui_text_first_responder,
        "t-u2-text-view-rejoined-stays-off",
    );

    // F5: late text and an Edit command reaching GPUI's views are dropped.
    let before = probe.sink();
    let drops = s.late_drops;
    if let Some(text_view) = native::gpui_text_view(window) {
        native::insert_text(&text_view, "LATE");
    }
    sdk::edit(probe.any(), herdr_uikit::NativeEditAction::Paste, true);
    let after = probe.sink();
    let s = state(probe);
    record(format!(
        "c2: late input sink-before={before:?} after={after:?} drops {drops}->{}",
        s.late_drops
    ));
    check(
        after.text == before.text && after.key_down == before.key_down,
        "f5-late-text-dropped",
    );
    check(s.late_drops > drops, "f5-late-input-observed");

    // Query and selection callbacks, with the results revision.
    if let Some(field) = native::find_class(window, "HerdrNativeSearchField") {
        native::insert_text(&field, "pro");
    }
    let query = probe
        .wait(2_000, |_| log.has(&Event::QueryChanged("pro".into())))
        .await;
    check(query, "query-changed-committed-text");
    select_row(window);
    let first = probe
        .wait(2_000, |_| {
            log.has(&Event::Selected {
                id: 11,
                revision: 0,
            })
        })
        .await;
    check(first, "selected-row-and-revision");
    let revision = handle.update(results([21, 22], "second")).unwrap_or(0);
    select_row(window);
    let second = probe
        .wait(2_000, |_| log.has(&Event::Selected { id: 21, revision }))
        .await;
    check(
        revision == 1 && second,
        "selected-after-update-carries-new-revision",
    );

    // F9: composition in the field is not a query; committing it and
    // closing at once still sends the committed text before the dismissal.
    let field = native::find_class(window, "HerdrNativeSearchField");
    if let Some(field) = &field {
        native::set_marked(field, "x");
    }
    probe.sleep(500).await;
    let marked_sent = log.count(|e| matches!(e, Event::QueryChanged(q) if q.contains('x')));
    record(format!("c2: marked {:?}", state(probe)));
    check(
        field.is_some() && state(probe).field_marked && marked_sent == 0,
        "f9-marked-text-is-not-a-query",
    );
    if let Some(field) = &field {
        native::unmark(field);
    }

    // F6: dismissed but still held: GPUI does not get first responder.
    let _ = handle.dismiss_selected(21);
    let dismissed = probe
        .wait(3_000, |_| log.has(&Event::Dismissed(Reason::Selected(21))))
        .await;
    check(dismissed, "dismissed-selected");
    let order: Vec<_> = log.drain().to_vec();
    let committed = order
        .iter()
        .position(|e| *e == Event::QueryChanged("prox".into()));
    let closed = order
        .iter()
        .position(|e| *e == Event::Dismissed(Reason::Selected(21)));
    record(format!("c2: after commit {order:?}"));
    check(
        committed.is_some_and(|q| closed.is_some_and(|d| q < d)),
        "f9-committed-query-before-dismissed",
    );
    probe.sleep(800).await;
    let s = state(probe);
    record(format!("c2: awaiting consumer {s:?}"));
    check(
        s.phase == "AwaitingConsumer"
            && !s.gpui_text_first_responder
            && !s.gpui_drawing_first_responder,
        "f6-no-restore-before-release",
    );
    // T-U2 with the panel gone: nothing UIKit presents holds first
    // responder now, so only the text view's own check keeps it from
    // taking first responder when it moves into its window again.
    let rejoined = rejoin_text_view(window);
    probe.sleep(800).await;
    let s = state(probe);
    record(format!(
        "c2: text view rejoined while awaiting {:?}, {s:?}",
        rejoined.is_some()
    ));
    check(
        rejoined.is_some()
            && s.phase == "AwaitingConsumer"
            && !s.gpui_text_first_responder
            && !s.gpui_drawing_first_responder,
        "t-u2-text-view-rejoined-while-awaiting-stays-off",
    );
    let restored = finish(probe, handle, log, "c2").await;
    let s = state(probe);
    check(
        restored == Some(RestoredResponder::Text)
            && s.gpui_text_first_responder
            && s.phase == "None",
        "f6-restored-text-responder",
    );
}

/// Moves GPUI's text view out of its window and back at the same index, as
/// UIKit does when it rebuilds the hierarchy: `didMoveToWindow` asks to
/// acquire first responder.
fn rejoin_text_view(window: &objc2_ui_kit::UIWindow) -> Option<()> {
    let text_view = native::gpui_text_view(window)?;
    let parent = text_view.superview()?;
    let index = parent
        .subviews()
        .iter()
        .position(|v| std::ptr::eq(&*v, &*text_view))?;
    text_view.removeFromSuperview();
    parent.insertSubview_atIndex(&text_view, index as isize);
    Some(())
}

/// Sends the table delegate's selection for row 0 of section 0.
fn select_row(window: &objc2_ui_kit::UIWindow) {
    let Some(table) = native::find_class(window, "UITableView") else {
        check(false, "results-table-found");
        return;
    };
    // SAFETY: UITableView's delegate getter and UITableViewDelegate's
    // selection selector with an index path the table shows.
    unsafe {
        let delegate: Option<Retained<AnyObject>> = msg_send![&*table, delegate];
        let path: Retained<NSIndexPath> =
            msg_send![objc2::class!(NSIndexPath), indexPathForRow: 0isize, inSection: 0isize];
        if let Some(delegate) = delegate {
            let () = msg_send![&*delegate, tableView: &*table, didSelectRowAtIndexPath: &*path];
        }
    }
}

/// F3: a key GPUI holds when the gate shuts gets its KeyUp first, its repeat
/// stops, and its later release (after the restore) never reaches GPUI;
/// a new press of it does.
async fn c3_held_key(probe: &Probe) {
    let Some(press) = held_press(HeldProbeKeySpec::ESCAPE) else {
        check(false, "held-press-object");
        return;
    };
    let down_before = probe.sink().key_down;
    let began = inject_held_press(
        probe.any(),
        &press,
        HeldProbePhase::Began,
        HeldProbeTarget::Current,
    );
    check(
        began.is_ok() && probe.sink().key_down == down_before + 1,
        "f3-held-press-reaches-gpui-before",
    );
    let up_before = probe.sink().key_up;
    let (handle, mut log) = open(probe, NativeSearchPresentation::Popover);
    check(
        probe.sink().key_up == up_before + 1,
        "f3-held-keyup-before-gate",
    );
    opened(probe, &mut log, "c3").await;
    let held = probe.sink().held;
    probe.sleep(1_200).await;
    let s = state(probe);
    record(format!(
        "c3: fenced {} sink {:?}",
        s.fenced_keys,
        probe.sink()
    ));
    check(probe.sink().held == held, "f3-repeat-stopped");
    check(s.fenced_keys == 1, "f3-held-key-fenced");
    let _ = handle.dismiss();
    probe
        .wait(3_000, |_| log.has(&Event::Dismissed(Reason::Requested)))
        .await;
    let restored = finish(probe, handle, log, "c3").await;
    check(restored.is_some(), "c3-restored");
    let up = probe.sink().key_up;
    let ended = inject_held_press(
        probe.any(),
        &press,
        HeldProbePhase::Ended,
        HeldProbeTarget::Current,
    );
    let s = state(probe);
    record(format!(
        "c3: release after restore {ended:?} sink {:?} fenced {}",
        probe.sink(),
        s.fenced_keys
    ));
    check(probe.sink().key_up == up, "f3-fenced-release-swallowed");
    check(s.fenced_keys == 0, "f3-release-ends-fence");
    let Some(again) = held_press(HeldProbeKeySpec::ESCAPE) else {
        check(false, "held-press-object-2");
        return;
    };
    let down = probe.sink().key_down;
    let _ = inject_held_press(
        probe.any(),
        &again,
        HeldProbePhase::Began,
        HeldProbeTarget::Current,
    );
    check(
        probe.sink().key_down == down + 1,
        "f3-new-press-reaches-gpui",
    );
    let _ = inject_held_press(
        probe.any(),
        &again,
        HeldProbePhase::Ended,
        HeldProbeTarget::Current,
    );
}

/// Cases with real input from the XCUITest (`NativeSearch.testProbe`), on
/// its fixed schedule: element queries do not answer while a panel is up.
async fn xcuitest_cases(probe: &Probe) {
    native::set_phase("await-tap");
    let tapped = probe.wait(300_000, |p| p.sink().taps > 0).await;
    check(tapped, "xcuitest-attached");
    // Lets the handshake tap finish (GPUI's focus and keyboard requests)
    // before the field takes first responder; opening at once raced with it
    // and the field's software keyboard sometimes never appeared.
    probe.sleep(2_000).await;

    // F4: Shift-Command-B (the app's menu command, a main-menu key command)
    // does nothing while the panel is up; the same command runs after it
    // closed (positive control).
    let actions = probe.actions.get();
    let (handle, mut log) = open(probe, NativeSearchPresentation::Popover);
    opened(probe, &mut log, "s1").await;
    native::set_phase("s1-open");
    probe.sleep(1_000).await;
    record(format!(
        "s1: keyboard notifications after 1 s {}",
        state(probe).keyboard_notifications_dropped
    ));
    let since = std::time::Instant::now();
    let mut notifications = None;
    // Long enough for XCUITest's wait for the app to idle, which gave up
    // after 57 s on some runs before typing.
    let typed = probe
        .wait(150_000, |_| {
            let s = native_search_state(probe.any());
            let count = s.as_ref().map(|s| s.keyboard_notifications_dropped);
            if count != notifications {
                record(format!(
                    "s1: +{} ms keyboard notifications {count:?}",
                    since.elapsed().as_millis()
                ));
                notifications = count;
            }
            s.is_some_and(|s| s.field_text.ends_with('#'))
        })
        .await;
    if !typed {
        record(format!(
            "s1: sentinel missing, {:?} sink {:?} actions {}",
            state(probe),
            probe.sink(),
            probe.actions.get()
        ));
    }
    check(typed, "s1-sentinel-typed");
    record(format!(
        "s1: actions {} -> {}",
        actions,
        probe.actions.get()
    ));
    check(
        probe.actions.get() == actions,
        "f4-menu-shortcut-disabled-while-open",
    );
    let returned = probe
        .wait(120_000, |_| log.has(&Event::Dismissed(Reason::Return)))
        .await;
    check(returned, "return-dismisses");
    let order: Vec<_> = log.drain().to_vec();
    let query = order
        .iter()
        .position(|e| *e == Event::QueryChanged("#".into()));
    let dismissed = order
        .iter()
        .position(|e| *e == Event::Dismissed(Reason::Return));
    check(
        query.is_some_and(|q| dismissed.is_some_and(|d| q < d)),
        "final-query-before-dismissed",
    );
    finish(probe, handle, log, "s1").await;
    // XCUITest's wait for app animations gave up after 60 s more than once
    // while the panel was up, so the shortcut came minutes late on some runs.
    let ran = probe
        .wait(300_000, |p| p.actions.get() == actions + 1)
        .await;
    check(ran, "f4-menu-shortcut-runs-after-close");

    // F2: dismissals UIKit makes on its own (outside tap, sheet swipe) are
    // session states with their own reason; the sheet's dimmed background
    // takes no touches.
    user_dismissal(
        probe,
        NativeSearchPresentation::Popover,
        Reason::OutsideTap,
        "f2-outside-tap-dismissed",
    )
    .await;
    user_dismissal(
        probe,
        NativeSearchPresentation::Sheet,
        Reason::SwipedDown,
        "f2-sheet-swipe-dismissed",
    )
    .await;
    inert_background(probe).await;
}

/// A tap on the sheet's dimmed background, 8 s after the swipe, neither
/// closes it nor reaches the window; the consumer then closes it.
async fn inert_background(probe: &Probe) {
    let name = "f2-sheet-background-inert";
    let (handle, mut log) = open(probe, NativeSearchPresentation::Sheet);
    opened(probe, &mut log, name).await;
    pin_large_detent(probe).await;
    let taps = probe.sink().taps;
    probe.sleep(12_000).await;
    let dismissals = log.count(|e| matches!(e, Event::Dismissed(_)));
    let s = state(probe);
    record(format!(
        "{name}: taps {taps} -> {}, dismissals {dismissals}, {s:?}",
        probe.sink().taps
    ));
    check(
        dismissals == 0
            && probe.sink().taps == taps
            && s.phase == "Presented"
            && s.field_first_responder,
        name,
    );
    let requested = handle.dismiss().is_ok()
        && probe
            .wait(5_000, |_| log.has(&Event::Dismissed(Reason::Requested)))
            .await;
    check(requested, &format!("{name}-then-requested"));
    let restored = finish(probe, handle, log, name).await;
    check(restored.is_some(), &format!("{name}-restored"));
}

/// Puts a sheet at its large detent (top at y 42 on this iPad), where the
/// XCUITest gestures start. The software keyboard raises it there on some
/// runs and not on others; without this the grabber is in either place.
async fn pin_large_detent(probe: &Probe) {
    probe.sleep(1_000).await;
    let sheet = native::gpui_window(probe.mtm)
        .and_then(|window| window.rootViewController())
        .and_then(|root| root.presentedViewController())
        .and_then(|panel| panel.sheetPresentationController());
    match sheet {
        Some(sheet) => {
            // SAFETY: an immutable UIKit constant.
            let large = unsafe { UISheetPresentationControllerDetentIdentifierLarge };
            sheet.setSelectedDetentIdentifier(Some(large));
        }
        None => record("sheet controller missing".into()),
    }
    probe.sleep(1_000).await;
}

/// The presented panel's frame in window points, and the window's size.
fn panel_frame(probe: &Probe) -> String {
    let Some(window) = native::gpui_window(probe.mtm) else {
        return "no window".into();
    };
    let to: &UIView = &window;
    let frame = window
        .rootViewController()
        .and_then(|root| root.presentedViewController())
        .and_then(|panel| panel.viewIfLoaded())
        .map(|view| view.convertRect_toView(view.bounds(), Some(to)));
    let bounds = window.bounds();
    match frame {
        Some(f) => format!(
            "x {} y {} w {} h {} in {}x{}",
            f.origin.x,
            f.origin.y,
            f.size.width,
            f.size.height,
            bounds.size.width,
            bounds.size.height
        ),
        None => "none".into(),
    }
}

/// Records the panel frame each time it changes while a case waits for a
/// gesture, with the milliseconds since the case opened.
struct FrameTrack {
    name: String,
    since: std::time::Instant,
    last: String,
}

impl FrameTrack {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            since: std::time::Instant::now(),
            last: String::new(),
        }
    }
    fn sample(&mut self, probe: &Probe) {
        let frame = panel_frame(probe);
        if frame != self.last {
            record(format!(
                "{}: +{} ms panel frame {frame}",
                self.name,
                self.since.elapsed().as_millis()
            ));
            self.last = frame;
        }
    }
}

/// Opens the panel and waits for the XCUITest gesture to dismiss it with
/// `reason`.
async fn user_dismissal(
    probe: &Probe,
    presentation: NativeSearchPresentation,
    reason: Reason,
    name: &str,
) {
    let (handle, mut log) = open(probe, presentation);
    opened(probe, &mut log, name).await;
    if presentation == NativeSearchPresentation::Sheet {
        pin_large_detent(probe).await;
    }
    let style = state(probe).presentation_style;
    record(format!("{name}: style {style:?}"));
    let mut track = FrameTrack::new(name);
    let dismissed = probe
        .wait(60_000, |p| {
            track.sample(p);
            log.has(&Event::Dismissed(reason))
        })
        .await;
    if !dismissed {
        record(format!("{name}: not dismissed, {:?}", state(probe)));
    }
    check(dismissed, name);
    let restored = finish(probe, handle, log, name).await;
    check(restored.is_some(), &format!("{name}-restored"));
}

#[allow(
    dead_code,
    reason = "keeps UIView in scope for the helpers' signatures"
)]
fn _view(_: &UIView) {}
