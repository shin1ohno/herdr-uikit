use super::*;
#[test]
fn injected_permission_states_never_imply_success() {
    for state in [
        Authorization::NotDetermined,
        Authorization::Denied,
        Authorization::Provisional,
        Authorization::Ephemeral,
        Authorization::Unknown,
    ] {
        assert_eq!(
            require_authorization(|| state),
            Err(NotificationError::Permission(state))
        );
    }
    assert_eq!(require_authorization(|| Authorization::Authorized), Ok(()));
}
#[test]
fn native_settings_adapter_preserves_every_status() {
    for (native, expected) in [
        (
            UNAuthorizationStatus::NotDetermined,
            Authorization::NotDetermined,
        ),
        (UNAuthorizationStatus::Denied, Authorization::Denied),
        (UNAuthorizationStatus::Authorized, Authorization::Authorized),
        (
            UNAuthorizationStatus::Provisional,
            Authorization::Provisional,
        ),
        (UNAuthorizationStatus::Ephemeral, Authorization::Ephemeral),
    ] {
        assert_eq!(authorization(native), expected);
    }
    assert_eq!(
        authorization(UNAuthorizationStatus(900)),
        Authorization::Unknown
    );
}
#[test]
fn malformed_payload_never_reaches_native_center() {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    post_notification(
        SystemNotification {
            tag: "".into(),
            title: "test".into(),
            body: "".into(),
            actions: vec![],
        },
        move |result| {
            let _ = sender.try_send(result);
        },
    );
    assert_eq!(
        receiver.try_recv(),
        Ok(Err(NotificationError::InvalidPayload))
    );
}
// This calls the actual OS query but neither requests authorization nor posts.
// Run explicitly in an app-bundled native harness; model tests do not prove OS delivery.
#[test]
#[ignore = "requires exclusive native harness lease; does not grant permission"]
fn actual_center_authorization_callback() {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    notification_authorization(move |state| {
        let _ = sender.try_send(state);
    });
    let state = receiver.recv_timeout(std::time::Duration::from_secs(5));
    assert!(state.is_ok(), "native center did not answer: {state:?}");
}

fn assert_presentation(ticket: Option<&presentation::Ticket>, show: bool) {
    let mut calls = 0;
    complete_presentation(ticket, |options| {
        calls += 1;
        let expected = if show {
            UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List
        } else {
            UNNotificationPresentationOptions::empty()
        };
        assert_eq!(options, expected, "final native display decision");
    });
    assert_eq!(calls, 1, "native completion must be called exactly once");
}

#[test]
fn add_check_disconnect_will_present_completion_remove_never_shows() {
    let registry = presentation::Registry::new();
    let active = Arc::new(AtomicBool::new(true));
    let current = active.clone();
    let ticket = registry
        .insert("race", Arc::new(move || current.load(Ordering::Acquire)))
        .unwrap_or_else(|e| panic!("insert: {e:?}"));
    // The add-request guard passed; OS completion is deliberately still pending.
    assert!(ticket.current());
    assert_presentation(registry.get("race").as_deref(), true);
    active.store(false, Ordering::Release);
    assert_presentation(registry.get("race").as_deref(), false);
    // A later submission completion/removal cannot undo a banner already shown.
    assert!(!ticket.current());
    registry.finish("race", &ticket);
    registry.remove(&["race".into()]);
    assert_presentation(registry.get("race").as_deref(), false);
}

#[test]
fn remove_before_authorization_or_display_invalidates_retained_callback() {
    let registry = presentation::Registry::new();
    let ticket = registry
        .insert("removed", Arc::new(|| true))
        .unwrap_or_else(|e| panic!("insert: {e:?}"));
    registry.remove(&["removed".into()]);
    assert!(!ticket.current());
    assert_presentation(Some(&ticket), false);
    assert_presentation(registry.get("removed").as_deref(), false);
    assert_presentation(None, false);
}

#[test]
fn unrelated_removal_preserves_current_tag_and_bounds_predicate_lifetime() {
    let registry = presentation::Registry::new();
    let owner = Arc::new(());
    for n in 0..64 {
        let owner = owner.clone();
        let predicate = Arc::new(move || {
            let _ = &owner;
            true
        });
        assert!(registry.insert(&format!("tag-{n}"), predicate).is_ok());
    }
    assert!(matches!(
        registry.insert("overflow", Arc::new(|| true)),
        Err(NotificationError::Busy)
    ));
    assert!(matches!(
        registry.insert("tag-0", Arc::new(|| true)),
        Err(NotificationError::Busy)
    ));
    registry.remove(&["tag-0".into()]);
    assert_eq!(Arc::strong_count(&owner), 64);
    assert_presentation(registry.get("tag-1").as_deref(), true);
    registry.remove(&(1..64).map(|n| format!("tag-{n}")).collect::<Vec<_>>());
    assert_eq!(Arc::strong_count(&owner), 1);
}

#[test]
fn predicate_can_invalidate_without_reentering_registry_lock() {
    let registry = Arc::new(presentation::Registry::new());
    let weak = Arc::downgrade(&registry);
    let ticket = registry
        .insert(
            "self-remove",
            Arc::new(move || {
                if let Some(registry) = weak.upgrade() {
                    registry.remove(&["self-remove".into()]);
                }
                true
            }),
        )
        .unwrap_or_else(|e| panic!("insert: {e:?}"));
    assert_presentation(Some(&ticket), false);
}

#[test]
fn stale_completion_cannot_retire_new_ticket_with_same_identifier() {
    let registry = presentation::Registry::new();
    let old = registry
        .insert("tag", Arc::new(|| true))
        .unwrap_or_else(|e| panic!("{e:?}"));
    registry.remove(&["tag".into()]);
    let new = registry
        .insert("tag", Arc::new(|| true))
        .unwrap_or_else(|e| panic!("{e:?}"));
    registry.finish("tag", &old);
    assert!(!old.current());
    assert!(new.current());
    assert_presentation(registry.get("tag").as_deref(), true);
}

#[test]
fn successful_add_completion_does_not_release_final_context() {
    let registry = presentation::Registry::new();
    let active = Arc::new(AtomicBool::new(true));
    let current = active.clone();
    let ticket = registry
        .insert(
            "accepted",
            Arc::new(move || current.load(Ordering::Acquire)),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(ticket.current()); // successful add completion, not presentation
    drop(ticket);
    assert_presentation(registry.get("accepted").as_deref(), true);
    active.store(false, Ordering::Release);
    assert_presentation(registry.get("accepted").as_deref(), false);
}
