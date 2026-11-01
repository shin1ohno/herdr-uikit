//! External edits must survive both the window's frame comparison and the
//! text input view's delegate notification comparison.
use super::*;
use state::{Change, TextState};

fn field(text: &str) -> TextState {
    let len = text.encode_utf16().count();
    TextState {
        context: Some(text.to_owned()),
        handler: true,
        len: Some(len),
        marked: None,
        selection: Some((len..len, false)),
    }
}

#[test]
fn changing_text_without_moving_the_caret_notifies_the_input_delegate() {
    let before = field("first");
    let after = field("other");
    assert_eq!(after.len, before.len);
    assert_eq!(after.selection, before.selection);
    assert!(crate::window::text_state::changed(
        Some(&before),
        Some(&after)
    ));
    assert_eq!(after.change_since(&before), Change::Text);
}

#[test]
fn changing_only_the_document_length_is_reported_at_both_boundaries() {
    let before = field("same");
    let mut after = before.clone();
    after.len = Some(before.len.unwrap_or_default() + 1);
    assert!(crate::window::text_state::changed(
        Some(&before),
        Some(&after)
    ));
    assert_eq!(after.change_since(&before), Change::Text);
}

#[test]
fn an_unchanged_frame_does_not_notify_the_input_delegate() {
    let before = field("にほん");
    let after = before.clone();
    assert!(!crate::window::text_state::changed(
        Some(&before),
        Some(&after)
    ));
    assert_eq!(after.change_since(&before), Change::None);
}

#[test]
fn another_field_with_its_caret_at_zero_still_reports_different_text() {
    let mut before = field("first");
    let mut after = field("other");
    before.selection = Some((0..0, false));
    after.selection = Some((0..0, false));
    assert!(crate::window::text_state::changed(
        Some(&before),
        Some(&after)
    ));
    assert_eq!(after.change_since(&before), Change::Text);
}

#[test]
fn moving_the_sampled_context_window_is_only_a_selection_change() {
    let mut before = field("old context");
    before.len = Some(1_000);
    before.selection = Some((100..100, false));
    let mut after = before.clone();
    after.selection = Some((101..101, false));
    after.context = Some("shifted context".to_owned());
    assert_eq!(after.change_since(&before), Change::Selection);
}
