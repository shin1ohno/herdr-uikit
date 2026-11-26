//! Whether a window fills its screen: full screen, or an iPadOS 26 window the
//! user maximized, and whether a window hides the OS status bar.
//!
//! #37 hid the status bar while the window filled its screen (owner decision
//! for yebisu#221, 2026-11-26). On iPadOS 27 that took the menu bar's touch
//! path with it: the menu bar opens from the app's name in the status bar,
//! and in full screen no top-edge swipe or tap brought a hidden status bar
//! back (yebisu#242, iOS 27.2 Simulator, 2026-11-27). So the status bar stays
//! (`hides_status_bar`); an app keeps its top row below it through the top
//! safe area, which UIKit reports with the status bar shown. The fill itself
//! is still reported to apps (`window_fills_screen`).
//!
//! `UIWindowScene.isFullScreen` exists only on Mac Catalyst (UIKit
//! `UIWindowScene.h`, iOS 27.2 SDK), so the sizes are compared instead.
//!
//! Plain arithmetic in points, with no UIKit types, so host tests can run it
//! (`tests/screen_fill.rs`).

/// How far a window's size may be from its screen's, in points, and still
/// fill it. UIKit rounds frames to the screen's pixels, which is at most half
/// a point at a scale of 1.
pub const TOLERANCE: f64 = 0.5;

/// Whether a `window` (width, height) fills a `screen` (width, height), both
/// in points in the interface's orientation. Always false on a Mac
/// (`on_mac`), where the iPad app's window has no iPad status bar over it,
/// and for a size that is not finite and positive.
pub fn fills_screen(window: (f64, f64), screen: (f64, f64), on_mac: bool) -> bool {
    let drawable = |(width, height): (f64, f64)| {
        width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
    };
    if on_mac || !drawable(window) || !drawable(screen) {
        return false;
    }
    (window.0 - screen.0).abs() <= TOLERANCE && (window.1 - screen.1).abs() <= TOLERANCE
}

/// Whether a window hides the OS status bar: never, whether it fills its
/// screen or not (yebisu#242 option B). The status bar holds the app's name,
/// the touch path to the iPadOS 27 menu bar.
pub fn hides_status_bar(_fills_screen: bool) -> bool {
    false
}
