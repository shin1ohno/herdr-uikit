//! Nullable application lookup for queries before UIApplicationMain.
#![allow(unsafe_code, reason = "UIKit can return nil before UIApplicationMain")]

use objc2::{MainThreadMarker, msg_send, rc::Retained};
use objc2_ui_kit::UIApplication;

pub(crate) fn shared(_mtm: MainThreadMarker) -> Option<Retained<UIApplication>> {
    // SAFETY: The marker proves main-thread access. The selector returns a
    // UIApplication with normal autoreleased ownership, or nil before launch.
    unsafe { msg_send![objc2::class!(UIApplication), sharedApplication] }
}
