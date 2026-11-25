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

#[test]
fn the_clearance_is_the_macs_80_pt_in_uikit_points() {
    // 1495 Mac points across a 1942-point UIKit window (macOS 26.7.3).
    let scale = window_scale(1495.0, 1942.0);
    assert!((scale - 0.7698).abs() < 1e-4, "{scale}");
    assert!(
        (clearance(scale) - 103.92).abs() < 0.01,
        "{}",
        clearance(scale)
    );
    assert_eq!(clearance(1.0), TRAFFIC_LIGHT_CLEARANCE);
}

#[test]
fn an_unmeasured_scale_keeps_the_macs_own_clearance() {
    for (mac, uikit) in [
        (0.0, 1942.0),
        (1495.0, 0.0),
        (f64::NAN, 1942.0),
        (-1.0, 1942.0),
    ] {
        assert_eq!(window_scale(mac, uikit), 1.0, "{mac} / {uikit}");
    }
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(clearance(scale), TRAFFIC_LIGHT_CLEARANCE, "{scale}");
    }
}

#[test]
fn content_under_the_title_bar_fills_its_band() {
    // UIKit reports 61 pt of top safe area once the content extends under
    // the title bar; 20 pt before.
    assert_eq!(content_top_inset(true, 61.0), 0.0);
}

#[test]
fn a_shown_title_keeps_the_reported_top_safe_area() {
    for top in [0.0, 20.0, 24.0, 61.0] {
        assert_eq!(content_top_inset(false, top), top);
    }
}

#[test]
fn the_simulator_reports_no_traffic_lights_over_its_content() {
    assert!(!crate::traffic_lights_over_content());
}
