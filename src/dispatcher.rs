//! GCD-backed `gpui::PlatformDispatcher`, as on macOS: the main queue for
//! foreground tasks, global queues by priority for background tasks, and
//! `dispatch_after` for timers. Ported from `gpui-pre-macos`'s dispatcher.

// Wave 0 skeleton: the runtime stream replaces this.
#![allow(dead_code, clippy::todo)]

/// Implements `gpui::PlatformDispatcher` once the runtime stream lands.
pub(crate) struct IosDispatcher {}

impl IosDispatcher {
    pub(crate) fn new() -> Self {
        Self {}
    }
}
