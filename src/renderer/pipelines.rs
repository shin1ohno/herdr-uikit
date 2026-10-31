//! Adapted from gpui-pre-apple 0.3.6 (zed@bcf6582), Apache-2.0; see NOTICE.md.
//! Source: metal_renderer.rs. Modified for UIKit, checked allocation, and iOS storage.
use super::*;
pub(super) fn build_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: MTLPixelFormat,
) -> Result<metal::RenderPipelineState> {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .map_err(|message| Error::PipelineBuild {
            function: vertex_fn_name.into(),
            message,
        })?;
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .map_err(|message| Error::PipelineBuild {
            function: fragment_fn_name.into(),
            message,
        })?;

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor
        .color_attachments()
        .object_at(0)
        .ok_or(Error::ColorAttachmentMissing)?;
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::SourceAlpha);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::One);

    device
        .new_render_pipeline_state(&descriptor)
        .map_err(|message| Error::PipelineBuild {
            function: format!("{vertex_fn_name}/{fragment_fn_name}"),
            message,
        })
}

pub(super) fn build_path_sprite_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: MTLPixelFormat,
) -> Result<metal::RenderPipelineState> {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .map_err(|message| Error::PipelineBuild {
            function: vertex_fn_name.into(),
            message,
        })?;
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .map_err(|message| Error::PipelineBuild {
            function: fragment_fn_name.into(),
            message,
        })?;

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    let color_attachment = descriptor
        .color_attachments()
        .object_at(0)
        .ok_or(Error::ColorAttachmentMissing)?;
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::One);

    device
        .new_render_pipeline_state(&descriptor)
        .map_err(|message| Error::PipelineBuild {
            function: format!("{vertex_fn_name}/{fragment_fn_name}"),
            message,
        })
}

pub(super) fn build_path_rasterization_pipeline_state(
    device: &metal::DeviceRef,
    library: &metal::LibraryRef,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: MTLPixelFormat,
    path_sample_count: u32,
) -> Result<metal::RenderPipelineState> {
    let vertex_fn = library
        .get_function(vertex_fn_name, None)
        .map_err(|message| Error::PipelineBuild {
            function: vertex_fn_name.into(),
            message,
        })?;
    let fragment_fn = library
        .get_function(fragment_fn_name, None)
        .map_err(|message| Error::PipelineBuild {
            function: fragment_fn_name.into(),
            message,
        })?;

    let descriptor = metal::RenderPipelineDescriptor::new();
    descriptor.set_label(label);
    descriptor.set_vertex_function(Some(vertex_fn.as_ref()));
    descriptor.set_fragment_function(Some(fragment_fn.as_ref()));
    if path_sample_count > 1 {
        descriptor.set_raster_sample_count(path_sample_count as _);
        descriptor.set_alpha_to_coverage_enabled(false);
    }
    let color_attachment = descriptor
        .color_attachments()
        .object_at(0)
        .ok_or(Error::ColorAttachmentMissing)?;
    color_attachment.set_pixel_format(pixel_format);
    color_attachment.set_blending_enabled(true);
    color_attachment.set_rgb_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_alpha_blend_operation(metal::MTLBlendOperation::Add);
    color_attachment.set_source_rgb_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_source_alpha_blend_factor(metal::MTLBlendFactor::One);
    color_attachment.set_destination_rgb_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.set_destination_alpha_blend_factor(metal::MTLBlendFactor::OneMinusSourceAlpha);

    device
        .new_render_pipeline_state(&descriptor)
        .map_err(|message| Error::PipelineBuild {
            function: format!("{vertex_fn_name}/{fragment_fn_name}"),
            message,
        })
}
