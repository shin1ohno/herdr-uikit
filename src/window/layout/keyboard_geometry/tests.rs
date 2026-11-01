use super::*;
use objc2_core_foundation::{CGPoint, CGSize};

mod notification_targets;

fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}

#[test]
fn docked_keyboard_frame_in_screen_space_avoids_the_bottom() {
    let view = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard = rect(0.0, 920.0, 1032.0, 456.0);
    assert_eq!(screen_inset(view, view, keyboard, true), Some(456.0));
}

#[test]
fn stage_manager_view_origin_is_in_the_same_space_as_the_keyboard() {
    let view_in_screen = rect(80.0, 120.0, 850.0, 1000.0);
    let screen = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard_in_screen = rect(0.0, 920.0, 1032.0, 456.0);
    assert_eq!(
        screen_inset(view_in_screen, screen, keyboard_in_screen, true),
        Some(200.0)
    );
    let view_local = rect(0.0, 0.0, 850.0, 1000.0);
    let keyboard_local = rect(-80.0, 800.0, 1032.0, 456.0);
    let screen_local = rect(-80.0, -120.0, 1032.0, 1376.0);
    assert_eq!(
        screen_inset(view_in_screen, screen, keyboard_in_screen, true),
        screen_inset(view_local, screen_local, keyboard_local, true)
    );
}

#[test]
fn floating_and_undocked_keyboards_reserve_no_bottom_space() {
    let view = rect(0.0, 0.0, 1032.0, 1376.0);
    for keyboard in [
        rect(600.0, 700.0, 320.0, 250.0),
        rect(0.0, 700.0, 1032.0, 456.0),
    ] {
        assert_eq!(screen_inset(view, view, keyboard, true), Some(0.0));
    }
}

#[test]
fn a_keyboard_on_another_screen_reserves_no_space() {
    let view = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard = rect(0.0, 920.0, 1032.0, 456.0);
    assert_eq!(screen_inset(view, view, keyboard, false), None);
}

#[test]
fn minimized_hardware_keyboard_assistant_reserves_only_its_height() {
    let view = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard = rect(0.0, 1321.0, 1032.0, 55.0);
    assert_eq!(screen_inset(view, view, keyboard, true), Some(55.0));
}

#[test]
fn floating_keyboard_dragged_against_the_screen_bottom_is_not_docked() {
    let screen = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard = rect(600.0, 1126.0, 320.0, 250.0);
    assert_eq!(screen_inset(screen, screen, keyboard, true), Some(0.0));
}

#[test]
fn undocked_full_width_keyboard_crossing_a_small_window_bottom_is_not_docked() {
    let screen = rect(0.0, 0.0, 1032.0, 1376.0);
    let view_in_screen = rect(80.0, 120.0, 850.0, 800.0);
    let keyboard = rect(0.0, 700.0, 1032.0, 456.0);
    assert_eq!(
        screen_inset(view_in_screen, screen, keyboard, true),
        Some(0.0)
    );
}

#[test]
fn short_full_width_assistant_is_docked_without_reserving_a_full_keyboard() {
    let screen = rect(0.0, 0.0, 1032.0, 1376.0);
    let keyboard = rect(0.0, 1332.0, 1032.0, 44.0);
    assert_eq!(screen_inset(screen, screen, keyboard, true), Some(44.0));
}

#[test]
fn hidden_keyboard_and_empty_layout_guide_reserve_no_space() {
    let view = rect(0.0, 0.0, 1032.0, 1376.0);
    assert_eq!(bottom_inset(view, rect(0.0, 1376.0, 1032.0, 456.0)), 0.0);
    assert_eq!(bottom_inset(view, rect(0.0, 1376.0, 1032.0, 0.0)), 0.0);
}
