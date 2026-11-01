use objc2_ui_kit::{NSWritingDirection, UISemanticContentAttribute, UITextLayoutDirection};

use super::*;

const ALL: [UITextLayoutDirection; 4] = [
    UITextLayoutDirection::Left,
    UITextLayoutDirection::Right,
    UITextLayoutDirection::Up,
    UITextLayoutDirection::Down,
];

#[test]
fn left_to_right_keeps_visual_directions() {
    for direction in ALL {
        assert_eq!(LayoutDirection::LeftToRight.logical(direction), direction);
    }
}

#[test]
fn right_to_left_swaps_left_and_right_only() {
    let rtl = LayoutDirection::RightToLeft;
    assert_eq!(
        rtl.logical(UITextLayoutDirection::Left),
        UITextLayoutDirection::Right
    );
    assert_eq!(
        rtl.logical(UITextLayoutDirection::Right),
        UITextLayoutDirection::Left
    );
    assert_eq!(
        rtl.logical(UITextLayoutDirection::Up),
        UITextLayoutDirection::Up
    );
    assert_eq!(
        rtl.logical(UITextLayoutDirection::Down),
        UITextLayoutDirection::Down
    );
}

#[test]
fn uikit_values_follow_the_direction() {
    assert_eq!(
        LayoutDirection::LeftToRight.writing_direction(),
        NSWritingDirection::LeftToRight
    );
    assert_eq!(
        LayoutDirection::RightToLeft.writing_direction(),
        NSWritingDirection::RightToLeft
    );
    assert_eq!(
        LayoutDirection::LeftToRight.semantic_content_attribute(),
        UISemanticContentAttribute::ForceLeftToRight
    );
    assert_eq!(
        LayoutDirection::RightToLeft.semantic_content_attribute(),
        UISemanticContentAttribute::ForceRightToLeft
    );
}

#[test]
fn setting_the_direction_off_the_main_thread_is_refused() {
    let result = set_layout_direction(LayoutDirection::RightToLeft);

    assert!(matches!(result, Err(crate::Error::NotMainThread)));
}

#[test]
fn the_default_direction_is_left_to_right() {
    assert_eq!(LayoutDirection::default(), LayoutDirection::LeftToRight);
}
