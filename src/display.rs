//! `gpui::PlatformDisplay` over `UIScreen`. An iPad has its built-in screen
//! and, with Stage Manager or mirroring, possibly one external screen; GPUI
//! sees each screen a window scene can be on.

// Wave 0 skeleton: the runtime stream replaces this.
#![allow(dead_code, clippy::todo)]

use gpui::DisplayId;

/// Implements `gpui::PlatformDisplay` once the runtime stream lands.
#[derive(Debug)]
pub(crate) struct IosDisplay {}

impl IosDisplay {
    /// The screen the app launched on.
    pub(crate) fn primary() -> Self {
        todo!("runtime stream")
    }

    /// Every screen a scene can currently be on, primary first.
    pub(crate) fn all() -> Vec<Self> {
        todo!("runtime stream")
    }

    pub(crate) fn find_by_id(_id: DisplayId) -> Option<Self> {
        todo!("runtime stream")
    }
}
