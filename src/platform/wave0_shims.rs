//! Wave 0 integration shims, not production code. Delete each block below
//! when the stream named on it lands, and this file and its `mod` line with
//! the last one.
//!
//! The platform hands GPUI `IosDispatcher`, `IosTextSystem`, `IosDisplay`,
//! and `IosWindow` as trait objects, and the Wave 0 skeletons of those types
//! do not implement GPUI's traits yet, so the platform cannot build on its
//! own without these. Each impl here conflicts (E0119) with the real one, so
//! a merge cannot keep a shim by accident, and every method panics, so
//! nothing built from this file can appear to work.

use std::{borrow::Cow, rc::Rc, sync::Arc, time::Duration};

use futures_channel::oneshot;
use gpui::{
    Bounds, Capslock, DevicePixels, DispatchEventResult, DisplayId, Font, FontId, FontMetrics,
    FontRun, GlyphId, GpuSpecs, LineLayout, Modifiers, Pixels, PlatformAtlas, PlatformDispatcher,
    PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformTextSystem, PlatformWindow,
    Point, Priority, PromptButton, PromptLevel, RenderGlyphParams, RequestFrameOptions,
    RunnableVariant, Scene, Size, TextRenderingMode, WindowAppearance, WindowBackgroundAppearance,
    WindowBounds, WindowControlArea, WindowVisibility,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use uuid::Uuid;

use crate::{
    dispatcher::IosDispatcher, display::IosDisplay, text_system::IosTextSystem, window::IosWindow,
};

macro_rules! shim {
    ($stream:literal) => {
        unimplemented!(concat!(
            "Wave 0 shim: the ",
            $stream,
            " stream implements this"
        ))
    };
}

// Delete when the runtime stream lands.
impl PlatformDispatcher for IosDispatcher {
    fn is_main_thread(&self) -> bool {
        shim!("runtime")
    }

    fn dispatch(&self, _runnable: RunnableVariant, _priority: Priority) {
        shim!("runtime")
    }

    fn dispatch_on_main_thread(&self, _runnable: RunnableVariant, _priority: Priority) {
        shim!("runtime")
    }

    fn dispatch_after(&self, _duration: Duration, _runnable: RunnableVariant) {
        shim!("runtime")
    }

    fn spawn_realtime(&self, _f: Box<dyn FnOnce() + Send>) {
        shim!("runtime")
    }
}

// Delete when the runtime stream lands.
impl PlatformDisplay for IosDisplay {
    fn id(&self) -> DisplayId {
        shim!("runtime")
    }

    fn uuid(&self) -> anyhow::Result<Uuid> {
        shim!("runtime")
    }

    fn bounds(&self) -> Bounds<Pixels> {
        shim!("runtime")
    }
}

// Delete when the text stream lands.
impl PlatformTextSystem for IosTextSystem {
    fn add_fonts(&self, _fonts: Vec<Cow<'static, [u8]>>) -> anyhow::Result<()> {
        shim!("text")
    }

    fn all_font_names(&self) -> Vec<String> {
        shim!("text")
    }

    fn font_id(&self, _descriptor: &Font) -> anyhow::Result<FontId> {
        shim!("text")
    }

    fn font_metrics(&self, _font_id: FontId) -> FontMetrics {
        shim!("text")
    }

    fn typographic_bounds(
        &self,
        _font_id: FontId,
        _glyph_id: GlyphId,
    ) -> anyhow::Result<Bounds<f32>> {
        shim!("text")
    }

    fn advance(&self, _font_id: FontId, _glyph_id: GlyphId) -> anyhow::Result<Size<f32>> {
        shim!("text")
    }

    fn glyph_for_char(&self, _font_id: FontId, _ch: char) -> Option<GlyphId> {
        shim!("text")
    }

    fn glyph_raster_bounds(
        &self,
        _params: &RenderGlyphParams,
    ) -> anyhow::Result<Bounds<DevicePixels>> {
        shim!("text")
    }

    fn rasterize_glyph(
        &self,
        _params: &RenderGlyphParams,
        _raster_bounds: Bounds<DevicePixels>,
    ) -> anyhow::Result<(Size<DevicePixels>, Vec<u8>)> {
        shim!("text")
    }

    fn layout_line(&self, _text: &str, _font_size: Pixels, _runs: &[FontRun]) -> LineLayout {
        shim!("text")
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        shim!("text")
    }
}

// Delete when the window stream lands.
impl HasWindowHandle for IosWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        shim!("window")
    }
}

// Delete when the window stream lands.
impl HasDisplayHandle for IosWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        shim!("window")
    }
}

// Delete when the window stream lands.
impl PlatformWindow for IosWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        shim!("window")
    }

    fn is_maximized(&self) -> bool {
        shim!("window")
    }

    fn window_bounds(&self) -> WindowBounds {
        shim!("window")
    }

    fn content_size(&self) -> Size<Pixels> {
        shim!("window")
    }

    fn resize(&mut self, _size: Size<Pixels>) {
        shim!("window")
    }

    fn scale_factor(&self) -> f32 {
        shim!("window")
    }

    fn appearance(&self) -> WindowAppearance {
        shim!("window")
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        shim!("window")
    }

    fn mouse_position(&self) -> Point<Pixels> {
        shim!("window")
    }

    fn modifiers(&self) -> Modifiers {
        shim!("window")
    }

    fn capslock(&self) -> Capslock {
        shim!("window")
    }

    fn set_input_handler(&mut self, _input_handler: PlatformInputHandler) {
        shim!("window")
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        shim!("window")
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        shim!("window")
    }

    fn activate(&self) {
        shim!("window")
    }

    fn is_active(&self) -> bool {
        shim!("window")
    }

    fn visibility(&self) -> WindowVisibility {
        shim!("window")
    }

    fn is_hovered(&self) -> bool {
        shim!("window")
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        shim!("window")
    }

    fn set_title(&mut self, _title: &str) {
        shim!("window")
    }

    fn set_background_appearance(&self, _background_appearance: WindowBackgroundAppearance) {
        shim!("window")
    }

    fn minimize(&self) {
        shim!("window")
    }

    fn zoom(&self) {
        shim!("window")
    }

    fn toggle_fullscreen(&self) {
        shim!("window")
    }

    fn is_fullscreen(&self) -> bool {
        shim!("window")
    }

    fn on_request_frame(&self, _callback: Box<dyn FnMut(RequestFrameOptions)>) {
        shim!("window")
    }

    fn on_input(&self, _callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        shim!("window")
    }

    fn on_active_status_change(&self, _callback: Box<dyn FnMut(bool)>) {
        shim!("window")
    }

    fn on_visibility_change(&self, _callback: Box<dyn FnMut(WindowVisibility)>) {
        shim!("window")
    }

    fn on_hover_status_change(&self, _callback: Box<dyn FnMut(bool)>) {
        shim!("window")
    }

    fn on_resize(&self, _callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        shim!("window")
    }

    fn on_moved(&self, _callback: Box<dyn FnMut()>) {
        shim!("window")
    }

    fn on_should_close(&self, _callback: Box<dyn FnMut() -> bool>) {
        shim!("window")
    }

    fn on_hit_test_window_control(&self, _callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        shim!("window")
    }

    fn on_close(&self, _callback: Box<dyn FnOnce()>) {
        shim!("window")
    }

    fn on_appearance_changed(&self, _callback: Box<dyn FnMut()>) {
        shim!("window")
    }

    fn draw(&self, _scene: &Scene) {
        shim!("window")
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        shim!("window")
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        shim!("window")
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        shim!("window")
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {
        shim!("window")
    }
}
