//! Core Text shaping and rasterization, ported from `gpui-pre-macos`'s
//! `text_system` and `open_type` (NOTICE.md). Core Text, Core Graphics, and
//! zed-font-kit's Core Text loader are the same on iOS as on macOS; what
//! differs is the system font set and that AppKit types are unavailable.

// Wave 0 skeleton: the text stream replaces this with the real system.
#![allow(dead_code, clippy::todo)]

/// Implements `gpui::PlatformTextSystem` once the text stream lands.
pub(crate) struct IosTextSystem {}

impl IosTextSystem {
    pub(crate) fn new() -> Self {
        Self {}
    }
}
