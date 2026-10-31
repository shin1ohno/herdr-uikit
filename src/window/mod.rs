//! Windows as UIKit scenes. Each `IosWindow` owns one `UIWindow` in one
//! `UIWindowScene`, a root view controller, and a `GpuiView` (a `UIView`
//! subclass) that hosts the renderer's `CAMetalLayer` and turns touches,
//! pointer hover and scroll, hardware key presses, and `UITextInput` editing
//! into GPUI input.
//!
//! Scenes arrive on the system's schedule, not GPUI's, so this module keeps
//! the bookkeeping between them (docs/ios.md, "Scenes and windows"): the
//! platform's scene delegate reports each scene through [`scene_connected`],
//! [`scene_disconnected`], and [`scene_activation_changed`]; `IosWindow::open`
//! takes a connected scene that hosts no window yet, or asks the system for a
//! new one and attaches when it connects. A window whose scene request the
//! system refuses closes, as one whose scene disconnects does.
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | `state` | The shared `Window`, its cached state, and GPUI's callbacks |
//! | `lifecycle` | Building, attaching, activating, focusing, and closing |
//! | `scene`, `requests` | Which scene hosts which window; scene requests to the system |
//! | `view` | The `GpuiView` class and what it forwards |
//! | `layout` | Size, scale, appearance, safe area, and keyboard insets |
//! | `frame` | The display link, frames on demand, the keyboard animation |
//! | `touch`, `pointer` | Touches and clicks; hover and scroll |
//! | `input_target`, `text_input`, `text_state` | Keyboard and text input |
//! | `platform_window` | `gpui::PlatformWindow` over the above |

mod convert;
mod error;
mod frame;
mod input_target;
mod layout;
mod lifecycle;
mod platform_window;
mod pointer;
mod requests;
mod scene;
mod state;
mod text_input;
mod text_state;
mod touch;
mod view;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::{
    AnyWindowHandle, ForegroundExecutor, WindowKind, WindowParams, popup::PopupNotSupportedError,
};
use objc2::MainThreadMarker;
use objc2_ui_kit::UIApplication;

pub(crate) use self::error::Error;
#[allow(
    unused_imports,
    reason = "the platform module's scene delegate and window queries call these"
)]
pub(crate) use self::scene::{
    active_window, scene_activation_changed, scene_connected, scene_disconnected, window_stack,
};
use self::state::Window;
use crate::renderer;

/// A GPUI window on iPadOS. Dropping it closes the window, and its scene.
pub(crate) struct IosWindow {
    window: Rc<Window>,
}

impl IosWindow {
    #[allow(
        dead_code,
        reason = "`gpui::Platform::open_window` in the platform module calls this"
    )]
    pub(crate) fn open(
        handle: AnyWindowHandle,
        params: WindowParams,
        executor: ForegroundExecutor,
        renderer_context: renderer::Context,
        mtm: MainThreadMarker,
    ) -> anyhow::Result<Self> {
        check_kind(&params.kind)?;
        // Without multiple-scene support only the launch scene ever exists.
        if !scene::has_idle_scene(mtm)
            && !UIApplication::sharedApplication(mtm).supportsMultipleScenes()
        {
            return Err(crate::Error::NoScene.into());
        }
        let renderer = renderer::Renderer::new(renderer_context, false)?;
        // From here, dropping `this` tears down whatever was built.
        let this = Self {
            window: Window::new(handle, &params, executor, renderer, mtm),
        };
        this.window.install_display_link()?;
        // A scene is on screen from the moment it exists, so a window opened
        // hidden takes none until GPUI activates it.
        if params.show {
            this.window.place(params.focus);
        }
        Ok(this)
    }
}

/// Refuses the kinds of window iPadOS cannot have. An anchored popup gets
/// GPUI's own error, which tells the caller to draw it inside the window.
fn check_kind(kind: &WindowKind) -> anyhow::Result<()> {
    match kind {
        WindowKind::Normal => Ok(()),
        WindowKind::AnchoredPopup(_) => Err(PopupNotSupportedError.into()),
        WindowKind::PopUp => Err(Error::UnsupportedKind("pop-up").into()),
        WindowKind::Floating => Err(Error::UnsupportedKind("floating").into()),
        WindowKind::Dialog => Err(Error::UnsupportedKind("dialog").into()),
    }
}

impl Drop for IosWindow {
    fn drop(&mut self) {
        self.window.close();
    }
}
