//! `gpui::PlatformWindow` for `IosWindow`. Getters read the cached state;
//! UIKit is called only for what GPUI asks to change.
//!
//! Desktop window management (titlebar controls, minimizing, zooming,
//! fullscreen toggles, tabs, resizing on request) has no UIKit counterpart:
//! iPadOS sizes and arranges scenes itself, so those are no-ops.

use std::{ffi::c_void, ptr::NonNull, rc::Rc, sync::Arc};

use futures::channel::oneshot;
use gpui::{
    Bounds, Capslock, DispatchEventResult, GpuSpecs, Modifiers, Pixels, PlatformAtlas,
    PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow, Point, PromptButton,
    PromptLevel, RequestFrameOptions, Scene, Size, TextInputConfiguration, TextInputStateChange,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowInsets,
    WindowVisibility,
};
use objc2::{Message, rc::Retained};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawWindowHandle,
    UiKitWindowHandle, WindowHandle,
};

use super::{IosWindow, state::register};
use crate::display::IosDisplay;

impl PlatformWindow for IosWindow {
    fn on_display_changed(&self, callback: Box<dyn FnMut()>) {
        self.window
            .callbacks
            .display_id
            .set(self.display().map(|display| display.id()));
        register(&self.window.callbacks.display_changed, callback);
    }

    fn bounds(&self) -> Bounds<Pixels> {
        self.window.state.borrow().bounds
    }

    fn is_maximized(&self) -> bool {
        false
    }

    fn window_bounds(&self) -> WindowBounds {
        let bounds = self.bounds();
        if self.is_fullscreen() {
            WindowBounds::Fullscreen(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        }
    }

    fn content_size(&self) -> Size<Pixels> {
        self.window.state.borrow().bounds.size
    }

    fn resize(&mut self, size: Size<Pixels>) {
        tracing::debug!(
            ?size,
            "iPadOS sizes window scenes itself; ignoring a resize"
        );
    }

    fn scale_factor(&self) -> f32 {
        self.window.state.borrow().scale_factor
    }

    fn appearance(&self) -> WindowAppearance {
        self.window.state.borrow().appearance
    }

    /// The screen the window's scene is on; none before it has a scene.
    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        let scene = self.window.state.borrow().scene.clone()?;
        Some(Rc::new(IosDisplay::for_screen(
            scene.screen(),
            self.window.mtm,
        )))
    }

    fn set_text_input_configuration(&mut self, configuration: TextInputConfiguration) {
        #[cfg(feature = "test-support")]
        super::retirement_probe::NativeCutRefusal::Configuration.record();
        self.window
            .text_input
            .set_text_input_configuration(&configuration);
    }

    fn mouse_position(&self) -> Point<Pixels> {
        self.window.state.borrow().mouse_position
    }

    fn modifiers(&self) -> Modifiers {
        self.window.state.borrow().modifiers
    }

    fn capslock(&self) -> Capslock {
        self.window.state.borrow().capslock
    }

    /// GPUI sets the handler again around every frame and event, so this is
    /// not a change the text input view hears of; after each frame the window
    /// compares the handler's text state instead (`text_state`).
    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        self.window.state.borrow_mut().input_handler = Some(input_handler);
        #[cfg(feature = "test-support")]
        super::retirement_probe::NativeCutCounter::HandlerRestores.on_set(&self.window);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        #[cfg(feature = "test-support")]
        super::retirement_probe::NativeCutCounter::HandlerRestores.on_take(&self.window);
        self.window.state.borrow_mut().input_handler.take()
    }

    /// GPUI draws its own prompt when the platform offers none.
    fn prompt(
        &self,
        _level: PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        None
    }

    fn activate(&self) {
        self.window.activate();
    }

    fn is_active(&self) -> bool {
        self.window.state.borrow().is_active
    }

    fn visibility(&self) -> WindowVisibility {
        self.window.state.borrow().visibility
    }

    fn is_hovered(&self) -> bool {
        self.window.state.borrow().hovered
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        self.window.state.borrow().background_appearance
    }

    fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    fn get_title(&self) -> String {
        self.window.state.borrow().title.clone()
    }

    fn set_background_appearance(&self, background_appearance: WindowBackgroundAppearance) {
        self.window.set_background_appearance(background_appearance);
    }

    fn minimize(&self) {}

    fn zoom(&self) {}

    fn toggle_fullscreen(&self) {}

    fn is_fullscreen(&self) -> bool {
        self.window.is_full_screen()
    }

    // Holds the window weakly: GPUI keeps the waker in the window's
    // invalidator, which the window's own callbacks capture.
    fn frame_waker(&self) -> Option<Rc<dyn Fn()>> {
        let window = Rc::downgrade(&self.window);
        Some(Rc::new(move || {
            if let Some(window) = window.upgrade() {
                window.request_frame();
            }
        }))
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        register(&self.window.callbacks.request_frame, callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        register(&self.window.callbacks.input, callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        register(&self.window.callbacks.active_status, callback);
    }

    fn on_visibility_change(&self, callback: Box<dyn FnMut(WindowVisibility)>) {
        register(&self.window.callbacks.visibility, callback);
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        register(&self.window.callbacks.hover_status, callback);
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        register(&self.window.callbacks.resize, callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        register(&self.window.callbacks.moved, callback);
    }

    /// Kept but never asked: iPadOS closes scenes without consulting the app.
    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        register(&self.window.callbacks.should_close, callback);
    }

    /// Kept but never asked: iPadOS draws no window controls in the content.
    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        register(&self.window.callbacks.hit_test_window_control, callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        register(&self.window.callbacks.close, callback);
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        register(&self.window.callbacks.appearance_changed, callback);
    }

    fn draw(&self, scene: &Scene) {
        #[cfg(feature = "test-support")]
        super::retirement_probe::NativeCutCounter::Presentations.presented();
        if self.window.state.borrow().closed {
            return;
        }
        self.window.renderer.borrow_mut().draw(scene);
    }

    fn schedule_frame(&self) {
        self.window.request_frame();
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.window.renderer.borrow().sprite_atlas()
    }

    /// iOS renders text with grayscale antialiasing only.
    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        None
    }

    fn update_ime_position(&self, bounds: Bounds<Pixels>) {
        self.window
            .defer(move |window| window.text_input.caret_bounds_changed(bounds));
    }

    fn insets(&self) -> WindowInsets {
        self.window.state.borrow().insets.clone()
    }

    fn on_insets_changed(&self, callback: Box<dyn FnMut(WindowInsets)>) {
        register(&self.window.callbacks.insets_changed, callback);
    }

    fn show_soft_keyboard(&self) {
        #[cfg(feature = "test-support")]
        super::retirement_probe::NativeCutRefusal::KeyboardShown.record();
        self.window.show_keyboard();
    }

    fn hide_soft_keyboard(&self) {
        #[cfg(feature = "test-support")]
        super::retirement_probe::NativeCutRefusal::KeyboardHidden.record();
        self.window.hide_keyboard();
    }

    /// The text input view is first responder exactly while GPUI's focused
    /// element takes text, which GPUI reports here once per change. The input
    /// handler itself is taken and set again around every frame and event, so
    /// it does not say whether text input is focused.
    fn text_input_state_changed(&self, change: TextInputStateChange) {
        #[cfg(feature = "test-support")]
        super::retirement_probe::NativeCutRefusal::FocusChange.on_state(&change);
        match change {
            TextInputStateChange::FocusGained => self.window.set_text_input_focused(true),
            TextInputStateChange::FocusLost => self.window.set_text_input_focused(false),
            // GPUI sends neither today. After each frame the window compares
            // the handler's text state and tells the text input view of a
            // change (`text_state`), which covers both.
            TextInputStateChange::SelectionChanged | TextInputStateChange::ContentChanged => {}
        }
    }
}

impl HasWindowHandle for IosWindow {
    #[allow(
        unsafe_code,
        reason = "raw-window-handle borrows the handle from raw pointers"
    )]
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let view = object_pointer(&self.window.view).ok_or(HandleError::Unavailable)?;
        let mut handle = UiKitWindowHandle::new(view);
        handle.ui_view_controller = object_pointer(&self.window.controller);
        // SAFETY: this window retains the view and its view controller, so
        // both outlive the borrow of `self` the handle is tied to.
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::UiKit(handle)) })
    }
}

impl HasDisplayHandle for IosWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::uikit())
    }
}

fn object_pointer<T: Message>(object: &Retained<T>) -> Option<NonNull<c_void>> {
    NonNull::new(Retained::as_ptr(object).cast_mut().cast())
}
