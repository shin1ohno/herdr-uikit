//! Frame pacing through `CADisplayLink` on the main run loop, which replaces
//! macOS's `CVDisplayLink`. The callback runs on the main thread, so the window
//! draws directly from it; ProMotion iPads get up to 120 Hz through
//! `preferredFrameRateRange`.
//!
//! A display link calls a selector on a target object, so the callback lives
//! in a small `NSObject` subclass. The link retains its target and the main
//! run loop retains the link; invalidating the link on drop breaks that chain,
//! which frees the target and the callback with it.
//!
//! A link follows one screen's refresh. `new` creates it for the device's own
//! screen, as `+[CADisplayLink displayLinkWithTarget:selector:]` does, and
//! `set_screen` replaces it with one from `-[UIScreen
//! displayLinkWithTarget:selector:]` when the window's scene is on another
//! screen, such as an external monitor under Stage Manager, which refreshes
//! at its own rate.

#![allow(
    unsafe_code,
    reason = "CADisplayLink takes an Objective-C target, a selector, and a run loop"
)]

#[cfg(test)]
mod tests;

use std::{cell::RefCell, ptr};

use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
    rc::Retained, sel,
};
use objc2_foundation::{NSObject, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::{CADisplayLink, CAFrameRateRange};
use objc2_ui_kit::UIScreen;

use crate::Error;

/// The frame rates the link asks for. ProMotion iPads run at up to 120 Hz and
/// others at 60; the system picks the closest rate it can sustain, lower in
/// Low Power Mode or under thermal pressure.
const FRAME_RATE_RANGE: CAFrameRateRange = CAFrameRateRange {
    minimum: 60.,
    maximum: 120.,
    preferred: 120.,
};

// The range must contain its preferred rate.
const _: () = assert!(
    FRAME_RATE_RANGE.minimum <= FRAME_RATE_RANGE.preferred
        && FRAME_RATE_RANGE.preferred <= FRAME_RATE_RANGE.maximum
);

type FrameCallback = Box<dyn FnMut()>;

pub(crate) struct DisplayLink {
    link: Retained<CADisplayLink>,
    /// The screen `link` came from, or `None` for the device's own screen
    /// that `new` starts on.
    screen: Option<Retained<UIScreen>>,
    /// Kept so `set_screen` can hand the same callback to a new link.
    target: Retained<DisplayLinkTarget>,
}

impl DisplayLink {
    /// A paused link that calls `callback` on every frame once started, paced
    /// by the device's own screen until [`set_screen`](Self::set_screen)
    /// names the window's.
    #[expect(
        dead_code,
        reason = "the window module creates one per window; that module is still a Wave 0 stub"
    )]
    pub(crate) fn new(mtm: MainThreadMarker, callback: FrameCallback) -> crate::Result<Self> {
        let target = DisplayLinkTarget::new(mtm, callback);
        // SAFETY: `target` is a live object that implements `step:` with the
        // one-object argument `CADisplayLink` passes, and the declared return
        // type matches `+displayLinkWithTarget:selector:`, which returns an
        // autoreleased link or nil.
        let link: Option<Retained<CADisplayLink>> = unsafe {
            msg_send![
                CADisplayLink::class(),
                displayLinkWithTarget: &*target,
                selector: sel!(step:)
            ]
        };
        let link = link.ok_or(Error::DisplayLink)?;
        attach(&link, true, mtm);
        Ok(Self {
            link,
            screen: None,
            target,
        })
    }

    /// Paces the callback by `screen` from now on, keeping whether the link
    /// is paused. The window calls this with its scene's screen when it opens
    /// and whenever the scene moves to another screen; naming the current
    /// screen again does nothing. On failure the old link keeps running.
    #[expect(
        dead_code,
        reason = "the window module calls this for its scene's screen; that module is still a Wave 0 stub"
    )]
    pub(crate) fn set_screen(&mut self, screen: &UIScreen) -> crate::Result<()> {
        if self
            .screen
            .as_deref()
            .is_some_and(|current| ptr::eq(current, screen))
        {
            return Ok(());
        }
        // SAFETY: `self.target` is a live object that implements `step:` with
        // the one-object argument `CADisplayLink` passes.
        let link = unsafe { screen.displayLinkWithTarget_selector(&self.target, sel!(step:)) }
            .ok_or(Error::DisplayLink)?;
        attach(&link, self.link.isPaused(), screen.mtm());
        // The new link is on the run loop before the old one leaves it, and
        // both are on the main thread, so no frame runs in between.
        self.link.invalidate();
        self.link = link;
        self.screen = Some(screen.retain());
        Ok(())
    }

    #[expect(
        dead_code,
        reason = "the window module starts it when its window shows; that module is still a Wave 0 stub"
    )]
    pub(crate) fn start(&mut self) {
        self.link.setPaused(false);
    }

    #[expect(
        dead_code,
        reason = "the window module stops it when its window hides; that module is still a Wave 0 stub"
    )]
    pub(crate) fn stop(&mut self) {
        self.link.setPaused(true);
    }
}

impl Drop for DisplayLink {
    fn drop(&mut self) {
        // Removes the link from the run loop, which releases it, and makes it
        // release its target; with `self.target` dropped next, the callback
        // is freed.
        self.link.invalidate();
    }
}

/// Readies a new link: paused or not, asking for [`FRAME_RATE_RANGE`], on the
/// main run loop.
fn attach(link: &CADisplayLink, paused: bool, _mtm: MainThreadMarker) {
    link.setPaused(paused);
    link.setPreferredFrameRateRange(FRAME_RATE_RANGE);
    // SAFETY: `_mtm` proves this is the main thread, whose run loop the link
    // joins, so it fires there. Common modes keep frames coming while UIKit
    // tracks a touch or a scroll in its tracking mode. Foundation initializes
    // `NSRunLoopCommonModes` before any code can run.
    unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
}

struct TargetIvars {
    callback: RefCell<FrameCallback>,
}

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `DisplayLinkTarget` does not implement `Drop`; its ivars drop with it.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetIvars]
    struct DisplayLinkTarget;

    impl DisplayLinkTarget {
        #[unsafe(method(step:))]
        fn step(&self, _link: &CADisplayLink) {
            // The callback may drop its `DisplayLink`, which invalidates the
            // link and releases this target; holding a reference keeps the
            // callback alive until it returns.
            let _alive = self.retain();
            fire(&self.ivars().callback);
        }
    }
);

impl DisplayLinkTarget {
    fn new(mtm: MainThreadMarker, callback: FrameCallback) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars {
            callback: RefCell::new(callback),
        });
        // SAFETY: `init` is NSObject's designated initializer, and the ivars
        // were set above.
        unsafe { msg_send![super(this), init] }
    }
}

/// Runs the frame callback unless it is already running, which happens when
/// it spins the run loop itself (a modal wait): a second frame inside the
/// first is skipped rather than reentering the callback. Returns whether the
/// callback ran.
fn fire(callback: &RefCell<FrameCallback>) -> bool {
    let Ok(mut callback) = callback.try_borrow_mut() else {
        tracing::trace!("skipped a frame that arrived while the previous one was still drawing");
        return false;
    };
    (*callback)();
    true
}
