use super::*;
use gpui::px;

mod detached_view;

fn edges(left: f32, right: f32) -> Edges<Pixels> {
    Edges {
        left: px(left),
        right: px(right),
        ..Default::default()
    }
}

#[test]
fn window_controls_changing_alone_update_the_window() {
    let safe = Edges {
        top: px(24.),
        ..Default::default()
    };
    // Measured on iPadOS 26.5: full width, then a narrow windowed window.
    assert!(needs_update(
        (&safe, &edges(5.5, 5.5)),
        (&safe, &edges(66., 9.5))
    ));
    assert!(needs_update(
        (&safe, &edges(66., 9.5)),
        (&safe, &edges(5.5, 5.5))
    ));
}

#[test]
fn a_repeated_sample_does_not_update_the_window() {
    let safe = Edges {
        top: px(24.),
        ..Default::default()
    };
    assert!(!needs_update(
        (&safe, &edges(66., 9.5)),
        (&safe, &edges(66., 9.5))
    ));
}

#[test]
fn a_safe_area_change_updates_the_window() {
    let before = Edges::default();
    let after = Edges {
        bottom: px(20.),
        ..Default::default()
    };
    assert!(needs_update(
        (&before, &edges(0., 0.)),
        (&after, &edges(0., 0.))
    ));
}

fn insets(top: f64, left: f64, bottom: f64, right: f64) -> UIEdgeInsets {
    UIEdgeInsets {
        top,
        left,
        bottom,
        right,
    }
}

#[test]
fn window_controls_extend_past_the_safe_area_on_the_left() {
    assert_eq!(
        beyond(insets(24.0, 74.0, 20.0, 0.0), insets(24.0, 0.0, 20.0, 0.0)),
        insets(0.0, 74.0, 0.0, 0.0),
    );
}

#[test]
fn equal_region_and_safe_area_leave_no_extra_insets() {
    let safe = insets(24.0, 0.0, 20.0, 0.0);
    assert_eq!(beyond(safe, safe), insets(0.0, 0.0, 0.0, 0.0));
}

#[test]
fn extra_insets_are_never_negative() {
    assert_eq!(
        beyond(insets(0.0, 0.0, 0.0, 0.0), insets(0.0, 20.0, 0.0, 0.0)),
        insets(0.0, 0.0, 0.0, 0.0),
    );
}

#[test]
fn window_controls_preserve_the_physical_right_edge() {
    assert_eq!(
        beyond(insets(0.0, 0.0, 0.0, 74.0), insets(0.0, 0.0, 0.0, 0.0)),
        insets(0.0, 0.0, 0.0, 74.0),
    );
}
