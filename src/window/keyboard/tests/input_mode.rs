use super::*;

#[test]
fn switching_native_input_modes_does_not_leave_text_disabled_for_the_next_focus_update() {
    let mut keyboard = visible();
    for event in [
        Event::WillHide,
        Event::DidHide,
        Event::WillShow,
        Event::DidShow,
    ] {
        keyboard = on_keyboard_event(keyboard, event, text_client());
    }
    keyboard.focus_changed();
    assert_eq!(keyboard.visibility, Visibility::Visible);
    assert!(!keyboard.minimized);
    assert!(keyboard.dismissal.allows_text());
}

#[test]
fn another_responder_showing_does_not_reopen_a_keyboard_the_user_dismissed() {
    let keyboard = on_keyboard_event(visible(), Event::WillHide, text_client());
    let other = Context {
        first_responder: false,
        ..text_client()
    };
    let keyboard = on_keyboard_event(keyboard, Event::WillShow, other);
    let keyboard = on_keyboard_event(keyboard, Event::DidShow, other);
    assert_eq!(keyboard.dismissal, Dismissal::User);
}

#[test]
fn an_uncompleted_show_and_hardware_only_acquisition_preserve_user_hide() {
    let mut keyboard = visible();
    keyboard.resigned(keyboard.resign_context(true, true, true));
    keyboard.acquired();
    keyboard.focus_changed();
    assert_eq!(keyboard.dismissal, Dismissal::User);
    let showing = on_keyboard_event(keyboard, Event::WillShow, text_client());
    assert_eq!(showing.dismissal, Dismissal::User);
    let hiding = on_keyboard_event(showing, Event::WillHide, text_client());
    let stale = on_keyboard_event(hiding, Event::DidShow, text_client());
    assert_eq!(stale, hiding);
    assert_eq!(stale.dismissal, Dismissal::User);
}

#[test]
fn native_show_does_not_override_an_application_hide() {
    let mut keyboard = visible();
    keyboard.hide();
    keyboard = on_keyboard_event(keyboard, Event::WillShow, text_client());
    keyboard = on_keyboard_event(keyboard, Event::DidShow, text_client());
    assert_eq!(keyboard.dismissal, Dismissal::Application);
    assert!(!keyboard.dismissal.allows_text());
}
