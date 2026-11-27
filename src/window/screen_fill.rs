//! Whether a window fills its screen: full screen, or an iPadOS 26 window the
//! user maximized. Such a window hides the OS status bar (owner decision for
//! yebisu#221, 2026-11-26): in an iPadOS 26 window the status bar is drawn
//! over the window's top edge, so with the window filling the screen its time
//! and date cover the app's top row. A smaller window, one on a Mac and one
//! not yet laid out keep the status bar.
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
