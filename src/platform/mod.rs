//! `gpui::Platform` for iPadOS: `UIApplicationMain`, the application and
//! scene delegates, the pasteboard, keyboard layout, URL opening, and the app
//! lifecycle and memory-warning callbacks GPUI's core already models.
//!
//! [`gpui::Platform::run`] calls `UIApplicationMain`, which never returns. The
//! application delegate supplies a scene configuration whose delegate class
//! forwards scene events to `crate::window`, and GPUI's launch callback runs
//! once, when the first window scene connects (docs/ios.md).

// Wave 0 skeleton: the platform stream replaces this.
#![allow(dead_code, clippy::todo)]

/// Implements `gpui::Platform` once the platform stream lands.
pub struct IosPlatform {}

impl IosPlatform {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for IosPlatform {
    fn default() -> Self {
        Self::new()
    }
}
