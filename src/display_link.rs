//! Frame pacing through `CADisplayLink` on the main run loop, which replaces
//! macOS's `CVDisplayLink`. The callback runs on the main thread, so the window
//! draws directly from it; ProMotion iPads get up to 120 Hz through
//! `preferredFrameRateRange`.

// Wave 0 skeleton: the runtime stream replaces this.
#![allow(dead_code, clippy::todo)]

use objc2::MainThreadMarker;

pub(crate) struct DisplayLink {}

impl DisplayLink {
    /// A paused link that calls `callback` on every frame once started.
    pub(crate) fn new(_mtm: MainThreadMarker, _callback: Box<dyn FnMut()>) -> crate::Result<Self> {
        todo!("runtime stream")
    }

    pub(crate) fn start(&mut self) {
        todo!("runtime stream")
    }

    pub(crate) fn stop(&mut self) {
        todo!("runtime stream")
    }
}
