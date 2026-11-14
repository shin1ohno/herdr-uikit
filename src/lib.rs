//! GPUI's platform for iPadOS, on UIKit.
//!
//! GPUI ships no iOS platform, but `gpui::Application::with_platform` accepts
//! any [`gpui::Platform`], so this crate supplies one and Herdr UI selects it on
//! `target_os = "ios"` in place of `gpui_platform::application()`. The Metal
//! renderer and the Core Text text system are ported from GPUI's macOS crates
//! (`NOTICE.md`); the application lifecycle, windows, displays, and input are
//! UIKit's.
//!
//! UIKit owns the process: [`gpui::Platform::run`] enters `UIApplicationMain`
//! and never returns. Windows are scenes. GPUI's launch callback runs when the
//! first `UIWindowScene` connects, so the first `open_window` has a scene to
//! draw in; later windows ask the system for another scene. Every UIKit call
//! happens on the main thread, which `objc2::MainThreadMarker` proves.
//!
//! Native editing commands synthesize the usual GPUI Command-key events.
//! Consumers can read [`native_edit_action()`] in their synchronous input
//! callback to distinguish [`NativeEditAction::Paste`] (and Copy, Cut, SelectAll)
//! from an otherwise identical physical key. The origin is scoped to key-down
//! and key-up, restores on unwinding, and does not change event handling or the
//! unhandled-paste fallback.
//!
//! The crate is empty on every other target. FFI modules allow `unsafe_code`
//! for themselves only, and every `unsafe` block states its invariants in a
//! `// SAFETY:` comment.
#![cfg(target_os = "ios")]

mod application;
mod background_task;
mod dispatcher;
mod display;
mod display_link;
mod error;
mod gpui_call;
mod key_routing;
mod layout_direction;
mod menu;
mod native_edit_action;
mod native_edit_availability;
mod native_edit_menu;
mod platform;
mod renderer;
mod scene_fallback;
mod text_system;
mod window;
mod window_controls;

pub use background_task::BackgroundTask;
pub use error::{Error, Result};
pub use layout_direction::{LayoutDirection, layout_direction, set_layout_direction};
pub use native_edit_action::{NativeEditAction, native_edit_action};
pub use native_edit_availability::{NativeEditAvailability, set_native_edit_availability};
pub use native_edit_menu::{
    EDIT_MENU_IDENTIFIER_PREFIX, EDIT_MENU_PROPERTY_KEY, NativeEditMenuStatus, NativeEditMenuToken,
    RevokeReason, dismiss_native_edit_menu, native_edit_menu_status, native_edit_menu_token,
    present_native_edit_menu,
};
pub use platform::IosPlatform;
#[cfg(feature = "test-support")]
pub use platform::notifications::NotificationResponseFixture;
pub use platform::notifications::{
    Authorization, NotificationError, NotificationResult, notification_authorization,
    post_notification, post_notification_when, remove_notifications,
    request_notification_authorization,
};
pub use window_controls::window_control_insets;

/// The GPUI application on iPadOS, the counterpart of
/// `gpui_platform::application()` on desktop targets.
pub fn application() -> gpui::Application {
    gpui::Application::with_platform(std::rc::Rc::new(IosPlatform::new()))
}

/// Simulator contract runner: called from a harness-free main-thread test binary.
#[cfg(feature = "test-support")]
#[doc(hidden)]
pub fn test_native_edit_menu_contract() {
    window::edit_menu::tests::run();
}

#[cfg(target_abi = "sim")]
pub use native_edit_menu::{
    NativeEditMenuSwitchObservation, take_native_edit_menu_switch_observations,
};
