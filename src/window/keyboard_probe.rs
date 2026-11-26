//! A transparent subview of the drawing view whose `UIKeyboardLayoutGuide`
//! follows undocked keyboards (`followsUndockedKeyboard`), so the window can
//! tell a floating keyboard from a hidden one. The drawing view's own guide
//! keeps following only a docked keyboard: its frame is the bottom inset
//! (`layout/keyboard_geometry.rs`).
//!
//! The keyboard frame notification alone cannot tell: after a keyboard
//! floats, UIKit posts one more frame, zero tall at the screen's bottom,
//! while the floating keyboard stays up; this guide keeps the floating frame
//! (yebisu#229, iPadOS 27.2 Simulator; `docs/keyboard-bar.md`).
#![allow(
    unsafe_code,
    reason = "UIKeyboardLayoutGuide's selectors are sent with msg_send: objc2's typed \
              subclass needs the unused UITrackingLayoutGuide feature"
)]

use objc2::{MainThreadMarker, msg_send, rc::Retained};
use objc2_core_foundation::CGRect;
use objc2_ui_kit::{UICoordinateSpace, UILayoutGuide, UIScreen, UIView, UIViewAutoresizing};

/// Adds the probe to `host`, filling it, and makes its guide follow
/// undocked keyboards. It takes no touches and no accessibility focus.
pub(super) fn install(host: &UIView, mtm: MainThreadMarker) -> Retained<UIView> {
    let probe = UIView::new(mtm);
    probe.setUserInteractionEnabled(false);
    // SAFETY: NSObject's UIAccessibility category; the setter takes a BOOL.
    let _: () = unsafe { msg_send![&*probe, setAccessibilityElementsHidden: true] };
    probe.setFrame(host.bounds());
    probe.setAutoresizingMask(
        UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
    );
    host.addSubview(&probe);
    let guide = guide(&probe);
    // SAFETY: the guide is the UIKeyboardLayoutGuide `keyboardLayoutGuide`
    // returned; the setter takes an Objective-C BOOL (iOS 15).
    let _: () = unsafe { msg_send![&*guide, setFollowsUndockedKeyboard: true] };
    probe
}

fn guide(probe: &UIView) -> Retained<UILayoutGuide> {
    // SAFETY: UIView returns its UIKeyboardLayoutGuide, a UILayoutGuide
    // subclass, on the minimum supported iOS 26.
    unsafe { msg_send![probe, keyboardLayoutGuide] }
}

/// The keyboard the guide follows, in `screen`'s coordinates (x, y, width,
/// height). A hidden keyboard leaves a full-width guide on the bottom safe
/// area.
pub(super) fn keyboard_in_screen(probe: &UIView, screen: &UIScreen) -> (f64, f64, f64, f64) {
    // Nothing else lays the probe out, so its guide's frame is resolved
    // here; the probe is a leaf, and UIKit skips a pass already running.
    probe.layoutIfNeeded();
    let frame = guide(probe).layoutFrame();
    let rect: CGRect = probe.convertRect_toCoordinateSpace(frame, &screen.coordinateSpace());
    (
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
}
