use super::*;

use state::{Change, Snapshot};

fn snapshot(
    selection: Option<std::ops::Range<usize>>,
    marked: Option<std::ops::Range<usize>>,
    len: Option<usize>,
) -> Snapshot {
    Snapshot {
        handler: true,
        selection,
        marked,
        len,
    }
}

#[test]
fn identical_state_needs_no_notification() {
    let state = snapshot(Some(3..3), None, Some(10));
    assert_eq!(state.change_since(&state.clone()), Change::None);
}

#[test]
fn a_moved_caret_is_a_selection_change() {
    let old = snapshot(Some(3..3), None, Some(10));
    let new = snapshot(Some(4..4), None, Some(10));
    assert_eq!(new.change_since(&old), Change::Selection);
}

#[test]
fn edits_and_compositions_are_text_changes() {
    let old = snapshot(Some(3..3), None, Some(10));
    let longer = snapshot(Some(4..4), None, Some(11));
    assert_eq!(longer.change_since(&old), Change::Text);
    let composing = snapshot(Some(3..3), Some(2..3), Some(10));
    assert_eq!(composing.change_since(&old), Change::Text);
}

#[test]
fn gaining_or_losing_the_handler_is_a_text_change() {
    let focused = snapshot(Some(0..0), None, None);
    let unfocused = Snapshot::default();
    assert_eq!(unfocused.change_since(&focused), Change::Text);
    assert_eq!(focused.change_since(&unfocused), Change::Text);
}

#[test]
fn document_length_falls_back_to_the_furthest_offset() {
    assert_eq!(snapshot(Some(1..2), None, Some(7)).document_len(), 7);
    // A terminal reports only its composition.
    assert_eq!(snapshot(Some(1..1), Some(0..3), None).document_len(), 3);
    assert_eq!(snapshot(Some(4..5), Some(0..2), None).document_len(), 5);
    assert_eq!(Snapshot::default().document_len(), 0);
}
