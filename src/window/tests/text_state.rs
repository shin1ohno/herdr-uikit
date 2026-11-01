use crate::window::text_state::{TextState, changed};

/// A field with the caret at the end of `text`, composing its last `marked`
/// UTF-16 units if given.
fn caret_after(text: &str, marked: Option<usize>) -> TextState {
    let end = text.encode_utf16().count();
    TextState {
        context: Some(text.to_owned()),
        handler: true,
        len: Some(end),
        marked: marked.map(|length| end - length..end),
        selection: Some((end..end, false)),
    }
}

#[test]
fn an_unchanged_handler_across_frames_is_not_reported() {
    let previous = caret_after("cd ~/src", None);
    let current = caret_after("cd ~/src", None);
    assert!(!changed(Some(&previous), Some(&current)));
}

#[test]
fn idle_frames_during_a_composition_are_not_reported() {
    // A cursor blink or terminal output redraws while the IME composes.
    let previous = caret_after("にほん", Some(3));
    let current = caret_after("にほん", Some(3));
    assert!(!changed(Some(&previous), Some(&current)));
}

#[test]
fn the_imes_own_edits_are_not_reported() {
    let before = caret_after("ls ", None);
    let started = caret_after("ls に", Some(1));
    let extended = caret_after("ls にほ", Some(2));
    assert!(!changed(Some(&before), Some(&started)));
    assert!(!changed(Some(&started), Some(&extended)));
}

#[test]
fn ending_a_composition_is_reported() {
    let previous = caret_after("にほん", Some(3));
    let current = caret_after("日本", None);
    assert!(changed(Some(&previous), Some(&current)));
}

#[test]
fn typing_is_reported() {
    let previous = caret_after("cd ~/sr", None);
    let current = caret_after("cd ~/src", None);
    assert!(changed(Some(&previous), Some(&current)));
}

#[test]
fn a_moved_selection_is_reported() {
    let previous = caret_after("abc", None);
    let mut current = previous.clone();
    current.selection = Some((1..3, true));
    assert!(changed(Some(&previous), Some(&current)));
}

#[test]
fn another_field_with_the_same_selection_is_reported() {
    let previous = caret_after("first", None);
    let current = caret_after("other", None);
    assert!(changed(Some(&previous), Some(&current)));
}

#[test]
fn an_element_taking_text_is_reported_once() {
    let current = caret_after("", None);
    assert!(changed(None, Some(&current)));
    assert!(!changed(Some(&current), Some(&current)));
}

#[test]
fn no_element_taking_text_is_not_reported() {
    let previous = caret_after("abc", None);
    assert!(!changed(Some(&previous), None));
    assert!(!changed(None, None));
}
