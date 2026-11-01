//! The app's layout direction (`crate::layout_direction`) on each window's
//! views, so UIKit lays out what it draws around them (the keyboard's
//! assistant items, menus, the text input's own UI) the same way.

use objc2::MainThreadMarker;

use super::{scene, state::Window};
use crate::layout_direction::LayoutDirection;

impl Window {
    /// Forces `direction` on the drawing view and the text input view. A
    /// window takes the current direction when it attaches to a scene, and
    /// open windows when it changes.
    pub(super) fn apply_layout_direction(&self, direction: LayoutDirection) {
        let attribute = direction.semantic_content_attribute();
        self.view.setSemanticContentAttribute(attribute);
        self.text_input
            .view()
            .setSemanticContentAttribute(attribute);
    }
}

/// The app's layout direction changed: windows in a scene follow now, and
/// windows waiting for one when they attach.
pub(crate) fn layout_direction_changed(direction: LayoutDirection, mtm: MainThreadMarker) {
    for window in scene::live_windows(mtm) {
        window.apply_layout_direction(direction);
    }
}
