//! Per-window standard edit availability for non-document consumers.

use crate::NativeEditAction;
use objc2::MainThreadMarker;

/// Explicit availability for a focused consumer whose selection is not the
/// UITextInput document (for example, a terminal's rendered cell selection).
/// Update after focus/selection changes; use `None` to restore SDK defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeEditAvailability {
    pub copy: bool,
    pub cut: bool,
    pub paste: bool,
    pub select_all: bool,
}

impl NativeEditAvailability {
    pub(crate) fn allows(self, action: NativeEditAction) -> bool {
        match action {
            NativeEditAction::Copy => self.copy,
            NativeEditAction::Cut => self.cut,
            NativeEditAction::Paste => self.paste,
            NativeEditAction::SelectAll => self.select_all,
        }
    }
}

/// Publishes one window's edit policy without borrowing GPUI from UIKit's
/// validation callbacks. Both standard menu surfaces and execution consult it.
/// Returns false off the main thread or if the window no longer exists.
/// Availability never gives an inactive window or another responder focus.
pub fn set_native_edit_availability(
    window: gpui::AnyWindowHandle,
    availability: Option<NativeEditAvailability>,
) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    crate::window::set_native_edit_availability(window, availability, mtm)
}
