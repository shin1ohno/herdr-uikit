use objc2_ui_kit::UIEdgeInsets;

use super::*;

fn insets(left: f64, right: f64) -> UIEdgeInsets {
    UIEdgeInsets {
        top: 0.0,
        left,
        bottom: 0.0,
        right,
    }
}

fn pair(insets: UIEdgeInsets) -> (f64, f64, f64, f64) {
    (insets.top, insets.left, insets.bottom, insets.right)
}

#[test]
fn only_a_mac_hides_the_title() {
    assert!(hides_title(true));
    assert!(!hides_title(false));
}

#[test]
fn the_simulator_keeps_its_title() {
    assert!(!ios_app_on_mac());
    assert!(!title_hidden());
}

#[test]
fn a_hidden_title_clears_the_traffic_lights_on_the_left() {
    let cleared = clear_traffic_lights(true, LayoutDirection::LeftToRight, insets(9.5, 9.5));

    assert_eq!(pair(cleared), (0.0, TRAFFIC_LIGHT_CLEARANCE, 0.0, 9.5));
}

#[test]
fn a_hidden_title_clears_the_traffic_lights_on_the_right_right_to_left() {
    let cleared = clear_traffic_lights(true, LayoutDirection::RightToLeft, insets(9.5, 9.5));

    assert_eq!(pair(cleared), (0.0, 9.5, 0.0, TRAFFIC_LIGHT_CLEARANCE));
}

#[test]
fn a_wider_reported_reach_wins_over_the_clearance() {
    let cleared = clear_traffic_lights(true, LayoutDirection::LeftToRight, insets(96.0, 0.0));

    assert_eq!(pair(cleared), (0.0, 96.0, 0.0, 0.0));
}

#[test]
fn a_shown_title_leaves_the_sample_alone() {
    let sampled = insets(66.0, 9.5);

    for direction in [LayoutDirection::LeftToRight, LayoutDirection::RightToLeft] {
        assert_eq!(
            pair(clear_traffic_lights(false, direction, sampled)),
            pair(sampled)
        );
    }
}
