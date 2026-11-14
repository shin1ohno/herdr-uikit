use super::{response_fixture::NotificationResponseFixture, *};
use std::time::{Duration, Instant};
const DEFAULT: &str = "com.apple.UNNotificationDefaultActionIdentifier";

#[test]
fn current_six_second_response_uses_issued_receipt_not_display_age() {
    let fixture = NotificationResponseFixture::default();
    let start = Instant::now();
    let display = Arc::new(AtomicBool::new(true));
    let current = display.clone();
    fixture
        .issue("six", start, Authorization::Authorized, move || {
            current.load(Ordering::Acquire)
        })
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(fixture.presents("six"));
    display.store(false, Ordering::Release); // injected display age now exceeds five seconds
    assert!(!fixture.presents("six"));
    let mut calls = 0;
    assert!(
        fixture.respond("six", DEFAULT, start + Duration::from_secs(6), |response| {
            calls += 1;
            assert_eq!(response.tag.as_ref(), "six");
            assert!(response.action_id.is_none());
        }),
        "D5 six-second SDK response must reach consumer"
    );
    assert_eq!(calls, 1);
    assert!(!fixture.presents("six"));
}

#[test]
fn removed_expired_and_duplicate_responses_reach_guarded_consumer() {
    let fixture = NotificationResponseFixture::default();
    let start = Instant::now();
    for tag in ["removed", "expired", "duplicate"] {
        fixture
            .issue(tag, start, Authorization::Authorized, || true)
            .unwrap_or_else(|e| panic!("{e:?}"));
    }
    fixture.remove("removed");
    let mut calls = Vec::new();
    for (tag, seconds) in [
        ("removed", 6),
        ("expired", 301),
        ("duplicate", 6),
        ("duplicate", 7),
    ] {
        assert!(
            fixture.respond(tag, DEFAULT, start + Duration::from_secs(seconds), |r| {
                calls.push(r.tag)
            })
        );
        assert!(!fixture.presents(tag));
    }
    assert_eq!(calls.len(), 4); // consumer, not the display gate, decides refusal
}

#[test]
fn unissued_denied_dismissed_and_malformed_responses_never_activate() {
    let fixture = NotificationResponseFixture::default();
    let now = Instant::now();
    assert_eq!(
        fixture.issue("denied", now, Authorization::Denied, || true),
        Err(NotificationError::Permission(Authorization::Denied))
    );
    assert_eq!(
        fixture.issue("suppressed", now, Authorization::Authorized, || false),
        Err(NotificationError::Cancelled)
    );
    fixture
        .issue("issued", now, Authorization::Authorized, || true)
        .unwrap_or_else(|e| panic!("{e:?}"));
    for (tag, action) in [
        ("unknown", DEFAULT),
        ("denied", DEFAULT),
        ("suppressed", DEFAULT),
        ("issued", "com.apple.UNNotificationDismissActionIdentifier"),
        ("", DEFAULT),
    ] {
        assert!(!fixture.respond(tag, action, now, |_| panic!("unowned activation")));
    }
    assert!(fixture.presents("issued"));
    let mut action = None;
    assert!(fixture.respond("issued", "unsupported", now, |r| action = r.action_id));
    assert!(action.is_some()); // consumer refuses actions; never reinterpret as default
}

#[test]
fn receipt_bounds_expiry_failure_and_predicate_teardown() {
    let presentations = presentation::Registry::new();
    let responses = response::Registry::new();
    let now = Instant::now();
    let owner = Arc::new(());
    for n in 0..65 {
        let retained = owner.clone();
        let tag = format!("receipt-{n}");
        let ticket = presentations
            .insert(
                &tag,
                Arc::new(move || {
                    let _ = &retained;
                    true
                }),
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
        responses.issue(&tag, &ticket, now);
        presentations.remove(&[tag]);
    }
    assert_eq!(
        Arc::strong_count(&owner),
        1,
        "receipt must not retain consumer predicate"
    );
    assert!(!responses.owns("receipt-0", now));
    for n in 1..65 {
        assert!(responses.owns(&format!("receipt-{n}"), now));
    }
    assert!(responses.owns("receipt-64", now + Duration::from_secs(599)));
    assert!(!responses.owns("receipt-64", now + Duration::from_secs(600)));
    let ticket = presentations
        .insert("failed", Arc::new(|| true))
        .unwrap_or_else(|e| panic!("{e:?}"));
    responses.issue("failed", &ticket, now);
    responses.forget("failed", &ticket);
    assert!(!responses.owns("failed", now));
    presentations.finish("failed", &ticket);
    assert!(!ticket.current());
}
