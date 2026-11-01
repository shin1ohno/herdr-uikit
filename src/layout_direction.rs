//! The app's layout direction, which UIKit cannot infer: GPUI lays out every
//! pixel itself, so the views UIKit sees carry no direction of their own.
//! The app sets it for its language ([`set_layout_direction`]); UIKit then
//! reports the matching base writing direction to the keyboard and IME,
//! mirrors its own UI around the windows, and the text input view maps the
//! visual directions UIKit asks about (left and right) onto the text's
//! logical order (towards its start or end).
//!
//! It never touches terminal rendering or text shaping: those stay GPUI's.
//! So the mapping assumes the focused text is drawn in the app's direction.
//! Text drawn the other way, such as Latin text or a terminal in a
//! right-to-left app, still runs left to right on screen, and there a caret
//! UIKit moves left (the space-bar trackpad, an IME) goes towards the end
//! instead of visually left, and `baseWritingDirectionForPosition:` reports
//! right to left for it. GPUI's input handler reports no per-range
//! direction to do better with; this, and whether UIKit's string tokenizer
//! applies the base writing direction to left and right itself before
//! calling back (which would swap them twice), have to be checked on a
//! device in a right-to-left app (README, "Layout direction").

use std::cell::Cell;

use objc2::MainThreadMarker;
use objc2_ui_kit::{NSWritingDirection, UISemanticContentAttribute, UITextLayoutDirection};

thread_local! {
    /// Only the main thread's copy is ever set or read for UIKit.
    static DIRECTION: Cell<LayoutDirection> = const { Cell::new(LayoutDirection::LeftToRight) };
}

/// Which way the app's text and layout run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    #[default]
    LeftToRight,
    RightToLeft,
}

/// Sets the app's layout direction, on the main thread. Open windows and
/// their text input follow at once. The main menu is rebuilt, which only
/// refreshes its commands: the menu bar's own direction follows the app's
/// localization, not this setting.
///
/// # Errors
///
/// [`crate::Error::NotMainThread`] when called on another thread, where
/// nothing changes.
pub fn set_layout_direction(direction: LayoutDirection) -> crate::Result<()> {
    let mtm = MainThreadMarker::new().ok_or(crate::Error::NotMainThread)?;
    if DIRECTION.replace(direction) == direction {
        return Ok(());
    }
    crate::window::layout_direction_changed(direction, mtm);
    crate::platform::menus::request_rebuild(mtm);
    Ok(())
}

/// The layout direction last set on the main thread. Other threads always
/// see [`LayoutDirection::LeftToRight`].
pub fn layout_direction() -> LayoutDirection {
    DIRECTION.get()
}

/// The direction UIKit's calls on the main thread should follow.
pub(crate) fn current(_mtm: MainThreadMarker) -> LayoutDirection {
    DIRECTION.get()
}

impl LayoutDirection {
    /// The base writing direction `UITextInput` reports.
    pub(crate) fn writing_direction(self) -> NSWritingDirection {
        match self {
            Self::LeftToRight => NSWritingDirection::LeftToRight,
            Self::RightToLeft => NSWritingDirection::RightToLeft,
        }
    }

    /// Forced on the app's views, since UIKit cannot see GPUI's content to
    /// decide for itself.
    pub(crate) fn semantic_content_attribute(self) -> UISemanticContentAttribute {
        match self {
            Self::LeftToRight => UISemanticContentAttribute::ForceLeftToRight,
            Self::RightToLeft => UISemanticContentAttribute::ForceRightToLeft,
        }
    }

    /// The logical direction of a visual one: left and right are reading
    /// order (left is towards the start) only left to right, so right to
    /// left swaps them. Up and down are lines, the same either way.
    pub(crate) fn logical(self, direction: UITextLayoutDirection) -> UITextLayoutDirection {
        match (self, direction) {
            (Self::RightToLeft, UITextLayoutDirection::Left) => UITextLayoutDirection::Right,
            (Self::RightToLeft, UITextLayoutDirection::Right) => UITextLayoutDirection::Left,
            _ => direction,
        }
    }
}

#[cfg(test)]
mod tests;
