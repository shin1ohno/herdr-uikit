//! Window controls sampled from each window's drawing view.

use objc2::MainThreadMarker;

/// How far `window`'s top corners must be kept clear past its safe area, per
/// edge, in points: the drawing view's safe area with horizontal corner
/// adaptation, less `safeAreaInsets`, never negative. Edges are physical, as
/// UIKit reports them. With iPadOS 26's window controls showing it covers
/// them (66 pt on the left of a narrow windowed iPadOS 26.5 window); without
/// them UIKit still reports the window's corner allowance (5.5-9.5 pt). Zero
/// on iOS 17-25, for a window in no live scene, and off the main thread.
/// Sampled on UIKit layout callbacks; GPUI redraws the window when it changes.
pub fn window_control_insets(window: gpui::AnyWindowHandle) -> gpui::Edges<gpui::Pixels> {
    let Some(mtm) = MainThreadMarker::new() else {
        return gpui::Edges::default();
    };
    crate::window::window_control_insets(window, mtm).unwrap_or_default()
}
