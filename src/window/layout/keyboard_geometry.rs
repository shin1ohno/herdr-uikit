//! Keyboard occlusion in one shared coordinate space. UIKit notifications
//! use screen coordinates; the layout guide uses the drawing view's own.

use objc2_core_foundation::{CGFloat, CGRect};

#[cfg(test)]
mod tests;

/// Floating keyboards and full-width undocked keyboards can overlap a small
/// window's bottom without being docked to the physical screen. Only screen-
/// docked geometry becomes a bottom inset; UIKit's layout guide follows the
/// same policy by leaving followsUndockedKeyboard disabled. An unrelated
/// notification has no target; it must not reset a window-local guide inset.
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
    if !spans_screen || !reaches_bottom {
        return Some(0.0);
    }
    Some(bottom_inset(view, keyboard))
}

pub(super) fn bottom_inset(view: CGRect, keyboard: CGRect) -> CGFloat {
    super::convert::keyboard_overlap(view, keyboard)
}
