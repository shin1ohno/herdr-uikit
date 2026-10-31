//! The Metal renderer, ported from `gpui-pre-apple`'s `metal_renderer` and
//! `metal_atlas` (NOTICE.md). It owns a `CAMetalLayer`; the window adds that
//! layer to its view and keeps its frame and scale in step with the view.
//!
//! Contract for the window (Wave 0, docs/ios.md): construct with [`Renderer::new`],
//! host [`Renderer::layer_ptr`], call [`Renderer::update_drawable_size`] on
//! every layout and scale change, [`Renderer::draw`] once per frame from the
//! display link, and [`Renderer::destroy`] when the window closes.

// Wave 0 skeleton: the renderer stream replaces these bodies.
#![allow(dead_code, clippy::todo)]

use std::{ffi::c_void, sync::Arc};

use gpui::{DevicePixels, PlatformAtlas, Scene, Size};
use parking_lot::Mutex;

/// Instance buffers shared by every window's renderer, as on macOS.
#[derive(Default)]
pub(crate) struct InstanceBufferPool {}

/// What the platform hands each new window's renderer.
pub(crate) type Context = Arc<Mutex<InstanceBufferPool>>;

pub(crate) struct Renderer {}

impl Renderer {
    /// Creates the Metal device, command queue, pipelines, and a
    /// `CAMetalLayer` with `opaque = !transparent`.
    pub(crate) fn new(_context: Context, _transparent: bool) -> crate::Result<Self> {
        todo!("renderer stream")
    }

    /// The `CAMetalLayer`, retained for the renderer's lifetime. The window
    /// adds it as a sublayer of its view's layer.
    pub(crate) fn layer_ptr(&self) -> *mut c_void {
        todo!("renderer stream")
    }

    pub(crate) fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        todo!("renderer stream")
    }

    /// Resizes the drawable; `size` is in device pixels (points × scale).
    pub(crate) fn update_drawable_size(&mut self, _size: Size<DevicePixels>) {
        todo!("renderer stream")
    }

    pub(crate) fn update_transparency(&mut self, _transparent: bool) {
        todo!("renderer stream")
    }

    pub(crate) fn draw(&mut self, _scene: &Scene) {
        todo!("renderer stream")
    }

    pub(crate) fn destroy(&self) {
        todo!("renderer stream")
    }
}
