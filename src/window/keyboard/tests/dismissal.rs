use super::*;

fn active_responder(keyboard: Keyboard) -> ResignContext {
    keyboard.resign_context(true, true, true)
}

fn resigned_text_client() -> Context {
    Context {
        first_responder: false,
        ..text_client()
    }
}

#[test]
fn hide_key_resigning_before_will_hide_records_a_user_dismissal() {
    let mut keyboard = visible();
    keyboard.resigned(active_responder(keyboard));
    assert_eq!(keyboard.dismissal, Dismissal::User);
    assert!(!keyboard.minimized);

    keyboard = on_keyboard_event(keyboard, Event::WillHide, resigned_text_client());
    keyboard = on_keyboard_event(keyboard, Event::DidHide, resigned_text_client());
    assert_eq!(keyboard.dismissal, Dismissal::User);
    assert_eq!(keyboard.visibility, Visibility::Hidden);
    assert!(!keyboard.minimized);
}

#[test]
fn a_hide_notification_records_dismissal_when_native_resign_already_finished() {
    let keyboard = on_keyboard_event(visible(), Event::WillHide, resigned_text_client());
    assert_eq!(keyboard.dismissal, Dismissal::User);
    assert_eq!(keyboard.visibility, Visibility::Hiding);
    assert!(!keyboard.minimized);
}

#[test]
fn user_dismissal_survives_focus_lost_then_gained_until_explicit_show() {
    let mut keyboard = visible();
    keyboard.resigned(active_responder(keyboard));
    keyboard.focus_changed();
    keyboard.focus_changed();
    assert_eq!(keyboard.dismissal, Dismissal::User);
    assert!(!keyboard.dismissal.allows_text());

    keyboard.show();
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert!(keyboard.dismissal.allows_text());
}

#[test]
fn acquisition_and_native_show_do_not_clear_a_user_dismissal() {
    let mut keyboard = visible();
    keyboard.resigned(active_responder(keyboard));
    keyboard.acquired();
    let context = Context {
        text_requested: false,
        ..text_client()
    };
    keyboard = on_keyboard_event(keyboard, Event::WillShow, context);
    keyboard = on_keyboard_event(keyboard, Event::DidShow, context);
    assert_eq!(keyboard.visibility, Visibility::Visible);
    assert_eq!(keyboard.dismissal, Dismissal::User);
}

#[test]
fn application_dismissal_keeps_its_existing_focus_change_semantics() {
    let mut keyboard = visible();
    keyboard.hide();
    keyboard.resigned(active_responder(keyboard));
    assert_eq!(keyboard.dismissal, Dismissal::Application);

    keyboard.focus_changed();
    assert_eq!(keyboard.dismissal, Dismissal::None);
    keyboard.hide();
    keyboard.show();
    assert_eq!(keyboard.dismissal, Dismissal::None);
}

#[test]
fn an_application_hide_never_becomes_a_user_dismissal() {
    let mut keyboard = visible();
    keyboard.hide();
    keyboard.resigned(active_responder(keyboard));
    keyboard = on_keyboard_event(keyboard, Event::WillHide, resigned_text_client());
    keyboard = on_keyboard_event(keyboard, Event::DidHide, resigned_text_client());
    assert_eq!(keyboard.dismissal, Dismissal::Application);
    assert!(!keyboard.minimized);
}

#[test]
fn intentional_resigns_and_their_hide_notifications_do_not_record_user_dismissal() {
    for reason in [
        ResignReason::FocusChange,
        ResignReason::Inactive,
        ResignReason::LeavingWindow,
        ResignReason::Restart,
    ] {
        let mut keyboard = visible();
        keyboard.expect_hide(reason);
        keyboard.resigned(active_responder(keyboard));
        keyboard = on_keyboard_event(keyboard, Event::WillHide, resigned_text_client());
        keyboard = on_keyboard_event(keyboard, Event::DidHide, resigned_text_client());
        assert_eq!(keyboard.dismissal, Dismissal::None, "{reason:?}");
        assert!(!keyboard.minimized, "{reason:?}");
    }
}

#[test]
fn a_visible_keyboard_is_not_suppressed_after_background_and_return() {
    let mut keyboard = visible();
    keyboard.expect_hide(ResignReason::Inactive);
    keyboard.resigned(active_responder(keyboard));
    let background = Context {
        active: false,
        first_responder: false,
        ..text_client()
    };
    keyboard = on_keyboard_event(keyboard, Event::WillHide, background);
    keyboard = on_keyboard_event(keyboard, Event::DidHide, background);
    assert_eq!(keyboard.dismissal, Dismissal::None);

    keyboard.acquired();
    keyboard = on_keyboard_event(keyboard, Event::WillShow, text_client());
    keyboard = on_keyboard_event(keyboard, Event::DidShow, text_client());
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert_eq!(keyboard.visibility, Visibility::Visible);
}

#[test]
fn a_user_hidden_keyboard_stays_suppressed_after_background_and_return() {
    let mut keyboard = visible();
    keyboard.resigned(active_responder(keyboard));
    keyboard = on_keyboard_event(keyboard, Event::WillHide, resigned_text_client());
    keyboard = on_keyboard_event(keyboard, Event::DidHide, resigned_text_client());
    keyboard.expect_hide(ResignReason::Inactive);
    keyboard.resigned(ResignContext::default());
    keyboard.acquired();
    keyboard.focus_changed();
    assert_eq!(keyboard.dismissal, Dismissal::User);
    assert_eq!(keyboard.visibility, Visibility::Hidden);
}

#[test]
fn an_unexpected_native_responder_such_as_an_alert_counts_as_user_dismissal() {
    let mut keyboard = visible();
    keyboard.resigned(active_responder(keyboard));
    assert_eq!(keyboard.dismissal, Dismissal::User);
}

#[test]
fn inactive_or_unfocused_native_resigns_do_not_count_as_user_dismissal() {
    for context in [
        ResignContext {
            active: false,
            ..active_responder(visible())
        },
        ResignContext {
            first_responder: false,
            ..active_responder(visible())
        },
        ResignContext {
            text_requested: false,
            ..active_responder(visible())
        },
    ] {
        let mut keyboard = visible();
        keyboard.resigned(context);
        assert_eq!(keyboard.dismissal, Dismissal::None, "{context:?}");
    }
}

#[test]
fn an_explicit_minimized_restart_fences_its_stale_hide_completion() {
    let mut keyboard = on_keyboard_event(visible(), Event::WillHide, text_client());
    assert!(keyboard.minimized);
    keyboard.show();
    keyboard.expect_hide(ResignReason::Restart);
    keyboard.resigned(active_responder(keyboard));
    keyboard = on_keyboard_event(keyboard, Event::WillHide, resigned_text_client());
    keyboard = on_keyboard_event(keyboard, Event::WillShow, resigned_text_client());
    keyboard.acquired();
    keyboard = on_keyboard_event(keyboard, Event::DidShow, text_client());
    keyboard = on_keyboard_event(keyboard, Event::DidHide, text_client());
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert_eq!(keyboard.visibility, Visibility::Visible);
    assert!(!keyboard.minimized);
}

#[test]
fn never_shown_hardware_input_does_not_supply_minimize_evidence() {
    let mut keyboard = Keyboard::default();
    for _ in 0..3 {
        keyboard.show();
        keyboard.acquired();
        assert_eq!(keyboard.visibility, Visibility::Hidden);
        assert!(!keyboard.minimized);
        assert_eq!(keyboard.dismissal, Dismissal::None);
    }
}

#[test]
fn a_captured_restart_reason_survives_synchronous_reacquisition() {
    let mut keyboard = visible();
    keyboard.expect_hide(ResignReason::Restart);
    let resign = active_responder(keyboard);
    // UIKit can reacquire and send a show before the outer resign returns.
    keyboard.acquired();
    keyboard = on_keyboard_event(keyboard, Event::WillShow, text_client());
    keyboard.resigned(resign);
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert!(!keyboard.minimized);
}

#[test]
fn application_hide_cannot_erase_a_user_dismissal_through_focus_changes() {
    let mut keyboard = visible();
    keyboard.resigned(active_responder(keyboard));
    assert_eq!(keyboard.dismissal, Dismissal::User);
    keyboard.hide();
    keyboard.focus_changed();
    keyboard.focus_changed();
    assert_eq!(keyboard.dismissal, Dismissal::User);
    keyboard.show();
    assert_eq!(keyboard.dismissal, Dismissal::None);
}

#[test]
fn delayed_intentional_hide_after_acquisition_cannot_record_user_dismissal() {
    for reason in [
        ResignReason::Application,
        ResignReason::FocusChange,
        ResignReason::Restart,
    ] {
        let mut keyboard = visible();
        keyboard.expect_hide(reason);
        let resign = active_responder(keyboard);
        keyboard.resigned(resign);
        // Native acquisition can finish before the old hide notification arrives.
        keyboard.acquired();
        keyboard = on_keyboard_event(keyboard, Event::WillHide, resigned_text_client());
        assert_eq!(keyboard.dismissal, Dismissal::None, "{reason:?}");
        assert!(!keyboard.minimized, "{reason:?}");
        keyboard = on_keyboard_event(keyboard, Event::WillShow, resigned_text_client());
        keyboard = on_keyboard_event(keyboard, Event::DidShow, text_client());
        keyboard = on_keyboard_event(keyboard, Event::DidHide, text_client());
        assert_eq!(keyboard.dismissal, Dismissal::None, "{reason:?}");
        assert_eq!(keyboard.visibility, Visibility::Visible, "{reason:?}");
    }
}

#[test]
fn pending_hide_notification_does_not_mask_a_new_unknown_native_resign() {
    for reason in [ResignReason::Inactive, ResignReason::Restart] {
        let mut keyboard = Keyboard::default();
        keyboard.expect_hide(reason);
        let intentional = active_responder(keyboard);
        keyboard.resigned(intentional);
        keyboard.acquired();
        // A hardware-only client can reacquire without receiving WillShow.
        // The previous hide's provenance must not label a new native resign.
        let unknown = active_responder(keyboard);
        assert!(unknown.reason.is_none(), "{reason:?}");
        keyboard.resigned(unknown);
        assert_eq!(keyboard.dismissal, Dismissal::User, "{reason:?}");
        assert_eq!(keyboard.visibility, Visibility::Hidden, "{reason:?}");
        assert!(!keyboard.minimized, "{reason:?}");
    }
}
