//! The keyboard layout GPUI sees. UIKit names no hardware keyboard layout,
//! and `UIKey` already reports the characters the active layout produces, so
//! there is nothing to translate: the app reports one fixed layout and GPUI's
//! identity key-equivalent mapper, as GPUI's web platform does. The layout
//! never changes, so there is no layout change to announce either.

use gpui::PlatformKeyboardLayout;

pub(super) struct IosKeyboardLayout;

impl PlatformKeyboardLayout for IosKeyboardLayout {
    fn id(&self) -> &str {
        "ios"
    }

    fn name(&self) -> &str {
        "iPadOS"
    }
}
