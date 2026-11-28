//! Keyboard occlusion in one shared coordinate space. UIKit notifications
//! use screen coordinates; the layout guide uses the drawing view's own.

use objc2_core_foundation::{CGFloat, CGRect};

#[cfg(test)]
mod tests;

/// Floating keyboards and full-width undocked keyboards can overlap a small
/// window's bottom without being docked to the physical screen. Only screen-
/// docked geometry, or a full-width undocked keyboard still near the screen's
/// bottom edge (split, or just unsplit: `keyboard_inset::near_bottom`),
/// becomes a bottom inset; a keyboard moved up or floating leaves the window
/// as it is. An unrelated notification has no target; it must not reset a
/// window-local guide inset.
pub(super) fn screen_inset(
    view: CGRect,
    screen: CGRect,
    keyboard: CGRect,
    relevant: bool,
) -> Option<CGFloat> {
    if !relevant {
        return None;
    }
    let screen_right = screen.origin.x + screen.size.width;
    let screen_bottom = screen.origin.y + screen.size.height;
    let spans_screen = keyboard.origin.x <= screen.origin.x + 1.0
        && keyboard.origin.x + keyboard.size.width >= screen_right - 1.0;
    let reaches_bottom = keyboard.origin.y + keyboard.size.height >= screen_bottom - 1.0;
    if crate::window::keyboard_inset::near_bottom(tuple(screen), tuple(keyboard)) {
        return Some(near_bottom_inset(view, keyboard));
    }
    if !spans_screen || !reaches_bottom {
        return Some(0.0);
    }
    Some(bottom_inset(view, keyboard))
}

pub(super) fn bottom_inset(view: CGRect, keyboard: CGRect) -> CGFloat {
    super::convert::keyboard_overlap(view, keyboard)
}

/// How much of `view` a full-width undocked keyboard near the screen's
/// bottom covers (`keyboard_inset::near_bottom_inset`), both in the same
/// coordinate space.
pub(super) fn near_bottom_inset(view: CGRect, keyboard: CGRect) -> CGFloat {
    crate::window::keyboard_inset::near_bottom_inset(tuple(view), tuple(keyboard))
}

/// `rect` as (x, y, width, height), the form `keyboard_placement` takes.
pub(super) fn tuple(rect: CGRect) -> (f64, f64, f64, f64) {
    (
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
}
