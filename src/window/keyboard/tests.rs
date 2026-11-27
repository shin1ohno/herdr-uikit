use super::*;

#[path = "tests/dismissal.rs"]
mod dismissal;
#[path = "tests/input_mode.rs"]
mod input_mode;
#[path = "tests/notifications.rs"]
mod notifications;
#[path = "tests/visibility.rs"]
mod visibility;

fn text_client() -> Context {
    Context {
        active: true,
        first_responder: true,
        key_window: true,
        local: true,
        screen_matches: Some(true),
        text_requested: true,
        user_dismissable: true,
    }
}

fn visible() -> Keyboard {
    Keyboard {
        visibility: Visibility::Visible,
        ..Keyboard::default()
    }
}
