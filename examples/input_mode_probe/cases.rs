//! The fixture's cases, in order (plan §3.2). Each case first checks that the
//! text view is first responder and the interaction is active (or, for the
//! cases about the inactive state, its own precondition), then the one path
//! it guards, so a mutation of that path fails at the case's own assertion.
//! The reference control runs first: the cases that need the indicator to
//! appear are judged only when a standard field shows it under the same
//! conditions; otherwise they are recorded as not observable here.

use super::{CARET, Probe, check, native, record, uikit};
use herdr_uikit::{InputModeIndicatorState, retirement_probe as sdk};
use objc2::rc::Retained;
use objc2_ui_kit::{UIView, UIWindow};

/// Milliseconds after the key arrived within which an indicator must appear.
const APPEAR_MS: u64 = 2_000;
/// How long a requested key may take to arrive. XCUITest waits for the app
/// to be idle before it types, which took over 60 s on a loaded Mac.
const KEY_MS: u64 = 150_000;

/// Numbered requests for the XCUITest (`act-<n>-<verb>` in the phase label).
struct Acts(u32);

impl Acts {
    /// Posts one request and waits until `ack` grows (the key or tap
    /// arrived). Returns whether it did within `ms`.
    async fn send(
        &mut self,
        probe: &Probe,
        verb: &str,
        ms: u64,
        ack: impl Fn(&Probe) -> u32,
    ) -> bool {
        self.0 += 1;
        let before = ack(probe);
        native::set_phase(&format!("act-{}-{verb}", self.0));
        let arrived = probe.wait(ms, |p| ack(p) > before).await;
        if !arrived {
            record(format!(
                "act {} {verb}: no acknowledgement in {ms} ms",
                self.0
            ));
        }
        arrived
    }

    /// Posts one request that GPUI cannot see arrive (Shift alone reaches
    /// no GPUI listener), then waits `ms` for the XCUITest to perform it.
    async fn blind(&mut self, probe: &Probe, verb: &str, ms: u64) {
        self.0 += 1;
        native::set_phase(&format!("act-{}-{verb}", self.0));
        probe.sleep(ms).await;
    }
}

fn controls(probe: &Probe) -> u32 {
    probe.sink().control_downs
}
fn taps(probe: &Probe) -> u32 {
    probe.sink().taps
}
fn field_presses(_: &Probe) -> u32 {
    native::field_presses()
}

fn ready(state: &InputModeIndicatorState) -> bool {
    state.text_first_responder && state.activated
}

/// Every case's precondition: the text view is first responder and the
/// interaction is active (AC12).
async fn precondition(probe: &Probe, label: &str) -> InputModeIndicatorState {
    probe.wait(5_000, |p| ready(&p.state())).await;
    let state = probe.state();
    record(format!("{label}: state {state:?}"));
    check(
        state.text_first_responder,
        &format!("{label}-precondition-first-responder"),
    );
    check(state.activated, &format!("{label}-precondition-active"));
    state
}

/// Waits for no visible indicator (an earlier one fading out), then sends one
/// input source switch and returns the first indicator that was not visible
/// before it, within `APPEAR_MS` of the key arriving. A key that never
/// arrived fails `label`'s check: it was no switch, so it is no miss either.
async fn switch(
    probe: &Probe,
    acts: &mut Acts,
    ack: fn(&Probe) -> u32,
    label: &str,
) -> Option<uikit::Accessory> {
    probe
        .wait(6_000, |p| uikit::accessories(p.mtm).is_empty())
        .await;
    let before: Vec<usize> = uikit::accessories(probe.mtm).iter().map(|a| a.id).collect();
    if !before.is_empty() {
        record(format!(
            "indicator still visible before the switch: {before:?}"
        ));
    }
    let arrived = acts.send(probe, "switch", KEY_MS, ack).await;
    check(arrived, &format!("{label}-key-arrived"));
    let mut found = None;
    probe
        .wait(APPEAR_MS, |p| {
            found = uikit::accessories(p.mtm)
                .into_iter()
                .find(|a| !before.contains(&a.id));
            found.is_some()
        })
        .await;
    found
}

fn dump(probe: &Probe, label: &str) {
    record(format!("{label}: view hierarchy follows"));
    for line in uikit::dump_all(probe.mtm) {
        eprintln!("INPUTMODE_DUMP: {label} {line}");
    }
}

fn adjacent(accessory: [f64; 4], caret: [f64; 4]) -> bool {
    let caret_bottom = caret[1] + caret[3];
    let top_ok = accessory[1] >= caret_bottom - 1.0 && accessory[1] <= caret_bottom + 8.0;
    let (left, right) = (caret[0] - 40.0, caret[0] + caret[2] + 160.0);
    let overlap = accessory[0] < right && accessory[0] + accessory[2] > left;
    top_ok && overlap
}

fn near(a: Option<[f64; 4]>, x: f64, y: f64) -> bool {
    a.is_some_and(|r| (r[0] - x).abs() <= 0.5 && (r[1] - y).abs() <= 0.5)
}

pub(super) async fn run(probe: Probe) {
    probe.sleep(500).await;
    let Some(window) = native::gpui_window(probe.mtm) else {
        check(false, "gpui-window");
        return;
    };
    let Some(root) = window.rootViewController().and_then(|r| r.viewIfLoaded()) else {
        check(false, "gpui-root-view");
        return;
    };
    native::phase_label(&root, probe.mtm);
    native::set_phase("starting");

    im0_precondition(&probe, &window).await;
    let mut acts = Acts(0);
    // Handshake: the XCUITest is attached once its first tap arrives.
    let attached = acts.send(&probe, "tap", 300_000, taps).await;
    check(attached, "xcuitest-attached");
    probe.sleep(2_000).await;

    let (observable, pixel_rule) = reference_control(&probe, &window, &mut acts).await;
    im3_empty_document(&probe, &window, &mut acts, observable).await;
    im1_switch_shows_indicator(&probe, &mut acts, observable).await;
    im2_transparent_cursor(&probe, &window, pixel_rule).await;
    im4_adjacent_to_caret_after_move(&probe, &mut acts, observable).await;
    im7_marked_survives_caret_moves(&probe, &window).await;
    im10_retap_no_reshow(&probe, &mut acts).await;
    im5_key_only_deactivates(&probe).await;
    im5_requested_while_responder(&probe, &window).await;
    im6_retire_successor(&probe).await;
    im6_close_leaves_none(&probe, &root).await;
    eprintln!(
        "INPUTMODE_PASS: input mode indicator (indicator cases {})",
        if observable {
            "judged"
        } else {
            "not observable in this Simulator"
        }
    );
}

/// The window attached one interaction for the current text view, and GPUI's
/// focused text element made it first responder and switched it on.
async fn im0_precondition(probe: &Probe, window: &UIWindow) {
    probe
        .wait(10_000, |p| {
            let s = p.state();
            ready(&s) && s.interactions_on_drawing_view == 1
        })
        .await;
    let state = precondition(probe, "im0").await;
    check(
        state.interactions_on_drawing_view == 1,
        "im0-one-interaction-on-drawing-view",
    );
    check(
        state.text_input_is_current,
        "im0-text-input-is-current-view",
    );
    // Report the caret once, as a terminal does after drawing.
    probe.move_caret(CARET.0, CARET.1);
    let reported = probe
        .wait(3_000, |p| {
            near(p.state().caret_rect, f64::from(CARET.0), f64::from(CARET.1))
        })
        .await;
    check(reported, "im0-caret-reported");
    // The QA caret is the one GPUI reported; UIKit's own query agrees.
    let uikit = native::gpui_text_view(window).and_then(|text| uikit::uikit_caret(&text));
    let qa = probe.state().caret_rect;
    record(format!("im0: caret qa {qa:?} uikit {uikit:?}"));
    check(
        uikit.is_some_and(|u| {
            qa.is_some_and(|q| u.iter().zip(q).all(|(a, b)| (a - b).abs() <= 0.5))
        }),
        "im0-qa-caret-matches-uikit",
    );
}

/// The reference: a standard field, focused, in the hardware keyboard state
/// (a Shift press), shows the indicator on a switch. Also the pixel rule's
/// positive control: the field's own blue caret must be found.
async fn reference_control(probe: &Probe, window: &UIWindow, acts: &mut Acts) -> (bool, bool) {
    let field = uikit::reference_field(window, probe.mtm);
    let focused = field.becomeFirstResponder();
    probe.wait(3_000, |_| uikit::is_first(&field)).await;
    check(
        focused && uikit::is_first(&field),
        "reference-field-first-responder",
    );
    acts.blind(probe, "shift", 3_000).await;
    let mut shown = 0;
    for n in 0..3 {
        let found = switch(probe, acts, field_presses, &format!("reference-{n}")).await;
        record(format!(
            "reference switch {n}: {found:?} field caret {:?}",
            uikit::field_caret(&field, window)
        ));
        shown += usize::from(found.is_some());
    }
    let caret = uikit::field_caret(&field, window);
    let mut widest: f64 = 0.0;
    for n in 0..6 {
        if let Some(shot) = uikit::screenshot(window) {
            widest = widest.max(uikit::tinted_run(&shot, caret));
            if n < 3 {
                record(format!(
                    "reference shot {n}: {}",
                    uikit::caret_row(&shot, caret)
                ));
            }
        }
        probe.sleep(250).await;
    }
    record(format!(
        "reference: {shown}/3 switches showed the indicator; caret {caret:?} tinted run {widest:.1} pt"
    ));
    field.resignFirstResponder();
    field.removeFromSuperview();
    probe.update(|host, w, cx| {
        host.focus.focus(w, cx);
        w.request_virtual_keyboard();
    });
    let back = probe.wait(10_000, |p| ready(&p.state())).await;
    check(back, "gpui-text-view-reacquired-after-reference");
    let observable = shown > 0;
    if !observable {
        record("reference field showed no indicator: AC1/3/4 are not observable in this Simulator (owner check D2)".into());
    }
    (observable, widest >= 1.0)
}

/// AC3: with an empty document and the selection at 0..0, a switch shows it.
async fn im3_empty_document(probe: &Probe, window: &UIWindow, acts: &mut Acts, observable: bool) {
    precondition(probe, "im3").await;
    let Some(text) = native::gpui_text_view(window) else {
        check(false, "im3-text-view");
        return;
    };
    let (document, selection) = uikit::document(&text);
    record(format!(
        "im3: document {document:?} selection {selection:?}"
    ));
    check(
        document.is_empty() && selection == (0, 0),
        "im3-empty-document-before-switch",
    );
    acts.blind(probe, "shift", 3_000).await;
    let found = switch(probe, acts, controls, "im3").await;
    record(format!(
        "im3: indicator {found:?} qa {:?}",
        probe.state().accessory_frame
    ));
    if observable {
        if found.is_none() {
            dump(probe, "im3-miss");
        }
        check(found.is_some(), "im3-indicator-on-empty-document");
    }
}

/// AC1: ten switches, the indicator in at least nine; a dump for each miss.
/// The SDK's QA sees the same indicator on the drawing view.
async fn im1_switch_shows_indicator(probe: &Probe, acts: &mut Acts, observable: bool) {
    let mut shown = 0;
    let mut qa_seen = 0;
    for n in 1..=10 {
        precondition(probe, &format!("im1-{n}")).await;
        let found = switch(probe, acts, controls, &format!("im1-{n}")).await;
        let qa = probe.state().accessory_frame;
        record(format!("im1 switch {n}: {found:?} qa {qa:?}"));
        shown += usize::from(found.is_some());
        qa_seen += usize::from(found.is_some() && qa.is_some());
        if found.is_none() {
            dump(probe, &format!("im1-miss-{n}"));
        }
    }
    record(format!(
        "im1: {shown}/10 switches showed the indicator; QA saw {qa_seen}"
    ));
    if observable {
        check(shown >= 9, "im1-nine-of-ten-switches");
        check(qa_seen == shown, "im1-qa-sees-each-indicator");
    }
}

/// AC2: the cursor view is the transparent one, and no system caret (a 2 pt
/// tinted column) is drawn over the caret, over six samples (it blinks). The
/// pixel rule counts only after it found the reference field's own caret.
async fn im2_transparent_cursor(probe: &Probe, window: &UIWindow, pixel_rule: bool) {
    let state = precondition(probe, "im2").await;
    check(
        state.cursor_class == "HerdrTransparentCursorView",
        "im2-transparent-cursor-class",
    );
    check(pixel_rule, "pixel-rule-finds-the-reference-caret");
    let Some(caret) = state.caret_rect else {
        check(false, "im2-caret-rect");
        return;
    };
    let mut widest: f64 = 0.0;
    let mut shots = 0;
    for _ in 0..6 {
        if let Some(shot) = uikit::screenshot(window) {
            shots += 1;
            widest = widest.max(uikit::tinted_run(&shot, caret));
        }
        probe.sleep(250).await;
    }
    record(format!(
        "im2: {shots} screenshots, widest tinted run {widest:.1} pt at {caret:?}"
    ));
    check(shots == 6, "im2-screenshots");
    check(widest < 1.0, "im2-no-system-caret-pixels");
}

/// AC4 and gate G6: the indicator touches the caret, after the caret moved
/// (one re-read request for one move).
async fn im4_adjacent_to_caret_after_move(probe: &Probe, acts: &mut Acts, observable: bool) {
    let before = precondition(probe, "im4").await;
    let (x, y) = (600.0_f32, 420.0_f32);
    probe.move_caret(x, y);
    let moved = probe
        .wait(3_000, |p| {
            near(p.state().caret_rect, f64::from(x), f64::from(y))
        })
        .await;
    let after = probe.state();
    record(format!(
        "im4: caret {:?} -> {:?}, re-read requests {} -> {}",
        before.caret_rect, after.caret_rect, before.needs_update_calls, after.needs_update_calls
    ));
    check(moved, "im4-caret-moved");
    check(
        after.needs_update_calls == before.needs_update_calls + 1,
        "im4-one-move-one-reread",
    );
    let found = switch(probe, acts, controls, "im4").await;
    let qa = probe.state().accessory_frame;
    record(format!(
        "im4: indicator {found:?} qa {qa:?} uikit caret {:?}",
        after.caret_rect
    ));
    if observable {
        let (Some(frame), Some(caret)) = (qa, after.caret_rect) else {
            dump(probe, "im4-miss");
            check(false, "im4-indicator-shown");
            return;
        };
        check(adjacent(frame, caret), "im4-adjacent-to-moved-caret");
    }
    probe.move_caret(CARET.0, CARET.1);
    probe
        .wait(3_000, |p| {
            near(p.state().caret_rect, f64::from(CARET.0), f64::from(CARET.1))
        })
        .await;
}

/// AC7 and gate G7: marked text survives caret moves, and a move tells the
/// input delegate nothing. The composition is set once the last switch's
/// indicator is gone and held for 3 s with no move (the control), so a
/// composition UIKit ends on its own is told apart from one a move ends.
async fn im7_marked_survives_caret_moves(probe: &Probe, window: &UIWindow) {
    precondition(probe, "im7").await;
    let Some(text) = native::gpui_text_view(window) else {
        check(false, "im7-text-view");
        return;
    };
    probe
        .wait(6_000, |p| uikit::accessories(p.mtm).is_empty())
        .await;
    probe.sleep(2_000).await;
    native::set_marked(&text, "かな");
    let marked = probe.wait(3_000, |p| p.sink().marked == "かな").await;
    check(marked && uikit::has_marked(&text), "im7-marked-before");
    let unmarks = probe.sink().unmarks;
    probe.sleep(3_000).await;
    let control = probe.sink();
    record(format!(
        "im7 control (3 s, no move): marked {:?}, unmarks {} -> {}, uikit marked {}",
        control.marked,
        unmarks,
        control.unmarks,
        uikit::has_marked(&text)
    ));
    check(
        control.marked == "かな" && uikit::has_marked(&text),
        "im7-control-composition-held",
    );
    let notified = sdk::delegate_notifications();
    let commits = probe.sink().commits.len();
    for n in 1..=5_u8 {
        let x = CARET.0 + 9.0 * f32::from(n);
        probe.move_caret(x, CARET.1);
        let moved = probe
            .wait(3_000, |p| {
                near(p.state().caret_rect, f64::from(x), f64::from(CARET.1))
            })
            .await;
        check(moved, &format!("im7-caret-move-{n}"));
        probe.sleep(300).await;
        let sink = probe.sink();
        record(format!(
            "im7 move {n}: marked {:?}, unmarks {}, uikit marked {}, delegate {}",
            sink.marked,
            sink.unmarks,
            uikit::has_marked(&text),
            sdk::delegate_notifications()
        ));
    }
    probe.sleep(500).await;
    let sink = probe.sink();
    record(format!(
        "im7: delegate notifications {notified} -> {}, marked {:?}, commits {:?}, unmarks {}",
        sdk::delegate_notifications(),
        sink.marked,
        sink.commits,
        sink.unmarks
    ));
    check(
        sdk::delegate_notifications() == notified,
        "im7-no-delegate-traffic",
    );
    check(
        sink.marked == "かな" && uikit::has_marked(&text),
        "im7-marked-survives",
    );
    check(sink.commits.len() == commits, "im7-nothing-committed");
    native::unmark(&text);
    probe.move_caret(CARET.0, CARET.1);
    probe.sleep(500).await;
}

/// AC10: tapping the focused terminal again neither toggles the interaction
/// nor shows the indicator.
async fn im10_retap_no_reshow(probe: &Probe, acts: &mut Acts) {
    precondition(probe, "im10").await;
    probe
        .wait(6_000, |p| uikit::accessories(p.mtm).is_empty())
        .await;
    let tapped = acts.send(probe, "tap", KEY_MS, taps).await;
    check(tapped, "im10-tap-arrived");
    let mut stayed = true;
    let mut appeared = false;
    for _ in 0..40 {
        let state = probe.state();
        stayed &= ready(&state);
        appeared |= !uikit::accessories(probe.mtm).is_empty();
        probe.sleep(50).await;
    }
    record(format!(
        "im10: stayed active {stayed}, indicator appeared {appeared}"
    ));
    check(stayed, "im10-stays-active");
    check(!appeared, "im10-no-reshow");
}

fn accepts(probe: &Probe, on: bool) {
    probe.update(|host, _, cx| {
        host.accepts = on;
        cx.notify();
    });
}

/// AC5 and gates G1/G3: key-only mode resigns the text view and switches the
/// interaction off; taking text again switches it on.
async fn im5_key_only_deactivates(probe: &Probe) {
    precondition(probe, "im5").await;
    accepts(probe, false);
    let off = probe
        .wait(5_000, |p| {
            let s = p.state();
            !s.activated && !s.text_first_responder
        })
        .await;
    record(format!("im5: key-only {:?}", probe.state()));
    check(off, "im5-key-only-deactivates");
    accepts(probe, true);
    let on = probe.wait(5_000, |p| ready(&p.state())).await;
    record(format!("im5: text again {:?}", probe.state()));
    check(on, "im5-text-again-activates");
}

/// Gate G2: a text request reaching a view that is already first responder
/// (UIKit made it first responder itself) switches the interaction on.
async fn im5_requested_while_responder(probe: &Probe, window: &UIWindow) {
    precondition(probe, "im5b").await;
    accepts(probe, false);
    probe.wait(5_000, |p| !p.state().activated).await;
    let Some(text) = native::gpui_text_view(window) else {
        check(false, "im5b-text-view");
        return;
    };
    let became = text.becomeFirstResponder();
    probe.wait(3_000, |_| uikit::is_first(&text)).await;
    let state = probe.state();
    record(format!("im5b: UIKit-made responder {became} {state:?}"));
    check(
        state.text_first_responder && !state.activated,
        "im5b-precondition-responder-inactive",
    );
    accepts(probe, true);
    let on = probe.wait(5_000, |p| ready(&p.state())).await;
    record(format!("im5b: requested {:?}", probe.state()));
    check(on, "im5b-request-while-responder-activates");
}

/// AC6 and gates G9/G10: after retirement the successor has the one
/// interaction, and it switches on when the successor acquires.
async fn im6_retire_successor(probe: &Probe) {
    precondition(probe, "im6").await;
    let receipt = match herdr_uikit::retire_text_input_session(probe.any(), |_| {}) {
        Ok(receipt) => receipt,
        Err(error) => {
            record(format!("im6: retirement refused {error:?}"));
            check(false, "im6-retirement-accepted");
            return;
        }
    };
    let replaced = probe
        .wait(5_000, |_| {
            matches!(
                receipt.status(),
                herdr_uikit::TextInputRetirementStatus::Replaced { .. }
            )
        })
        .await;
    check(replaced, "im6-replaced");
    probe.wait(5_000, |p| ready(&p.state())).await;
    let state = probe.state();
    record(format!(
        "im6: after retirement {state:?} receipt {:?}",
        receipt.status()
    ));
    check(
        state.interactions_on_drawing_view == 1,
        "im6-one-interaction-after-retirement",
    );
    check(state.text_input_is_current, "im6-successor-is-text-input");
    check(ready(&state), "im6-successor-active");
}

/// AC6 and gate G12: closing the window leaves no interaction on the drawing
/// view, which the fixture keeps alive to look.
async fn im6_close_leaves_none(probe: &Probe, root: &UIView) {
    let state = precondition(probe, "im6c").await;
    check(
        state.interactions_on_drawing_view == 1,
        "im6c-one-interaction-before-close",
    );
    let drawing: Retained<UIView> = objc2::Message::retain(root);
    native::set_phase("closing");
    probe.sleep(1_500).await;
    probe.update(|_, window, _| window.remove_window());
    let none = probe
        .wait(5_000, |_| uikit::selection_interactions(&drawing) == 0)
        .await;
    record(format!(
        "im6c: interactions on the closed window's drawing view {}",
        uikit::selection_interactions(&drawing)
    ));
    check(none, "im6c-close-leaves-none");
}
