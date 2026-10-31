//! Windows as UIKit scenes. Each `IosWindow` owns one `UIWindow` in one
//! `UIWindowScene`, a root view controller, and a `GPUIView` (a `UIView`
//! subclass) that hosts the renderer's `CAMetalLayer` and turns touches,
//! pointer hover and scroll, hardware key presses, and `UITextInput` editing
//! into GPUI input.
//!
//! Scenes arrive on the system's schedule, not GPUI's, so this module keeps
//! the bookkeeping between them (docs/ios.md, "Scenes and windows"): the
//! platform's scene delegate reports each scene through [`scene_connected`],
//! [`scene_disconnected`], and [`scene_activation_changed`]; `IosWindow::open`
//! takes a connected scene that hosts no window yet, or asks the system for a
//! new one and attaches when it connects.

// Wave 0 skeleton: the window stream replaces this.
#![allow(dead_code, clippy::todo)]

mod input_target;
mod text_input;

use gpui::{AnyWindowHandle, ForegroundExecutor, WindowParams};
use objc2::MainThreadMarker;
use objc2_ui_kit::UIWindowScene;

use crate::renderer;

/// Implements `gpui::PlatformWindow` once the window stream lands.
pub(crate) struct IosWindow {}

impl IosWindow {
    pub(crate) fn open(
        _handle: AnyWindowHandle,
        _params: WindowParams,
        _executor: ForegroundExecutor,
        _renderer_context: renderer::Context,
        _mtm: MainThreadMarker,
    ) -> anyhow::Result<Self> {
        todo!("window stream")
    }
}

/// A scene connected. It hosts the oldest window waiting for a scene, or
/// waits for the next `IosWindow::open`.
pub(crate) fn scene_connected(_scene: &UIWindowScene, _mtm: MainThreadMarker) {
    todo!("window stream")
}

/// A scene disconnected, which closes the window it hosted.
pub(crate) fn scene_disconnected(_scene: &UIWindowScene, _mtm: MainThreadMarker) {
    todo!("window stream")
}

/// A scene became active (foreground and focused) or stopped being so.
pub(crate) fn scene_activation_changed(
    _scene: &UIWindowScene,
    _active: bool,
    _mtm: MainThreadMarker,
) {
    todo!("window stream")
}

/// The window in the active scene, if any.
pub(crate) fn active_window(_mtm: MainThreadMarker) -> Option<AnyWindowHandle> {
    todo!("window stream")
}

/// Windows front to back, as best UIKit reports scene order.
pub(crate) fn window_stack(_mtm: MainThreadMarker) -> Vec<AnyWindowHandle> {
    todo!("window stream")
}
