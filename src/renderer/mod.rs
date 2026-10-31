//! Adapted from gpui-pre-apple 0.3.6 (zed@bcf6582) metal_renderer.rs,
//! Apache-2.0; see NOTICE.md. Modified for UIKit and shared, checked Metal resources.
//!
//! Window contract: `new` → host the retained `layer_ptr` → call
//! `update_drawable_size` on every layout and scale change in device pixels
//! (points × scale) → `draw` per frame from the display link → `destroy` on close.
//! Subpixel sprites are unsupported: the window must return false from
//! `is_subpixel_rendering_supported()`.

// keep-sorted start
mod abi;
mod atlas;
mod buffers;
mod drawing;
mod error;
mod limits;
mod pipelines;
mod resources;
mod shared;
mod types;
// keep-sorted end

pub use error::Error;
type Result<T> = std::result::Result<T, Error>;
use atlas::MetalAtlas;
use buffers::*;
use foreign_types::ForeignType;
use gpui::{
    AtlasTextureId, Background, Bounds, DevicePixels, Path, PlatformAtlas, Point, PrimitiveBatch,
    ScaledPixels, Scene, Size, point, size,
};
use metal::MTLPixelFormat;
use parking_lot::Mutex;
use pipelines::*;
use std::{ffi::c_void, mem, ops::Range, sync::Arc};
use types::*;

const SHADERS_SOURCE_FILE: &str = include_str!(concat!(env!("OUT_DIR"), "/stitched_shaders.metal"));
const PATH_SAMPLE_COUNT: u32 = 4;
const INSTANCE_BUFFER_ALIGNMENT: usize = 256;
pub(crate) use buffers::InstanceBufferPool;
use limits::MAX_INSTANCE_BUFFER_SIZE;
pub(crate) type Context = Arc<Mutex<InstanceBufferPool>>;

pub(crate) struct Renderer {
    shared: Arc<shared::SharedResources>,
    layer: metal::MetalLayer,
    drawable_size: Size<DevicePixels>,
    presents_with_transaction: bool,
    opaque: bool,
    instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    sprite_atlas: Arc<MetalAtlas>,
    path_intermediate_texture: Option<metal::Texture>,
    path_intermediate_msaa_texture: Option<metal::Texture>,
}
impl Renderer {
    /// Creates the layer and runtime-compiled pipelines; UIKit hosts the retained layer.
    pub(crate) fn new(instance_buffer_pool: Context, transparent: bool) -> crate::Result<Self> {
        Self::new_inner(instance_buffer_pool, transparent).map_err(crate::Error::from)
    }

    fn new_inner(instance_buffer_pool: Context, transparent: bool) -> Result<Self> {
        let shared = instance_buffer_pool.lock().shared_resources()?;
        let layer = metal::MetalLayer::new();
        layer.set_device(&shared.device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        layer.set_opaque(!transparent);
        layer.set_maximum_drawable_count(3);
        configure_timeout(&layer);
        let sprite_atlas = Arc::new(MetalAtlas::new(shared.device.clone()));
        Ok(Self {
            shared,
            layer,
            drawable_size: size(DevicePixels(0), DevicePixels(0)),
            presents_with_transaction: false,
            opaque: !transparent,
            instance_buffer_pool,
            sprite_atlas,
            path_intermediate_texture: None,
            path_intermediate_msaa_texture: None,
        })
    }

    /// A borrowed pointer valid while this renderer lives.
    pub(crate) fn layer_ptr(&self) -> *mut c_void {
        self.layer.as_ptr().cast()
    }
    pub(crate) fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.sprite_atlas.clone()
    }
    /// Resize on every layout and scale change; size is device pixels (points × scale).
    pub(crate) fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        if self.drawable_size == size {
            return;
        }
        self.drawable_size = size;
        if size.width.0 <= 0 || size.height.0 <= 0 {
            self.path_intermediate_texture = None;
            self.path_intermediate_msaa_texture = None;
            return;
        }
        let mut drawable_size = self.layer.drawable_size();
        drawable_size.width = size.width.0 as f64;
        drawable_size.height = size.height.0 as f64;
        self.layer.set_drawable_size(drawable_size);
        if let Err(error) = self.update_path_intermediate_textures(size) {
            self.path_intermediate_texture = None;
            self.path_intermediate_msaa_texture = None;
            tracing::warn!(%error, "resizing Metal textures failed");
        }
    }

    fn update_path_intermediate_textures(&mut self, size: Size<DevicePixels>) -> Result<()> {
        let texture_descriptor = metal::TextureDescriptor::new();
        texture_descriptor.set_width(size.width.0 as u64);
        texture_descriptor.set_height(size.height.0 as u64);
        texture_descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        texture_descriptor.set_storage_mode(metal::MTLStorageMode::Private);
        texture_descriptor
            .set_usage(metal::MTLTextureUsage::RenderTarget | metal::MTLTextureUsage::ShaderRead);
        self.path_intermediate_texture = Some(resources::texture(
            &self.shared.device,
            &texture_descriptor,
        )?);

        if PATH_SAMPLE_COUNT > 1 {
            // Simulator supports only Private MSAA textures, even on Apple GPUs.
            #[cfg(target_env = "sim")]
            let storage_mode = metal::MTLStorageMode::Private;
            #[cfg(not(target_env = "sim"))]
            let storage_mode = metal::MTLStorageMode::Memoryless;

            let msaa_descriptor = texture_descriptor;
            msaa_descriptor.set_texture_type(metal::MTLTextureType::D2Multisample);
            msaa_descriptor.set_storage_mode(storage_mode);
            msaa_descriptor.set_sample_count(PATH_SAMPLE_COUNT as _);
            self.path_intermediate_msaa_texture =
                Some(resources::texture(&self.shared.device, &msaa_descriptor)?);
        } else {
            self.path_intermediate_msaa_texture = None;
        }
        Ok(())
    }

    /// Match layer presentation to a UIKit rotation or resize transaction.
    pub(crate) fn set_presents_with_transaction(&mut self, enabled: bool) {
        self.presents_with_transaction = enabled;
        self.layer.set_presents_with_transaction(enabled);
    }

    pub(crate) fn update_transparency(&mut self, transparent: bool) {
        self.opaque = !transparent;
        self.layer.set_opaque(!transparent);
    }

    /// Called when the window closes; queued GPU work retains its resources.
    pub(crate) fn destroy(&self) {
        // nothing to do
    }

    /// Draw once per frame from the display link.
    pub(crate) fn draw(&mut self, scene: &Scene) {
        if self.drawable_size.width.0 <= 0 || self.drawable_size.height.0 <= 0 {
            return;
        }
        let layer = self.layer.clone();
        let viewport_size = self.drawable_size;
        let drawable = if let Some(drawable) = layer.next_drawable() {
            drawable
        } else {
            tracing::error!(
                "failed to retrieve next drawable, drawable size: {:?}",
                viewport_size
            );
            return;
        };

        let command_buffer = match self.render_frame(scene, drawable.texture(), viewport_size) {
            Ok(command_buffer) => command_buffer,
            Err(error) => {
                tracing::warn!(%error, "failed to render");
                return;
            }
        };

        if self.presents_with_transaction {
            command_buffer.commit();
            command_buffer.wait_until_scheduled();
            drawable.present();
        } else {
            command_buffer.present_drawable(drawable);
            command_buffer.commit();
        }
    }
}

#[allow(unsafe_code)]
fn configure_timeout(layer: &metal::MetalLayerRef) {
    // SAFETY: layer is a live CAMetalLayer; this setter accepts Objective-C BOOL.
    // A missing drawable must time out rather than hang UIKit's main thread.
    unsafe {
        use foreign_types::ForeignTypeRef;
        let layer = &*layer.as_ptr().cast::<objc2::runtime::AnyObject>();
        let _: () = objc2::msg_send![layer, setAllowsNextDrawableTimeout: true];
    }
}

fn warn_subpixel() {
    static WARN: std::sync::Once = std::sync::Once::new();
    WARN.call_once(|| tracing::warn!("subpixel sprites are unsupported on iOS"));
}
