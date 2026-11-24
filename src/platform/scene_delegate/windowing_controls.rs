//! The windowing-control style each window scene asks UIKit for.
//!
//! On a Mac running the iPad app ("Designed for iPad") a scene asks for
//! `unifiedStyle`, whose controls "appear as part of the scene's content"
//! (UIKit's UISceneWindowingControlStyle.h, iOS 26): the window's separate
//! title bar goes and the traffic lights sit in the app's own top row. The
//! corner-adapted safe area then reports them, so `window_control_insets`
//! covers them as it covers iPadOS 26's window controls. On an iPad the
//! scene asks for `automaticStyle`, which UIKit's header documents as what it
//! uses when the delegate does not implement the method.

use objc2::rc::Retained;
use objc2_foundation::NSProcessInfo;
use objc2_ui_kit::UISceneWindowingControlStyle;

/// The style a scene asks for, apart from the UIKit object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControlStyle {
    /// UIKit's default, as if the delegate did not implement the method.
    Automatic,
    /// The controls drawn over the scene's own content.
    Unified,
}

/// The style for a scene of this process: unified on a Mac running the iPad
/// app, automatic everywhere else.
pub(super) fn control_style(ios_app_on_mac: bool) -> ControlStyle {
    if ios_app_on_mac {
        ControlStyle::Unified
    } else {
        ControlStyle::Automatic
    }
}

/// Whether this process is the iPad app running on a Mac. Mac Catalyst
/// builds, which herdr-uikit does not make, report false.
pub(super) fn ios_app_on_mac() -> bool {
    NSProcessInfo::processInfo().isiOSAppOnMac()
}

/// The UIKit object for `style`. UIKit asks for a style only on iOS 26 and
/// later, where both class properties exist.
pub(super) fn uikit_style(style: ControlStyle) -> Retained<UISceneWindowingControlStyle> {
    match style {
        ControlStyle::Automatic => UISceneWindowingControlStyle::automaticStyle(),
        ControlStyle::Unified => UISceneWindowingControlStyle::unifiedStyle(),
    }
}
