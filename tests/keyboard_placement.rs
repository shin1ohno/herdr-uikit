//! Host tests for where the software keyboard is (std only). The frames are
//! the ones the iPadOS 27.2 Simulator's keyboard frame notification reported
//! on an iPad Pro 11-inch (M5) in portrait, 834 x 1210 points (yebisu#229
//! spike runs 1-4).
#[path = "../src/window/keyboard_placement.rs"]
mod keyboard_placement;

use keyboard_placement::{
    KeyboardPlacement, TOLERANCE, accessory_alone, combine, guide_placement, placement,
};

const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 834.0, 1210.0);

#[test]
fn a_docked_keyboard_spans_the_bottom() {
    // The UITextView keyboard with its shortcuts bar.
    assert_eq!(
        placement(SCREEN, (0.0, 870.0, 834.0, 340.0)),
        KeyboardPlacement::Docked
    );
    // UIKit's rounding stays within the tolerance.
    assert_eq!(
        placement(
            SCREEN,
            (TOLERANCE, 870.0, 834.0 - 2.0 * TOLERANCE, 340.0 - TOLERANCE)
        ),
        KeyboardPlacement::Docked
    );
}

#[test]
fn a_floating_keyboard_is_narrower_than_the_screen() {
    assert_eq!(
        placement(SCREEN, (250.0, 697.5, 334.0, 349.0)),
        KeyboardPlacement::Floating
    );
    // Dragged to the screen's bottom-left corner, it still floats.
    assert_eq!(
        placement(SCREEN, (0.0, 861.0, 334.0, 349.0)),
        KeyboardPlacement::Floating
    );
}

#[test]
fn a_full_width_keyboard_off_the_bottom_is_undocked() {
    assert_eq!(
        placement(SCREEN, (0.0, 500.0, 834.0, 340.0)),
        KeyboardPlacement::Undocked
    );
}

#[test]
fn a_keyboard_off_screen_or_empty_is_hidden() {
    // The hide animation's end frame: just below the screen.
    assert_eq!(
        placement(SCREEN, (0.0, 1210.0, 834.0, 340.0)),
        KeyboardPlacement::Hidden
    );
    assert_eq!(
        placement(SCREEN, (0.0, 0.0, 0.0, 0.0)),
        KeyboardPlacement::Hidden
    );
    assert_eq!(
        placement(SCREEN, (0.0, 870.0, 834.0, 0.0)),
        KeyboardPlacement::Hidden
    );
    assert_eq!(
        placement(SCREEN, (900.0, 700.0, 334.0, 349.0)),
        KeyboardPlacement::Hidden
    );
    assert_eq!(
        placement(SCREEN, (f64::NAN, 870.0, 834.0, 340.0)),
        KeyboardPlacement::Hidden
    );
    assert_eq!(
        placement((0.0, 0.0, 0.0, 0.0), (0.0, 870.0, 834.0, 340.0)),
        KeyboardPlacement::Hidden
    );
}

#[test]
fn the_screen_origin_is_respected() {
    // A screen whose coordinate space starts away from zero.
    let screen = (100.0, 50.0, 834.0, 1210.0);
    assert_eq!(
        placement(screen, (100.0, 920.0, 834.0, 340.0)),
        KeyboardPlacement::Docked
    );
    assert_eq!(
        placement(screen, (0.0, 920.0, 834.0, 340.0)),
        KeyboardPlacement::Floating
    );
}

#[test]
fn nothing_was_reported_yet() {
    assert_eq!(KeyboardPlacement::default(), KeyboardPlacement::Hidden);
}

/// The sequences the iPadOS 27.2 Simulator posted on an iPad Pro 13-inch (M5),
/// 1032 x 1376 points, 20 pt bottom safe area (yebisu#229 spike runs 8 and 9,
/// yebisu keyboard-bar-check run 6): each frame notification's end frame,
/// and the drawing view's undocked-following guide.
#[test]
fn the_guide_keeps_a_floating_keyboard_the_last_notification_drops() {
    let screen = (0.0, 0.0, 1032.0, 1376.0);
    let floating = (349.0, 877.5, 334.0, 335.0);
    let docked = (0.0, 973.0, 1032.0, 403.0);
    let zero_at_bottom = (0.0, 1376.0, 1032.0, 0.0);
    let safe_area = (0.0, 1356.0, 1032.0, 20.0);
    let report = |end, guide| combine(placement(screen, end), guide_placement(screen, guide, 20.0));
    // Docked, then pinched: the floating frame, then a zero-tall one.
    assert_eq!(report(docked, docked), KeyboardPlacement::Docked);
    assert_eq!(report(floating, floating), KeyboardPlacement::Floating);
    assert_eq!(
        report(zero_at_bottom, floating),
        KeyboardPlacement::Floating
    );
    // The floating keyboard resigns: an empty frame, the guide on the safe area.
    assert_eq!(
        report((0.0, 0.0, 0.0, 0.0), safe_area),
        KeyboardPlacement::Hidden
    );
    // A docked keyboard hides.
    assert_eq!(report(zero_at_bottom, safe_area), KeyboardPlacement::Hidden);
    // Pinched back to docked while the guide still shows the float: the
    // notification's real frame wins.
    assert_eq!(report(docked, floating), KeyboardPlacement::Docked);
    // Relaunched docked after a zero-tall last frame (run 6's redock).
    assert_eq!(report(zero_at_bottom, docked), KeyboardPlacement::Docked);
}

#[test]
fn a_guide_no_taller_than_the_safe_area_is_no_keyboard() {
    let screen = (0.0, 0.0, 1032.0, 1376.0);
    let strip = (0.0, 1356.0, 1032.0, 20.0);
    assert_eq!(
        guide_placement(screen, strip, 20.0),
        KeyboardPlacement::Hidden
    );
    assert_eq!(
        guide_placement(screen, strip, 0.0),
        KeyboardPlacement::Docked
    );
    assert_eq!(
        guide_placement(screen, (0.0, 1009.0, 1032.0, 367.0), 20.0),
        KeyboardPlacement::Docked
    );
}

#[test]
fn only_a_hidden_notification_defers_to_the_guide() {
    use KeyboardPlacement::*;
    for notified in [Hidden, Docked, Undocked, Floating] {
        for guide in [Hidden, Docked, Undocked, Floating] {
            let expected = if notified == Hidden { guide } else { notified };
            assert_eq!(combine(notified, guide), expected, "{notified:?} {guide:?}");
        }
    }
}

/// yebisu#276, iPad mini (A20 Pro) portrait, 756 x 1158 points: with the
/// 48 pt accessory row, a floating keyboard's notification ends at the row
/// alone, (0, 1110, 756, 48), while the guide follows the floating keyboard;
/// a docked keyboard's frame holds the row and the keys.
#[test]
fn the_accessory_row_alone_leaves_the_placement_to_the_guide() {
    const MINI: (f64, f64, f64, f64) = (0.0, 0.0, 756.0, 1158.0);
    const ROW: f64 = 48.0;
    let row_alone = (0.0, 1110.0, 756.0, 48.0);
    let docked_with_row = (0.0, 770.0, 756.0, 388.0);
    let floating_guide = (210.0, 657.0, 334.0, 337.5);
    assert!(accessory_alone(MINI, row_alone, ROW));
    assert!(!accessory_alone(MINI, docked_with_row, ROW));
    // Without a row the same frame is a docked bar, as before.
    assert!(!accessory_alone(MINI, row_alone, 0.0));
    // A frame off the bottom edge or narrower is not the row alone.
    assert!(!accessory_alone(MINI, (0.0, 1000.0, 756.0, 48.0), ROW));
    assert!(!accessory_alone(MINI, (100.0, 1110.0, 556.0, 48.0), ROW));
    // The window takes the row alone as Hidden, so the guide decides.
    let guide = guide_placement(MINI, floating_guide, 20.0);
    assert_eq!(guide, KeyboardPlacement::Floating);
    assert_eq!(
        combine(KeyboardPlacement::Hidden, guide),
        KeyboardPlacement::Floating
    );
    assert_eq!(
        combine(placement(MINI, docked_with_row), guide),
        KeyboardPlacement::Docked
    );
}
