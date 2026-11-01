//! `gpui::Platform` for iPadOS: `UIApplicationMain`, the application and
//! scene delegates, the pasteboard, keyboard layout, URL opening, and the app
//! lifecycle and memory-warning callbacks GPUI's core already models.
//!
//! [`gpui::Platform::run`] calls `UIApplicationMain`, which never returns. The
//! application delegate supplies a scene configuration whose delegate class
//! forwards scene events to `crate::window`, and GPUI's launch callback runs
//! once, when the first window scene connects (herdr-gpui's docs/ios.md).
//!
//! Desktop features iPadOS lacks log and do nothing, or fail where the trait
//! returns a result; none reports success for something that did not happen.

mod app_delegate;
mod error;
mod idle;
mod keyboard;
mod lifecycle;
mod pasteboard;
mod scene_delegate;
mod shared;
mod slot;
mod system;
mod thermal;

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use anyhow::Result;
use futures_channel::oneshot;
use gpui::{
    Action, ActivityGuard, AnyWindowHandle, AppLifecyclePhase, BackgroundExecutor, ClipboardItem,
    CursorStyle, DummyKeyboardMapper, ForegroundExecutor, Keymap, Menu, MenuItem,
    PathPromptOptions, Platform, PlatformDispatcher, PlatformDisplay, PlatformKeyboardLayout,
    PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Task, ThermalState,
    WindowAppearance, WindowKind, WindowParams, popup::PopupNotSupportedError,
};
use objc2::MainThreadMarker;
use objc2_ui_kit::UIApplication;

use self::{
    error::Error, keyboard::IosKeyboardLayout, pasteboard::Pasteboard, shared::Shared,
    thermal::ThermalObserver,
};
use crate::{
    dispatcher::IosDispatcher,
    display::IosDisplay,
    renderer,
    text_system::IosTextSystem,
    window::{self, IosWindow},
};

/// GPUI's platform on iPadOS. Create it on the main thread and hand it to
/// `gpui::Application::with_platform`; running the application gives the
/// process to UIKit.
pub struct IosPlatform {
    background_executor: BackgroundExecutor,
    foreground_executor: ForegroundExecutor,
    keyboard_mapper: Rc<DummyKeyboardMapper>,
    pasteboard: Pasteboard,
    renderer_context: renderer::Context,
    shared: Rc<Shared>,
    text_system: Arc<dyn PlatformTextSystem>,
}

impl IosPlatform {
    pub fn new() -> Self {
        let dispatcher: Arc<dyn PlatformDispatcher> = Arc::new(IosDispatcher::new());
        Self {
            background_executor: BackgroundExecutor::new(Arc::clone(&dispatcher)),
            foreground_executor: ForegroundExecutor::new(dispatcher),
            keyboard_mapper: Rc::new(DummyKeyboardMapper),
            pasteboard: Pasteboard::general(),
            renderer_context: renderer::Context::default(),
            shared: Rc::default(),
            text_system: Arc::new(IosTextSystem::new()),
        }
    }
}

impl Default for IosPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for IosPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.background_executor.clone()
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        self.foreground_executor.clone()
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        Arc::clone(&self.text_system)
    }

    /// Enters `UIApplicationMain`, which never returns. `on_finish_launching`
    /// runs when the first window scene connects.
    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::error!(error = %Error::NotMainThread, "cannot start UIKit");
            return;
        };
        self.shared.set_finish_launching(on_finish_launching);
        if !shared::install(Rc::clone(&self.shared)) {
            tracing::error!("a GPUI platform already ran in this process");
            return;
        }
        // UIKit may restore saved scene sessions before it asks the
        // application delegate for any configuration.
        scene_delegate::register();
        UIApplication::main(None, Some(&app_delegate::class_name()), mtm)
    }

    /// iPadOS apps do not end themselves: the system ends the process, and
    /// GPUI's quit callback runs from `applicationWillTerminate:` if it does
    /// so while the app runs.
    fn quit(&self) {
        tracing::info!("not quitting: iPadOS ends apps itself");
    }

    /// An iPadOS app cannot relaunch itself.
    fn restart(&self, _binary_path: Option<PathBuf>, _arguments: Vec<OsString>) {
        tracing::warn!("not restarting: iPadOS apps cannot relaunch themselves");
    }

    /// Only the user brings an app forward on iPadOS.
    fn activate(&self, _ignoring_other_apps: bool) {}

    /// Only the user moves an app out of the way on iPadOS.
    fn hide(&self) {}

    /// Apps cannot hide one another on iPadOS.
    fn hide_other_apps(&self) {}

    /// Apps cannot hide one another on iPadOS, so there is nothing to show.
    fn unhide_other_apps(&self) {}

    /// Screens are UIKit objects, so off the main thread there are none.
    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        IosDisplay::all(mtm)
            .into_iter()
            .map(|display| -> Rc<dyn PlatformDisplay> { Rc::new(display) })
            .collect()
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        let mtm = MainThreadMarker::new()?;
        Some(Rc::new(IosDisplay::primary(mtm)))
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        MainThreadMarker::new().and_then(window::active_window)
    }

    fn window_stack(&self) -> Option<Vec<AnyWindowHandle>> {
        MainThreadMarker::new().map(window::window_stack)
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        options: WindowParams,
    ) -> Result<Box<dyn PlatformWindow>> {
        // A native popup would become a scene of its own; refusing makes GPUI
        // draw its popovers inside the parent window instead.
        if matches!(options.kind, WindowKind::AnchoredPopup(_)) {
            return Err(PopupNotSupportedError.into());
        }
        let mtm = MainThreadMarker::new().ok_or(Error::NotMainThread)?;
        let window = IosWindow::open(
            handle,
            options,
            self.foreground_executor.clone(),
            self.renderer_context.clone(),
            mtm,
        )?;
        Ok(Box::new(window))
    }

    fn window_appearance(&self) -> WindowAppearance {
        MainThreadMarker::new().map_or(WindowAppearance::Light, system::window_appearance)
    }

    fn open_url(&self, url: &str) {
        match MainThreadMarker::new() {
            Some(mtm) => system::open_url(url, mtm),
            None => tracing::error!(error = %Error::NotMainThread, "not opening a URL"),
        }
    }

    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        self.shared.open_urls.set(callback);
    }

    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Err(Error::UrlSchemeRegistration.into()))
    }

    fn prompt_for_paths(
        &self,
        _options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        refused(Error::PathPrompt)
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        _suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        refused(Error::PathPrompt)
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    /// There is no file manager window to show a path in.
    fn reveal_path(&self, _path: &Path) {
        tracing::warn!("not revealing a path: iPad has no file manager window to show it in");
    }

    /// Handing a file to another app takes a share sheet the user starts.
    fn open_with_system(&self, _path: &Path) {
        tracing::warn!("not opening a file in another app: iPadOS needs the user to share it");
    }

    fn on_quit(&self, callback: Box<dyn FnMut() -> bool>) {
        self.shared.quit.set(callback);
    }

    /// A scene connecting while GPUI has no window is the reopen (see
    /// `Shared::scene_connected`).
    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        self.shared.reopen.set(callback);
    }

    /// iPadOS tells apps nothing about the device sleeping; a locked screen
    /// reaches the app as its scenes entering the background.
    fn on_system_sleep(&self, _callback: Box<dyn FnMut()>) {}

    /// As with sleep, waking reaches the app as its scenes returning.
    fn on_system_wake(&self, _callback: Box<dyn FnMut()>) {}

    fn on_app_lifecycle(&self, callback: Box<dyn FnMut(AppLifecyclePhase)>) {
        self.shared.app_lifecycle.set(callback);
    }

    fn on_memory_warning(&self, callback: Box<dyn FnMut()>) {
        self.shared.memory_warning.set(callback);
    }

    /// The iPadOS menu bar (`UIMenuBuilder`) is not wired to GPUI's menus.
    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {}

    /// iPadOS has no Dock menu.
    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {}

    /// No menu is shown (`set_menus`), so no menu action arrives.
    fn on_app_menu_action(&self, _callback: Box<dyn FnMut(&dyn Action)>) {}

    /// No menu is shown, so none opens.
    fn on_will_open_app_menu(&self, _callback: Box<dyn FnMut()>) {}

    /// No menu is shown, so no command is validated.
    fn on_validate_app_menu_command(&self, _callback: Box<dyn FnMut(&dyn Action) -> bool>) {}

    fn thermal_state(&self) -> ThermalState {
        thermal::thermal_state()
    }

    fn on_thermal_state_change(&self, callback: Box<dyn FnMut()>) {
        self.shared.thermal_state_change.set(callback);
        self.shared
            .thermal_observer
            .get_or_init(ThermalObserver::register);
    }

    fn prevent_idle_sleep(&self, reason: &str) -> Task<Result<ActivityGuard>> {
        Task::ready(
            MainThreadMarker::new()
                .map(|mtm| idle::hold(reason, mtm))
                .ok_or_else(|| Error::NotMainThread.into()),
        )
    }

    fn compositor_name(&self) -> &'static str {
        "UIKit"
    }

    fn app_path(&self) -> Result<PathBuf> {
        Ok(system::bundle_path())
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        Err(Error::AuxiliaryExecutable.into())
    }

    /// GPUI calls this for the hovered window. Its interaction owns the
    /// native shape; there is no app-wide UIKit cursor.
    fn set_cursor_style(&self, style: CursorStyle) {
        if let Some(mtm) = MainThreadMarker::new() {
            window::set_cursor_style(style, mtm);
        }
    }

    /// iPadOS hides the pointer itself while the user types.
    fn hide_cursor_until_mouse_moves(&self) {}

    /// The app never hides the pointer (`hide_cursor_until_mouse_moves`).
    fn is_cursor_visible(&self) -> bool {
        true
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.pasteboard.read()
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        self.pasteboard.write(item);
    }

    fn write_credentials(&self, _url: &str, _username: &str, _password: &[u8]) -> Task<Result<()>> {
        Task::ready(Err(Error::Credentials.into()))
    }

    fn read_credentials(&self, _url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        Task::ready(Err(Error::Credentials.into()))
    }

    fn delete_credentials(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Err(Error::Credentials.into()))
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(IosKeyboardLayout)
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        self.keyboard_mapper.clone()
    }

    /// The reported layout never changes (`keyboard`), so there is no change
    /// to announce.
    fn on_keyboard_layout_change(&self, _callback: Box<dyn FnMut()>) {}
}

/// A prompt answered at once with `error`.
fn refused<T>(error: Error) -> oneshot::Receiver<Result<T>> {
    let (sender, receiver) = oneshot::channel();
    // The receiver is held here, so the send cannot fail.
    let _ = sender.send(Err(error.into()));
    receiver
}
