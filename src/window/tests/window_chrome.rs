use super::super::layout::clear_window_controls;
use gpui::{Edges, px};

#[test]
fn interactive_content_clears_window_controls_and_retains_other_safe_edges() {
    let safe = Edges {
        top: px(0.),
        left: px(4.),
        bottom: px(20.),
        right: px(8.),
    };
    let corner = Edges {
        top: px(42.),
        left: px(0.),
        bottom: px(0.),
        right: px(0.),
    };
    assert_eq!(
        clear_window_controls(safe, Some(corner)),
        Edges {
            top: px(42.),
            left: px(4.),
            bottom: px(20.),
            right: px(8.)
        }
    );
}

#[test]
fn older_systems_and_corners_smaller_than_the_safe_area_keep_existing_geometry() {
    let safe = Edges {
        top: px(24.),
        left: px(0.),
        bottom: px(20.),
        right: px(0.),
    };
    assert_eq!(clear_window_controls(safe, None), safe);
    assert_eq!(clear_window_controls(safe, Some(Edges::default())), safe);
}
