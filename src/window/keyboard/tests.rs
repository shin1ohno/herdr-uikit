use super::*;

mod dismissal;
mod input_mode;
mod notifications;
mod visibility;

fn text_client() -> Context {
    Context {
        active: true,
        first_responder: true,
        key_window: true,
        local: true,
        screen_matches: Some(true),
        text_requested: true,
    }
}

fn visible() -> Keyboard {
    Keyboard {
        visibility: Visibility::Visible,
        ..Keyboard::default()
    }
}
