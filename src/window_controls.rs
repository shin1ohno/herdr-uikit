//! Window controls sampled from each window's drawing view.

use objc2::MainThreadMarker;

/// How far `window`'s top corners must be kept clear past its safe area, per
/// edge, in points: the drawing view's safe area with horizontal corner
/// adaptation, less `safeAreaInsets`, never negative. Edges are physical, as
/// UIKit reports them. With iPadOS 26's window controls showing it covers
/// them (66 pt on the left of a narrow windowed iPadOS 26.5 window); without
/// them UIKit still reports the window's corner allowance (5.5-9.5 pt). On a
/// Mac running the iPad app with its content under the hidden title bar, at
/// least the Mac's 80 pt (in UIKit points at the window's scale) on the
/// traffic lights' side (`crate::mac_titlebar`). Zero
/// for a window in no live scene, and off the main thread.
/// Sampled on UIKit layout callbacks; GPUI redraws the window when it changes.
pub fn window_control_insets(window: gpui::AnyWindowHandle) -> gpui::Edges<gpui::Pixels> {
    let Some(mtm) = MainThreadMarker::new() else {
        return gpui::Edges::default();
    };
    crate::window::window_control_insets(window, mtm).unwrap_or_default()
}

/// Whether the window controls are a Mac's traffic lights over the content's
/// top row: the iPad app running on a Mac ("Designed for iPad") with its
/// content extended under the hidden title bar (`crate::mac_titlebar`). Then
/// any top-row content, not only the sidebar header, must start past
/// `window_control_insets` on the traffic lights' side. Always false on an
/// iPad, and on a Mac whose window kept its title bar.
pub fn traffic_lights_over_content() -> bool {
    crate::mac_titlebar::title_hidden()
}

/// Whether `window` fills its screen: full screen, or an iPadOS 26 window the
/// user maximized. Such a window hides the OS status bar, which iPadOS 26
/// otherwise draws over its top edge (yebisu#221). False on a Mac, for a
/// window in no live scene, and off the main thread. Sampled on UIKit layout
/// callbacks; a change redraws the window as an insets change.
pub fn window_fills_screen(window: gpui::AnyWindowHandle) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    crate::window::window_fills_screen(window, mtm)
}
