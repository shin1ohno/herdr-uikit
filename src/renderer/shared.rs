//! Adapted from gpui-pre-apple 0.3.6 (zed@bcf6582), Apache-2.0; see NOTICE.md.
//! Source: metal_renderer.rs. Modified for UIKit, checked allocation, and iOS storage.
//! Device and immutable pipelines shared by every renderer in the process.
use super::*;

pub(super) struct SharedResources {
    pub(super) device: metal::Device,
    pub(super) _library: metal::Library,
    pub(super) command_queue: metal::CommandQueue,
    pub(super) unit_vertices: metal::Buffer,
    pub(super) paths_rasterization_pipeline_state: metal::RenderPipelineState,
    pub(super) path_sprites_pipeline_state: metal::RenderPipelineState,
    pub(super) shadows_pipeline_state: metal::RenderPipelineState,
    pub(super) quads_pipeline_state: metal::RenderPipelineState,
    pub(super) underlines_pipeline_state: metal::RenderPipelineState,
    pub(super) monochrome_sprites_pipeline_state: metal::RenderPipelineState,
    pub(super) polychrome_sprites_pipeline_state: metal::RenderPipelineState,
}

impl SharedResources {
    pub(super) fn get() -> Result<Arc<Self>> {
        static SHARED: Mutex<Option<Arc<SharedResources>>> = Mutex::new(None);
        let mut cached = SHARED.lock();
        if let Some(shared) = &*cached {
            return Ok(shared.clone());
        }
        let shared = Arc::new(Self::new()?);
        *cached = Some(shared.clone());
        Ok(shared)
    }

    fn new() -> Result<Self> {
        let device = metal::Device::system_default().ok_or(Error::NoMetalDevice)?;
        let library = device
            .new_library_with_source(SHADERS_SOURCE_FILE, &metal::CompileOptions::new())
            .map_err(|message| Error::ShaderLibrary { message })?;
        fn to_float2_bits(point: PointF) -> u64 {
            let mut output = point.y.to_bits() as u64;
            output <<= 32;
            output |= point.x.to_bits() as u64;
            output
        }

        let unit_vertices = [
            to_float2_bits(point(0., 0.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(0., 1.)),
            to_float2_bits(point(1., 0.)),
            to_float2_bits(point(1., 1.)),
        ];
        let unit_vertices = resources::buffer_with_data(&device, &unit_vertices)?;

        let paths_rasterization_pipeline_state = build_path_rasterization_pipeline_state(
            &device,
            &library,
            "paths_rasterization",
            "path_rasterization_vertex",
            "path_rasterization_fragment",
            MTLPixelFormat::BGRA8Unorm,
            PATH_SAMPLE_COUNT,
        )?;
        let path_sprites_pipeline_state = build_path_sprite_pipeline_state(
            &device,
            &library,
            "path_sprites",
            "path_sprite_vertex",
            "path_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let shadows_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "shadows",
            "shadow_vertex",
            "shadow_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let quads_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "quads",
            "quad_vertex",
            "quad_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let underlines_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "underlines",
            "underline_vertex",
            "underline_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let monochrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "monochrome_sprites",
            "monochrome_sprite_vertex",
            "monochrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let polychrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "polychrome_sprites",
            "polychrome_sprite_vertex",
            "polychrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
        )?;
        let command_queue = device.new_command_queue();
        Ok(Self {
            device,
            _library: library,
            command_queue,
            unit_vertices,
            paths_rasterization_pipeline_state,
            path_sprites_pipeline_state,
            shadows_pipeline_state,
            quads_pipeline_state,
            underlines_pipeline_state,
            monochrome_sprites_pipeline_state,
            polychrome_sprites_pipeline_state,
        })
    }
}
