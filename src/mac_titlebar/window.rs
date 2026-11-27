//! Step 2 of `crate::mac_titlebar`: the scene's NSWindow extends its content
//! under the hidden title bar (`NSWindowStyleMaskFullSizeContentView`).
//!
//! An iOS app has no API for its NSWindow. On a Mac, AppKit runs in the
//! process and `NSApplication.sharedApplication.windows` lists the scene
//! windows. Which of them shows this scene only UIKitMacHelper tells, through
//! two private getters ([`scene_identifier`]); without them every titled
//! window is extended ([`targets`]). Every selector is checked with
//! `respondsToSelector:` before it is sent. The window does not exist yet
//! when the scene connects (it appeared within 100 ms on macOS 26.7.3), so
//! this retries on the main queue and gives up after five seconds.

#![allow(
    unsafe_code,
    reason = "sends AppKit and UIKitMacHelper selectors to the process's NSApplication and \
              NSWindows, which an iOS SDK does not declare, after checking that the receiver \
              responds to each"
)]

use dispatch2::{DispatchQueue, DispatchTime, MainThreadBound};
use objc2::{
    MainThreadMarker, Message, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Sel},
    sel,
};
use objc2_core_foundation::CGRect;
use objc2_foundation::NSString;
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

/// One of the process's NSWindows as an attempt sees it: its style mask,
/// and the persistent identifier of the scene it shows when the window tells
/// ([`scene_identifier`]).
#[derive(Debug)]
pub(crate) struct Candidate<'a> {
    pub(crate) mask: usize,
    pub(crate) scene: Option<&'a str>,
}

/// The windows to extend for the scene whose session's persistent
/// identifier is `scene`: each one's index in `windows` and its new mask.
/// Once any window tells its scene, only the titled windows showing this
/// scene, so another scene's window and a window of no scene are left alone
/// (herdr-uikit#29). Where none tells (a Mac without the UIKitMacHelper
/// getters), every titled window, as before.
pub(crate) fn targets(scene: &str, windows: &[Candidate<'_>]) -> Vec<(usize, usize)> {
    let told = windows.iter().any(|window| window.scene.is_some());
    windows
        .iter()
        .enumerate()
        .filter(|(_, window)| !told || window.scene == Some(scene))
        .filter_map(|(index, window)| extended_mask(window.mask).map(|mask| (index, mask)))
        .collect()
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

/// The persistent identifier of the scene `window` shows, which matches its
/// `UISceneSession.persistentIdentifier`: UIKitMacHelper's private
/// `UINSWindow.sceneWindowController` and that controller's
/// `persistentIdentifier` (both listed on macOS 26.7.3, task
/// 9a44558d37d143fd `i29/uikitmachelper-introspection.txt`). `None` for a
/// window that does not answer them or gives no string.
fn scene_identifier(window: &AnyObject) -> Option<String> {
    if !responds(window, sel!(sceneWindowController)) {
        return None;
    }
    // SAFETY: the window responds to `sceneWindowController`, an object
    // getter with no arguments; `Option` admits nil.
    let controller: Option<Retained<AnyObject>> =
        unsafe { msg_send![window, sceneWindowController] };
    let controller = controller?;
    if !responds(&controller, sel!(persistentIdentifier)) {
        return None;
    }
    // SAFETY: the controller responds to `persistentIdentifier`, an object
    // getter with no arguments; `Option` admits nil, and the class is
    // checked before it is read as a string.
    let identifier: Option<Retained<AnyObject>> =
        unsafe { msg_send![&*controller, persistentIdentifier] };
    let identifier = identifier?.downcast::<NSString>().ok()?;
    Some(identifier.to_string())
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
    let selectors = [sel!(styleMask), sel!(setStyleMask:), sel!(frame)];
    let windows: Vec<_> = windows
        .into_iter()
        .filter(|window| {
            selectors
                .into_iter()
                .all(|selector| responds(window, selector))
        })
        .map(|window| {
            // SAFETY: NSWindow's NSUInteger `styleMask` getter, checked
            // above.
            let mask: usize = unsafe { msg_send![&*window, styleMask] };
            let scene = scene_identifier(&window);
            (window, mask, scene)
        })
        .collect();
    let candidates: Vec<_> = windows
        .iter()
        .map(|(_, mask, scene)| Candidate {
            mask: *mask,
            scene: scene.as_deref(),
        })
        .collect();
    let scene_id = scene.session().persistentIdentifier().to_string();
    let mut extended = None;
    for (index, mask) in targets(&scene_id, &candidates) {
        let Some((window, _, _)) = windows.get(index) else {
            continue;
        };
        // SAFETY: NSWindow's NSRect `frame` getter, checked above.
        let frame: CGRect = unsafe { msg_send![&**window, frame] };
        let scale = super::window_scale(frame.size.width, uikit_width);
        // Before the change: the safe-area callback it triggers reads both.
        super::content_extended(scale);
        // SAFETY: NSWindow's NSUInteger `setStyleMask:`, checked above.
        let () = unsafe { msg_send![&**window, setStyleMask: mask] };
        extended = Some(scale);
    }
    extended.map_or(Attempt::NotYet, Attempt::Extended)
}
