//! Host tests for whether a window fills its screen (std only). The window
//! hides the status bar exactly when `fills_screen` is true
//! (`src/window/root_controller.rs`, sampled in `src/window/layout.rs`).
#[path = "../src/window/screen_fill.rs"]
mod screen_fill;

use screen_fill::{TOLERANCE, fills_screen};

/// An iPad Pro 13-inch (M5) in landscape, in points.
const SCREEN: (f64, f64) = (1376.0, 1032.0);

#[test]
fn a_full_screen_or_maximized_window_fills_its_screen() {
    assert!(fills_screen(SCREEN, SCREEN, false));
    // UIKit's pixel rounding stays within the tolerance.
    assert!(fills_screen(
        (SCREEN.0 - TOLERANCE, SCREEN.1 + TOLERANCE),
        SCREEN,
        false
    ));
}

#[test]
fn a_smaller_window_does_not() {
    // The owner's external-display window (yebisu#202), Split View halves
    // and a window one point short in either direction.
    for window in [
        (1467.0, 750.0),
        (SCREEN.0 / 2.0, SCREEN.1),
        (SCREEN.0, SCREEN.1 - 1.0),
        (SCREEN.0 - 1.0, SCREEN.1),
    ] {
        assert!(!fills_screen(window, SCREEN, false), "{window:?}");
    }
}

#[test]
fn a_mac_keeps_its_window_as_it_is() {
    assert!(!fills_screen(SCREEN, SCREEN, true));
}

#[test]
fn a_window_not_laid_out_yet_does_not_fill_it() {
    for window in [(0.0, 0.0), (f64::NAN, SCREEN.1), (SCREEN.0, f64::INFINITY)] {
        assert!(!fills_screen(window, SCREEN, false), "{window:?}");
        assert!(!fills_screen(SCREEN, window, false), "{window:?}");
    }
}
