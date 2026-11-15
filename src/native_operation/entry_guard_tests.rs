//! Source guards: each independent native entry opens a refusal frame before its
//! body (docs/native-edit-retirement.md "Any distinct native entry ... interrupts
//! the queued request"). Dropping one is invisible to behavior tests, so these
//! read the sources; they are not native callback receipts.

const REFUSED: &str = "let _native_entry = crate::native_operation::refused();";
const RAW: &str = "let _raw_entry = crate::native_operation::raw(\"notification-callback\");";

/// The text of the body that follows `anchor`, from its first `{`.
fn body_after<'a>(source: &'a str, anchor: &str) -> &'a str {
    let at = source
        .find(anchor)
        .unwrap_or_else(|| panic!("anchor missing: {anchor}"));
    assert_eq!(
        source.matches(anchor).count(),
        1,
        "anchor must be unique: {anchor}"
    );
    let rest = &source[at..];
    let open = rest
        .find('{')
        .unwrap_or_else(|| panic!("no body after: {anchor}"));
    rest[open + 1..].trim_start()
}

#[test]
fn the_four_unframed_selectors_open_a_refusal_frame_first() {
    let notifications = include_str!("../platform/notifications.rs");
    let view = include_str!("../window/view.rs");
    let delegate = include_str!("../window/edit_menu/delegate.rs");
    for (source, selector) in [
        (view, "#[unsafe(method(resignFirstResponder))]"),
        (
            delegate,
            "#[unsafe(method_id(editMenuInteraction:menuForConfiguration:suggestedActions:))]",
        ),
        (
            notifications,
            "#[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]",
        ),
        (
            notifications,
            "#[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]",
        ),
    ] {
        assert!(
            body_after(source, selector).starts_with(REFUSED),
            "{selector} must start with the refusal frame"
        );
    }
}

#[test]
fn notification_main_queue_closures_open_a_raw_frame_first() {
    let source = include_str!("../platform/notifications.rs");
    let closure = "exec_async(move || {";
    // The two consumer-callback closures; the one-line `remove_identifiers`
    // closure only calls SDK-internal code and has no braces.
    assert_eq!(source.matches(closure).count(), 2, "closure set changed");
    for (at, _) in source.match_indices(closure) {
        let body = source[at + closure.len()..].trim_start();
        assert!(body.starts_with(RAW), "closure at byte {at} lacks raw()");
    }
}

/// The whole brace-balanced body of the item at `anchor`, whitespace removed.
/// Like `body_after`, the body starts at the first `{` after the anchor, so an
/// anchor must end before any braced argument type (none has one today).
fn exact_body(source: &str, anchor: &str) -> String {
    let body = body_after(source, anchor);
    let mut level = 1;
    let end = body
        .char_indices()
        .find(|&(_, c)| {
            level += i32::from(c == '{') - i32::from(c == '}');
            level == 0
        })
        .map(|(i, _)| i)
        .unwrap_or_else(|| panic!("unclosed body: {anchor}"));
    body[..end].split_whitespace().collect()
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect()
}

/// S9: each of the eight press selectors feeds the Window's held-press ledger
/// with its own phase as the first statement after its refusal frame. The
/// refusal frame must stay first: it interrupts a queued request before any
/// press is recorded (plan R5 is subsumed by this ordering).
#[test]
fn press_selectors_feed_the_ledger() {
    let text = include_str!("../window/text_input/view_class.rs");
    let drawing = include_str!("../window/view.rs");
    for (source, record) in [
        (
            text,
            "self.ivars().target.record_presses(PressPhase::{}, presses);",
        ),
        (
            drawing,
            "if let Some(window) = self.owner() { window.record_presses(PressPhase::{}, presses); }",
        ),
    ] {
        for (selector, phase) in [
            ("pressesBegan", "Began"),
            ("pressesChanged", "Changed"),
            ("pressesEnded", "Ended"),
            ("pressesCancelled", "Cancelled"),
        ] {
            let anchor = format!("#[unsafe(method({selector}:withEvent:))]");
            let body = body_after(source, &anchor);
            assert!(
                body.starts_with(REFUSED),
                "{selector} must start with the refusal frame"
            );
            let rest = body[REFUSED.len()..].trim_start();
            assert!(
                rest.starts_with(&record.replace("{}", phase)),
                "{selector} must feed the ledger with PressPhase::{phase}"
            );
        }
    }
}

/// G1: the adapter from `NSSet<UIPress>` to `PressFact`s cannot be exercised
/// without UIKit on the host, so its exact text is fixed here; the ledger
/// rules it feeds are tested in `press_ledger`.
#[test]
fn press_adapter_bodies_are_fixed() {
    let state = include_str!("../window/state.rs");
    assert_eq!(
        exact_body(
            state,
            "pub(super) fn record_presses(&self, phase: press_ledger::PressPhase, presses: &NSSet<UIPress>)"
        ),
        squash(
            "let facts: Vec<_> = presses
                .iter()
                .map(|press| {
                    press_ledger::PressFact::new(
                        press.key(self.mtm).map(|key| key.keyCode().0),
                        Retained::as_ptr(&press).addr(),
                        press,
                    )
                })
                .collect();
            press_ledger::apply(
                &mut self.state.borrow_mut().press_ledger,
                phase,
                facts,
                std::time::Instant::now(),
            );"
        ),
        "window-adapter-maps-usage-identity-phase"
    );
    let target = include_str!("../window/input_target.rs");
    assert_eq!(
        exact_body(target, "pub(crate) fn record_presses("),
        squash(
            "if let Some(window) = self.window.upgrade() {
                window.record_presses(phase, presses);
            }"
        ),
        "text-adapter-forwards-phase"
    );
}

/// S8 / G2: every key-state field reaches the second witness, and the text
/// view hands its own state to `hardware_held`.
#[test]
fn key_state_witness_maps_every_field() {
    let keys = include_str!("../window/text_input/keys.rs");
    assert_eq!(
        exact_body(keys, "pub(super) fn witness(&self) -> KeyWitness"),
        squash(
            "KeyWitness {
                presses: self.presses.len(),
                repeat: self.repeat.is_some(),
                menu_press: self.menu_press.is_some(),
                modifiers: self.held != HeldModifiers::default(),
            }"
        ),
        "witness-maps-every-field"
    );
    assert_eq!(
        exact_body(
            keys,
            "pub(in crate::window) fn key_witness(&self) -> KeyWitness"
        ),
        squash("self.ivars().keys.borrow().witness()"),
        "text-view-witness-is-its-key-state"
    );
}
