use super::*;

#[test]
fn a_never_shown_keyboard_has_no_minimize_evidence() {
    let keyboard = Keyboard::default();
    assert_eq!(keyboard.visibility, Visibility::default());
    assert!(!keyboard.minimized);
}

#[test]
fn minimizing_requires_positive_evidence_and_our_native_show_releases_the_dismissal() {
    let keyboard = on_keyboard_event(visible(), Event::WillHide, text_client());
    assert_eq!(keyboard.dismissal, Dismissal::User);
    assert!(keyboard.minimized);
    let keyboard = on_keyboard_event(keyboard, Event::DidHide, text_client());
    assert!(keyboard.minimized);
    assert_eq!(keyboard.visibility, Visibility::Hidden);

    for event in [Event::WillShow, Event::DidShow] {
        let shown = on_keyboard_event(keyboard, event, text_client());
        assert!(!shown.minimized);
        assert_eq!(
            shown.dismissal,
            if event == Event::DidShow {
                Dismissal::None
            } else {
                Dismissal::User
            }
        );
    }
}

#[test]
fn a_hide_after_an_intentional_resignation_is_not_a_user_dismissal_or_minimize() {
    let mut keyboard = on_keyboard_event(visible(), Event::WillHide, text_client());
    keyboard.show();
    keyboard.expect_hide(ResignReason::Restart);
    keyboard.resigned(keyboard.resign_context(true, true, true));
    assert!(!keyboard.minimized);
    let context = Context {
        first_responder: false,
        ..text_client()
    };
    let keyboard = on_keyboard_event(keyboard, Event::WillHide, context);
    let keyboard = on_keyboard_event(keyboard, Event::DidHide, context);
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert!(!keyboard.minimized);
}

#[test]
fn restart_resign_hide_become_show_fences_the_old_hide_completion() {
    let mut keyboard = on_keyboard_event(visible(), Event::WillHide, text_client());
    keyboard.show();
    keyboard.expect_hide(ResignReason::Restart);
    keyboard.resigned(keyboard.resign_context(true, true, true));
    keyboard = on_keyboard_event(
        keyboard,
        Event::WillHide,
        Context {
            first_responder: false,
            ..text_client()
        },
    );
    keyboard = on_keyboard_event(keyboard, Event::WillShow, text_client());
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert!(!keyboard.minimized);
    let showing = keyboard;
    keyboard = on_keyboard_event(keyboard, Event::DidHide, text_client());
    assert_eq!(keyboard, showing);
    keyboard = on_keyboard_event(keyboard, Event::DidShow, text_client());
    let shown = keyboard;
    keyboard = on_keyboard_event(keyboard, Event::DidHide, text_client());
    assert_eq!(keyboard, shown);
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert_eq!(keyboard.visibility, Visibility::Visible);
}

#[test]
fn an_app_hide_preserves_application_dismissal() {
    let mut keyboard = visible();
    keyboard.hide();
    let keyboard = on_keyboard_event(keyboard, Event::WillHide, text_client());
    let keyboard = on_keyboard_event(keyboard, Event::DidHide, text_client());
    assert_eq!(keyboard.dismissal, Dismissal::Application);
    assert!(!keyboard.dismissal.allows_text());
}

#[test]
fn did_hide_alone_is_not_positive_minimize_evidence() {
    let keyboard = Keyboard {
        visibility: Visibility::Hiding,
        ..Keyboard::default()
    };
    let keyboard = on_keyboard_event(keyboard, Event::DidHide, text_client());
    assert!(!keyboard.minimized);
}

#[test]
fn losing_text_focus_before_resigning_is_not_a_user_dismissal() {
    let context = Context {
        text_requested: false,
        ..text_client()
    };
    let keyboard = on_keyboard_event(visible(), Event::WillHide, context);
    assert_eq!(keyboard.dismissal, Dismissal::None);
}

#[test]
fn stale_completions_cannot_reverse_a_newer_keyboard_transition() {
    let mut visibility = Visibility::Visible;
    visibility.observe(Event::WillHide);
    visibility.observe(Event::WillShow);
    assert!(!visibility.observe(Event::DidHide));
    assert_eq!(visibility, Visibility::Showing);
    visibility.observe(Event::DidShow);
    assert!(!visibility.observe(Event::DidHide));
    assert_eq!(visibility, Visibility::Visible);

    visibility.observe(Event::WillHide);
    assert!(!visibility.observe(Event::DidShow));
    assert_eq!(visibility, Visibility::Hiding);
}
