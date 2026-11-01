use super::*;

fn unrelated_contexts() -> impl Iterator<Item = Context> {
    [
        Context {
            local: false,
            ..text_client()
        },
        Context {
            key_window: false,
            ..text_client()
        },
        Context {
            active: false,
            ..text_client()
        },
        Context {
            first_responder: false,
            text_requested: false,
            ..text_client()
        },
        Context {
            screen_matches: Some(false),
            ..text_client()
        },
    ]
    .into_iter()
}

#[test]
fn a_matching_screen_also_requires_local_active_key_window_text_ownership() {
    assert!(text_client().is_relevant());
    for context in unrelated_contexts() {
        assert!(!context.is_relevant(), "{context:?}");
    }
}

#[test]
fn text_requests_identify_our_client_before_become_first_responder_returns() {
    assert!(
        Context {
            first_responder: false,
            ..text_client()
        }
        .is_relevant()
    );
    assert!(
        Context {
            text_requested: false,
            ..text_client()
        }
        .is_relevant()
    );
}

#[test]
fn an_unscoped_frame_still_requires_our_local_active_text_client() {
    assert!(
        Context {
            screen_matches: None,
            ..text_client()
        }
        .is_relevant()
    );
    for context in unrelated_contexts().filter(|context| context.screen_matches != Some(false)) {
        assert!(
            !Context {
                screen_matches: None,
                ..context
            }
            .is_relevant()
        );
    }
}

#[test]
fn foreign_app_and_sibling_window_events_preserve_all_keyboard_state() {
    let minimized = on_keyboard_event(visible(), Event::WillHide, text_client());
    let missing_screen = Context {
        screen_matches: None,
        ..text_client()
    };
    for context in unrelated_contexts().chain([missing_screen]) {
        for keyboard in [visible(), minimized] {
            for event in [
                Event::WillShow,
                Event::DidShow,
                Event::WillHide,
                Event::DidHide,
            ] {
                assert_eq!(on_keyboard_event(keyboard, event, context), keyboard);
            }
        }
    }
}
