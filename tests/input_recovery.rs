//! Host tests for the Mac hardware-keyboard input loss: an unrequested
//! keyboard hide on a Mac must not hold the drawing view as first responder
//! (fix 1), a printable press from the drawing view takes the text input view
//! back only while GPUI takes text (fix 2), and the diagnostic codes are a
//! closed set of static names. Both modules are std only.
// The window uses parts of these modules this test does not.
#[allow(dead_code)]
#[path = "../src/window/input_diagnostics.rs"]
mod input_diagnostics;
#[allow(dead_code)]
#[path = "../src/window/keyboard.rs"]
mod keyboard;

use input_diagnostics::{
    CODES, Dedup, DismissalCause, DrawingCause, Press, Recovery, Responder, dismissal_code,
    is_printable, text_focus_code,
};
use keyboard::{Context, Dismissal, Event, Keyboard, Visibility, on_keyboard_event};

fn text_client(user_dismissable: bool) -> Context {
    Context {
        active: true,
        first_responder: true,
        key_window: true,
        local: true,
        screen_matches: Some(true),
        text_requested: true,
        user_dismissable,
    }
}

fn visible() -> Keyboard {
    let mut keyboard = Keyboard::default();
    keyboard.visibility = Visibility::Visible;
    keyboard
}

#[test]
fn a_mac_keyboard_hide_keeps_the_text_input_view_first_responder() {
    // UIKit posts zero-height keyboard notifications on a Mac, including
    // WillHide while our text client is first responder.
    let keyboard = on_keyboard_event(visible(), Event::WillHide, text_client(false));
    assert_eq!(keyboard.dismissal, Dismissal::None);
    assert_eq!(
        Responder::choose(true, true, keyboard.dismissal),
        Responder::Text
    );
}

#[test]
fn an_ipad_keyboard_hide_is_still_the_users_dismissal() {
    let keyboard = on_keyboard_event(visible(), Event::WillHide, text_client(true));
    assert_eq!(keyboard.dismissal, Dismissal::User);
    assert!(keyboard.minimized);
    assert_eq!(
        Responder::choose(true, true, keyboard.dismissal),
        Responder::Drawing(DrawingCause::UserDismissed)
    );
}

#[test]
fn a_mac_hide_still_records_minimization_and_an_application_hide() {
    let keyboard = on_keyboard_event(visible(), Event::WillHide, text_client(false));
    assert!(keyboard.minimized);
    let mut hidden = visible();
    hidden.hide();
    let hidden = on_keyboard_event(hidden, Event::WillHide, text_client(false));
    assert_eq!(hidden.dismissal, Dismissal::Application);
}

#[test]
fn the_responder_cause_names_the_first_failing_condition() {
    assert_eq!(
        Responder::choose(true, false, Dismissal::User),
        Responder::Drawing(DrawingCause::Inactive)
    );
    assert_eq!(
        Responder::choose(false, true, Dismissal::User),
        Responder::Drawing(DrawingCause::Unfocused)
    );
    assert_eq!(
        Responder::choose(true, true, Dismissal::Application),
        Responder::Drawing(DrawingCause::AppDismissed)
    );
    assert_eq!(
        Responder::choose(true, true, Dismissal::None),
        Responder::Text
    );
}

fn drawing_press() -> Press {
    Press {
        printable: true,
        text_first_responder: false,
        text_input_focused: true,
        is_active: true,
        modal: false,
    }
}

#[test]
fn a_printable_press_from_the_drawing_view_takes_text_back_while_gpui_takes_text() {
    assert_eq!(Recovery::decide(drawing_press()), Recovery::TakeText);
}

#[test]
fn no_recovery_while_no_element_takes_text() {
    // A palette or a settings sheet is open: GPUI reported FocusLost.
    let press = Press {
        text_input_focused: false,
        ..drawing_press()
    };
    assert_eq!(Recovery::decide(press), Recovery::NoTextHandler);
}

#[test]
fn no_recovery_when_nothing_is_lost() {
    for press in [
        Press {
            text_first_responder: true,
            ..drawing_press()
        },
        Press {
            printable: false,
            ..drawing_press()
        },
        Press {
            is_active: false,
            ..drawing_press()
        },
        Press {
            modal: true,
            ..drawing_press()
        },
    ] {
        assert_eq!(Recovery::decide(press), Recovery::None, "{press:?}");
    }
}

#[test]
fn printable_means_text_without_command_or_control() {
    assert!(is_printable(Some("a"), false, false));
    assert!(is_printable(Some("あ"), false, false));
    assert!(!is_printable(Some("a"), true, false));
    assert!(!is_printable(Some("a"), false, true));
    assert!(!is_printable(Some("\u{1b}"), false, false));
    assert!(!is_printable(Some("\r"), false, false));
    assert!(!is_printable(Some(""), false, false));
    assert!(!is_printable(None, false, false));
}

#[test]
fn codes_repeat_only_when_they_change_what_a_reader_learns() {
    let mut dedup = Dedup::default();
    let unfocused = Responder::Drawing(DrawingCause::Unfocused);
    assert_eq!(
        dedup.responder(unfocused),
        Some("responder_drawing_unfocused")
    );
    assert_eq!(dedup.responder(unfocused), None);
    assert_eq!(
        dedup.recovery(Recovery::NoTextHandler),
        Some("press_dropped_no_text")
    );
    assert_eq!(dedup.recovery(Recovery::NoTextHandler), None);
    assert_eq!(
        dedup.recovery(Recovery::TakeText),
        Some("press_recovered_text")
    );
    assert_eq!(dedup.recovery(Recovery::None), None);
    assert_eq!(dedup.responder(Responder::Text), Some("responder_text"));
    assert_eq!(
        dedup.responder(unfocused),
        Some("responder_drawing_unfocused")
    );
    assert_eq!(
        dedup.recovery(Recovery::NoTextHandler),
        Some("press_dropped_no_text")
    );
}

#[test]
fn dismissal_transitions_name_their_cause() {
    use Dismissal::{Application, None as Open, User};
    assert_eq!(dismissal_code(Open, Open, DismissalCause::Show), None);
    assert_eq!(
        dismissal_code(Open, User, DismissalCause::WillHide),
        Some("dismissal_user_will_hide")
    );
    assert_eq!(
        dismissal_code(Open, User, DismissalCause::Resign),
        Some("dismissal_user_resign")
    );
    assert_eq!(
        dismissal_code(Open, Application, DismissalCause::Hide),
        Some("dismissal_application")
    );
    assert_eq!(
        dismissal_code(User, Open, DismissalCause::Press),
        Some("dismissal_none_press")
    );
    assert_eq!(
        dismissal_code(Application, Open, DismissalCause::FocusChange),
        Some("dismissal_none_focus")
    );
    assert_eq!(
        dismissal_code(User, Open, DismissalCause::DidShow),
        Some("dismissal_none_did_show")
    );
    assert_eq!(
        dismissal_code(User, Open, DismissalCause::Show),
        Some("dismissal_none_show")
    );
}

/// Every code the decisions can produce, so the list cannot drift from them.
fn produced() -> Vec<&'static str> {
    use Dismissal::{Application, None as Open, User};
    let mut codes = vec![
        text_focus_code(true),
        text_focus_code(false),
        "keyboard_will_hide",
    ];
    for focused in [false, true] {
        for active in [false, true] {
            for dismissal in [Open, User, Application] {
                codes.push(Responder::choose(focused, active, dismissal).code());
            }
        }
    }
    for recovery in [Recovery::TakeText, Recovery::NoTextHandler] {
        codes.extend(Dedup::default().recovery(recovery));
    }
    let causes = [
        DismissalCause::DidShow,
        DismissalCause::FocusChange,
        DismissalCause::Hide,
        DismissalCause::Press,
        DismissalCause::Resign,
        DismissalCause::Show,
        DismissalCause::WillHide,
    ];
    for before in [Open, User, Application] {
        for after in [Open, User, Application] {
            for cause in causes {
                codes.extend(dismissal_code(before, after, cause));
            }
        }
    }
    codes.sort_unstable();
    codes.dedup();
    codes
}

#[test]
fn the_code_list_is_exactly_what_the_decisions_produce() {
    assert_eq!(produced(), CODES.to_vec());
}

#[test]
fn codes_are_static_names_without_payload() {
    for code in CODES {
        assert!(
            !code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{code}"
        );
    }
    assert_eq!(input_diagnostics::TARGET, "herdr_uikit::diagnostics");
}
