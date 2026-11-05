use super::*;
use crate::window::text_input::keymap;

#[gpui::test]
fn return_insertion_commits_before_acknowledging_a_forwarded_key(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Terminal::default());
    let mut handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), view.clone())),
        )
    });
    for text in ["\r", "\n"] {
        let key = keymap::control_keystroke(text).unwrap_or_else(|| panic!("return is a key"));
        for forwarded in [false, true] {
            let count = view.read_with(cx, |view, _| view.commits.len());
            handler.replace_and_mark_text_in_range(None, "日本😀", None);
            let dispatch = editing::control_input(&mut handler, &key, forwarded);
            assert_eq!(
                &view.read_with(cx, |view, _| view.commits.clone())[count..],
                &["日本😀"]
            );
            assert!(
                !dispatch,
                "confirming composition must not also submit the terminal line"
            );
            assert_eq!(handler.marked_text_range(), None);
            assert_eq!(
                editing::control_input(&mut handler, &key, forwarded),
                !forwarded
            );
            assert_eq!(view.read_with(cx, |view, _| view.commits.len()), count + 1);
        }
    }
}

#[gpui::test]
fn escape_insertion_cancels_composition_without_text_or_terminal_escape(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Terminal::default());
    let mut handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), view.clone())),
        )
    });
    let key = keymap::control_keystroke("\u{1b}").unwrap_or_else(|| panic!("escape is a key"));
    for forwarded in [false, true] {
        handler.replace_and_mark_text_in_range(None, "とりけし😀", None);
        assert!(!editing::control_input(&mut handler, &key, forwarded));
        assert_eq!(handler.marked_text_range(), None);
        assert!(view.read_with(cx, |view, _| view.commits.is_empty()));
        assert_eq!(
            editing::control_input(&mut handler, &key, forwarded),
            !forwarded
        );
    }
}
