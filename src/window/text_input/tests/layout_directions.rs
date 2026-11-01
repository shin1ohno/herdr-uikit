use objc2_ui_kit::UITextLayoutDirection;

use super::*;
use crate::layout_direction::LayoutDirection;

/// What `positionWithinRange:farthestInDirection:` answers for a visual
/// direction under a layout direction.
fn farthest(layout: LayoutDirection, direction: UITextLayoutDirection) -> usize {
    positions::farthest(2..6, layout.logical(direction))
}

#[test]
fn farthest_left_is_the_start_in_left_to_right_text() {
    let ltr = LayoutDirection::LeftToRight;
    assert_eq!(farthest(ltr, UITextLayoutDirection::Left), 2);
    assert_eq!(farthest(ltr, UITextLayoutDirection::Right), 6);
    assert_eq!(farthest(ltr, UITextLayoutDirection::Up), 2);
    assert_eq!(farthest(ltr, UITextLayoutDirection::Down), 6);
}

#[test]
fn farthest_left_is_the_end_in_right_to_left_text() {
    let rtl = LayoutDirection::RightToLeft;
    assert_eq!(farthest(rtl, UITextLayoutDirection::Left), 6);
    assert_eq!(farthest(rtl, UITextLayoutDirection::Right), 2);
    assert_eq!(farthest(rtl, UITextLayoutDirection::Up), 2);
    assert_eq!(farthest(rtl, UITextLayoutDirection::Down), 6);
}
