//! The window's title bar when the iPad app runs on a Mac ("Designed for
//! iPad"). There each scene's window gets a separate title bar above the
//! app's content. On a Mac only, the content fills the window instead and the
//! traffic lights sit over its top row, as in Sheprd. Two steps do that:
//!
//! 1. As the scene connects, the scene delegate hides its title
//!    (`UITitlebar.titleVisibility`, leaving the toolbar nil; macOS SDK
//!    `iOSSupport/.../UIKit.framework/Headers/NSToolbar+UIKitAdditions.h`).
//!    On macOS 26.7.3 this reaches the window: its title is hidden and its
//!    title bar transparent. The content still starts below the title bar,
//!    which then shows as an empty band (task b903b39987a54c31, `diag.jsonl`).
//! 2. Once the scene's window exists, `window::extend_content` adds
//!    `NSWindowStyleMaskFullSizeContentView` to it, so the content reaches the
//!    window's top edge. UIKit then reports the title bar's band as top safe
//!    area, which the content gives up ([`content_top_inset`]): its top row
//!    fills the band and keeps clear of the traffic lights through the
//!    window-control inset ([`clear_traffic_lights`]).
//!
//! The SDK declares `titlebar` for Mac Catalyst only, and an iOS app has no
//! API for its NSWindow, so both steps go through the runtime and check each
//! selector with `respondsToSelector:` first. A Mac where step 1 is missing
//! keeps its title bar; where step 2 cannot run within a few seconds, step 1
//! is undone, so the standard title bar stays rather than an empty band.

#![allow(
    unsafe_code,
    reason = "sends the Mac Catalyst titlebar selectors, which objc2-ui-kit does not generate \
              for iOS, after checking that the receiver responds to each"
)]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use objc2::{msg_send, rc::Retained, runtime::NSObject, sel};
use objc2_foundation::{NSObjectProtocol, NSProcessInfo};
use objc2_ui_kit::{UIEdgeInsets, UIWindowScene};

use crate::LayoutDirection;

#[cfg(test)]
mod tests;
mod window;

/// How far the window's top row keeps clear of the traffic lights while the
/// content sits under them, in the Mac's points from the window's edge: the
/// three buttons and their margins, as Sheprd leaves 80 pt before its title
/// bar's first item for the same buttons (sheprd
/// `crates/sheprd-gpui/src/titlebar.rs`, `LEADING`). UIKit measures in the
/// iPad's points, which the Mac draws smaller ([`clearance`]).
pub(crate) const TRAFFIC_LIGHT_CLEARANCE: f64 = 80.0;

/// `UITitlebarTitleVisibilityVisible` and `...Hidden`, the cases of the
/// header's `NS_ENUM(NSInteger, UITitlebarTitleVisibility)`.
const TITLE_VISIBILITY_VISIBLE: isize = 0;
const TITLE_VISIBILITY_HIDDEN: isize = 1;

/// Set once a window's content extends under its hidden title bar. Every
/// scene of the process makes the same decision, so one flag serves all
/// windows.
static TITLE_HIDDEN: AtomicBool = AtomicBool::new(false);

/// The Mac's points per UIKit point, as `f64` bits; 1.0 until measured.
static SCALE: AtomicU64 = AtomicU64::new(0x3FF0_0000_0000_0000);

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

/// Hides the title of `scene`'s window and extends its content under the
/// title bar on a Mac; does nothing elsewhere. Called as the scene connects,
/// before its window shows.
pub(crate) fn hide_title(scene: &UIWindowScene) {
    if !hides_title(ios_app_on_mac()) {
        return;
    }
    if set_title_visibility(scene, TITLE_VISIBILITY_HIDDEN) {
        window::extend_content(scene, 0);
    }
}

/// Whether the content extends under a hidden title bar, so the traffic
/// lights sit over its top row.
pub(crate) fn title_hidden() -> bool {
    TITLE_HIDDEN.load(Ordering::Relaxed)
}

/// Records that a window's content now extends under its hidden title bar,
/// drawn at `scale` of the Mac's points per UIKit point.
fn content_extended(scale: f64) {
    SCALE.store(scale.to_bits(), Ordering::Relaxed);
    TITLE_HIDDEN.store(true, Ordering::Relaxed);
}

/// The content's window could not be extended: the title shows again, so
/// the window keeps the standard title bar.
fn content_not_extended(scene: &UIWindowScene) {
    tracing::warn!("this Mac's window could not extend under its title bar; keeping the title bar");
    set_title_visibility(scene, TITLE_VISIBILITY_VISIBLE);
}

/// Sets the scene's title visibility; false, logged, when this Mac lacks the
/// titlebar or its setter.
fn set_title_visibility(scene: &UIWindowScene, visibility: isize) -> bool {
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
    let () = unsafe { msg_send![&*titlebar, setTitleVisibility: visibility] };
    true
}

/// The Mac's points per UIKit point for a window `mac_width` points wide on
/// the Mac whose UIKit window is `uikit_width` wide (0.77 on a Mac running
/// the iPad app at its default size, 1495 / 1942 pt); 1.0 for any width that
/// is not positive and finite.
pub(crate) fn window_scale(mac_width: f64, uikit_width: f64) -> f64 {
    let scale = mac_width / uikit_width;
    if mac_width > 0.0 && uikit_width > 0.0 && scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

/// [`TRAFFIC_LIGHT_CLEARANCE`] in UIKit points at `scale` (about 104 pt at
/// 0.77); the Mac's own value at any scale that is not positive and finite.
pub(crate) fn clearance(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        TRAFFIC_LIGHT_CLEARANCE / scale
    } else {
        TRAFFIC_LIGHT_CLEARANCE
    }
}

/// The top safe area the content keeps: none while it extends under the
/// hidden title bar, whose band UIKit reports as safe area, so the top row
/// fills that band; UIKit's value otherwise.
pub(crate) fn content_top_inset(title_hidden: bool, reported_top: f64) -> f64 {
    if title_hidden { 0.0 } else { reported_top }
}

/// The window-control sample with the traffic lights covered while the
/// content sits under them: at least [`TRAFFIC_LIGHT_CLEARANCE`] on their
/// side, in UIKit points at the measured scale, the left in a left-to-right
/// app and the right in a right-to-left one, where a Mac mirrors its window
/// buttons. Unchanged while the title shows.
pub(crate) fn clear_traffic_lights(
    title_hidden: bool,
    direction: LayoutDirection,
    sampled: UIEdgeInsets,
) -> UIEdgeInsets {
    if !title_hidden {
        return sampled;
    }
    let floor = clearance(f64::from_bits(SCALE.load(Ordering::Relaxed)));
    match direction {
        LayoutDirection::LeftToRight => UIEdgeInsets {
            left: sampled.left.max(floor),
            ..sampled
        },
        LayoutDirection::RightToLeft => UIEdgeInsets {
            right: sampled.right.max(floor),
            ..sampled
        },
    }
}
