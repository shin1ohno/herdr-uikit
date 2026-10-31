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
//! The crate is empty on every other target. FFI modules allow `unsafe_code`
//! for themselves only, and every `unsafe` block states its invariants in a
//! `// SAFETY:` comment.
#![cfg(target_os = "ios")]

mod dispatcher;
mod display;
mod display_link;
mod error;
mod platform;
mod renderer;
mod text_system;
mod window;

pub use error::{Error, Result};
pub use platform::IosPlatform;

/// The GPUI application on iPadOS, the counterpart of
/// `gpui_platform::application()` on desktop targets.
///
/// Wave 0 skeleton: the platform stream replaces the body with
/// `gpui::Application::with_platform(Rc::new(IosPlatform::new()))` once
/// `IosPlatform` implements `gpui::Platform`.
#[allow(clippy::todo)]
pub fn application() -> gpui::Application {
    todo!("platform stream")
}
