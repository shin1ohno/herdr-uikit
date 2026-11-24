//! The window's title bar when the iPad app runs on a Mac ("Designed for
//! iPad"). There each scene's window gets a separate title bar above the
//! app's content. On a Mac only, the scene delegate hides its title
//! (`UITitlebar.titleVisibility`, leaving the toolbar nil), which UIKit's
//! header documents as hiding "any window chrome at the top of the window"
//! (macOS SDK `iOSSupport/.../UIKit.framework/Headers/NSToolbar+UIKitAdditions.h`,
//! `UITitlebar.titleVisibility`). The content then fills the window and the
//! traffic lights sit over its top row.
//!
//! The SDK declares `titlebar` for Mac Catalyst only; an iOS binary reaches
//! it through the runtime, so each selector is checked with
//! `respondsToSelector:` first and a Mac that lacks one keeps its title bar.
//!
//! Whether the Mac reports the traffic lights through the corner-adapted
//! safe area that `window_control_insets` samples is not known, so while the
//! title is hidden the sample is kept at least [`TRAFFIC_LIGHT_CLEARANCE`]
//! wide on the traffic lights' side. A larger reported reach still wins.

#![allow(
    unsafe_code,
    reason = "sends the Mac Catalyst titlebar selectors, which objc2-ui-kit does not generate \
              for iOS, after checking that the receiver responds to each"
)]

use std::sync::atomic::{AtomicBool, Ordering};

use objc2::{msg_send, rc::Retained, runtime::NSObject, sel};
use objc2_foundation::{NSObjectProtocol, NSProcessInfo};
use objc2_ui_kit::{UIEdgeInsets, UIWindowScene};

use crate::LayoutDirection;

#[cfg(test)]
mod tests;

/// How far the window's top row keeps clear of the traffic lights while the
/// title is hidden, in points from the window's edge: the three buttons and
/// their margins, as Sheprd leaves 80 pt before its title bar's first item
/// for the same buttons (sheprd `crates/sheprd-gpui/src/titlebar.rs`,
/// `LEADING`).
pub(crate) const TRAFFIC_LIGHT_CLEARANCE: f64 = 80.0;

/// `UITitlebarTitleVisibilityHidden`, the second case of the header's
/// `NS_ENUM(NSInteger, UITitlebarTitleVisibility)`.
const TITLE_VISIBILITY_HIDDEN: isize = 1;

/// Set once a scene's title was hidden. Every scene of the process makes the
/// same decision, so one flag serves all windows.
static TITLE_HIDDEN: AtomicBool = AtomicBool::new(false);

/// Whether this process is the iPad app running on a Mac. Mac Catalyst
/// builds, which herdr-uikit does not make, report false.
pub(crate) fn ios_app_on_mac() -> bool {
    NSProcessInfo::processInfo().isiOSAppOnMac()
}

/// Whether a scene hides its window's title: on a Mac only. An iPad has no
/// title bar and keeps everything as it was.
pub(crate) fn hides_title(ios_app_on_mac: bool) -> bool {
    ios_app_on_mac
}

/// Hides the title of `scene`'s window on a Mac; does nothing elsewhere.
/// Called as the scene connects, before its window shows.
pub(crate) fn hide_title(scene: &UIWindowScene) {
    if !hides_title(ios_app_on_mac()) {
        return;
    }
    if hide_title_of(scene) {
        TITLE_HIDDEN.store(true, Ordering::Relaxed);
        tracing::debug!("window title hidden");
    }
}

/// Whether a scene's title has been hidden, so the traffic lights sit over
/// the content.
pub(crate) fn title_hidden() -> bool {
    TITLE_HIDDEN.load(Ordering::Relaxed)
}

fn hide_title_of(scene: &UIWindowScene) -> bool {
    if !scene.respondsToSelector(sel!(titlebar)) {
        tracing::warn!("this Mac's window scene has no titlebar; keeping the title bar");
        return false;
    }
    // SAFETY: the scene responds to `titlebar`, a nullable UITitlebar
    // property with no arguments; `Option` admits nil.
    let titlebar: Option<Retained<NSObject>> = unsafe { msg_send![scene, titlebar] };
    let Some(titlebar) = titlebar else {
        tracing::warn!("this Mac's window scene returned no titlebar; keeping the title bar");
        return false;
    };
    if !titlebar.respondsToSelector(sel!(setTitleVisibility:)) {
        tracing::warn!("this Mac's titlebar has no title visibility; keeping the title bar");
        return false;
    }
    // SAFETY: the titlebar responds to `setTitleVisibility:`, whose argument
    // is an NSInteger enum and which returns nothing.
    let () = unsafe { msg_send![&*titlebar, setTitleVisibility: TITLE_VISIBILITY_HIDDEN] };
    true
}

/// The window-control sample with the traffic lights covered while the title
/// is hidden: at least [`TRAFFIC_LIGHT_CLEARANCE`] on their side, the left in
/// a left-to-right app and the right in a right-to-left one, where a Mac
/// mirrors its window buttons. Unchanged while the title shows.
pub(crate) fn clear_traffic_lights(
    title_hidden: bool,
    direction: LayoutDirection,
    sampled: UIEdgeInsets,
) -> UIEdgeInsets {
    if !title_hidden {
        return sampled;
    }
    match direction {
        LayoutDirection::LeftToRight => UIEdgeInsets {
            left: sampled.left.max(TRAFFIC_LIGHT_CLEARANCE),
            ..sampled
        },
        LayoutDirection::RightToLeft => UIEdgeInsets {
            right: sampled.right.max(TRAFFIC_LIGHT_CLEARANCE),
            ..sampled
        },
    }
}
