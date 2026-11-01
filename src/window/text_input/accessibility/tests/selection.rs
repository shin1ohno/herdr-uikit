use super::*;

#[test]
fn accessibility_placement_follows_the_active_utf16_selection_edge() {
    for (range, reversed, caret) in [
        (0..0, false, 0..0),
        (4..8, false, 8..8),
        (4..8, true, 4..4),
        (usize::MAX..usize::MAX, true, usize::MAX..usize::MAX),
    ] {
        assert_eq!(selection_caret(UTF16Selection { range, reversed }), caret);
    }
}
