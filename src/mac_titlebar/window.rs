//! Step 2 of `crate::mac_titlebar`: the scene's NSWindow extends its content
//! under the hidden title bar (`NSWindowStyleMaskFullSizeContentView`).
//!
//! An iOS app has no API for its NSWindow. On a Mac, AppKit runs in the
//! process and `NSApplication.sharedApplication.windows` lists the scene
//! windows; every selector below is public AppKit and is checked with
//! `respondsToSelector:` before it is sent. The window does not exist yet
//! when the scene connects (it appeared within 100 ms on macOS 26.7.3), so
//! this retries on the main queue and gives up after five seconds.

#![allow(
    unsafe_code,
    reason = "sends public AppKit selectors to the process's NSApplication and NSWindows, which \
              an iOS SDK does not declare, after checking that the receiver responds to each"
)]

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};
use objc2::{
    MainThreadMarker, Message, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Sel},
    sel,
};
use objc2_core_foundation::CGRect;
use objc2_ui_kit::UIWindowScene;

#[cfg(test)]
mod tests;

/// `NSWindowStyleMaskTitled` and `NSWindowStyleMaskFullSizeContentView`
/// (macOS SDK `AppKit.framework/Headers/NSWindow.h`).
const TITLED: usize = 1;
const FULL_SIZE_CONTENT_VIEW: usize = 1 << 15;

/// Time between attempts, and how many, before the title shows again.
const RETRY_NANOS: i64 = 100_000_000;
const ATTEMPTS: u32 = 50;

/// The style mask that extends a titled window's content under its title
/// bar; `None` for a window without a title bar, or one that already does.
pub(crate) fn extended_mask(mask: usize) -> Option<usize> {
    (mask & TITLED != 0 && mask & FULL_SIZE_CONTENT_VIEW == 0)
        .then_some(mask | FULL_SIZE_CONTENT_VIEW)
}

/// What one attempt found.
#[derive(Debug, PartialEq)]
pub(crate) enum Attempt {
    /// A window was extended, drawn at this scale of Mac points per UIKit
    /// point.
    Extended(f64),
    /// No window to extend yet.
    NotYet,
    /// This Mac offers no NSApplication windows to extend.
    Unavailable,
}

/// What to do after `attempt` (0-based) found `found`: stop, retry, or give
/// up and show the title again.
#[derive(Debug, PartialEq)]
pub(crate) enum Next {
    Done,
    Retry,
    GiveUp,
}

pub(crate) fn next(found: &Attempt, attempt: u32) -> Next {
    match found {
        Attempt::Extended(_) => Next::Done,
        Attempt::Unavailable => Next::GiveUp,
        Attempt::NotYet if attempt + 1 >= ATTEMPTS => Next::GiveUp,
        Attempt::NotYet => Next::Retry,
    }
}

/// Extends the content of `scene`'s window under its title bar, now or on a
/// later attempt; shows the title again when that cannot happen.
pub(super) fn extend_content(scene: &UIWindowScene, attempt: u32) {
    let found = extend_now(scene);
    match next(&found, attempt) {
        Next::Done => {
            tracing::debug!(
                attempt,
                ?found,
                "window content extends under the title bar"
            );
            return;
        }
        Next::GiveUp => {
            tracing::warn!(attempt, ?found, "no window to extend under the title bar");
            super::content_not_extended(scene);
            return;
        }
        Next::Retry => {}
    }
    let mtm = MainThreadMarker::from(scene);
    let bound = MainThreadBound::new(scene.retain(), mtm);
    let _ = DispatchQueue::main().after(DispatchTime::NOW.time(RETRY_NANOS), move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        extend_content(bound.get(mtm), attempt + 1);
    });
}

fn responds(object: &AnyObject, selector: Sel) -> bool {
    // SAFETY: every NSObject answers respondsToSelector: with a BOOL.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

/// The process's NSWindows, or `None` when this Mac offers no
/// `NSApplication.sharedApplication.windows`.
fn ns_windows() -> Option<Vec<Retained<AnyObject>>> {
    let class = AnyClass::get(c"NSApplication")?;
    // SAFETY: NSApplication's class getter takes no arguments; `Option`
    // admits nil.
    let app: Option<Retained<AnyObject>> = unsafe { msg_send![class, sharedApplication] };
    let app = app?;
    if !responds(&app, sel!(windows)) {
        return None;
    }
    // SAFETY: `windows` is NSApplication's NSArray<NSWindow *> getter.
    let windows: Option<Retained<AnyObject>> = unsafe { msg_send![&*app, windows] };
    let windows = windows?;
    // SAFETY: NSArray's count and objectAtIndex: with an index below it.
    let count: usize = unsafe { msg_send![&*windows, count] };
    Some(
        (0..count)
            .map(|index| unsafe { msg_send![&*windows, objectAtIndex: index] })
            .collect(),
    )
}

/// The width of `scene`'s first UIWindow, in UIKit points.
fn uikit_width(scene: &UIWindowScene) -> Option<f64> {
    let window = scene.windows().firstObject()?;
    Some(window.bounds().size.width)
}

fn extend_now(scene: &UIWindowScene) -> Attempt {
    let Some(windows) = ns_windows() else {
        return Attempt::Unavailable;
    };
    let Some(uikit_width) = uikit_width(scene) else {
        return Attempt::NotYet;
    };
    let mut extended = None;
    for window in windows {
        let selectors = [sel!(styleMask), sel!(setStyleMask:), sel!(frame)];
        if !selectors
            .into_iter()
            .all(|selector| responds(&window, selector))
        {
            continue;
        }
        // SAFETY: NSWindow's NSUInteger `styleMask` and NSRect `frame`
        // getters, checked above.
        let mask: usize = unsafe { msg_send![&*window, styleMask] };
        let Some(mask) = extended_mask(mask) else {
            continue;
        };
        let frame: CGRect = unsafe { msg_send![&*window, frame] };
        let scale = super::window_scale(frame.size.width, uikit_width);
        // Before the change: the safe-area callback it triggers reads both.
        super::content_extended(scale);
        // SAFETY: NSWindow's NSUInteger `setStyleMask:`, checked above.
        let () = unsafe { msg_send![&*window, setStyleMask: mask] };
        extended = Some(scale);
    }
    extended.map_or(Attempt::NotYet, Attempt::Extended)
}
