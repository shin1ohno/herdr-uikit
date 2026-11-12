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
