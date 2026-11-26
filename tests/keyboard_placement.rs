//! Host tests for where the software keyboard is (std only). The frames are
//! the ones the iPadOS 27.2 Simulator's keyboard frame notification reported
//! on an iPad Pro 11-inch (M5) in portrait, 834 x 1210 points (yebisu#229
//! spike runs 1-4).
#[path = "../src/window/keyboard_placement.rs"]
mod keyboard_placement;

use keyboard_placement::{KeyboardPlacement, TOLERANCE, placement};

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
