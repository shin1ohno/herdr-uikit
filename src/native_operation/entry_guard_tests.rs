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
