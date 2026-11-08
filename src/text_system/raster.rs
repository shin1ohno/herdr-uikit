//! Glyph raster bounds and rasterization with Core Graphics, ported from
//! `gpui-pre-macos`'s `MacTextSystemState` (NOTICE.md).
//!
//! macOS dilates glyphs when the user's `AppleFontSmoothing` preference allows
//! it. iOS has no such preference, so `IosTextSystem` keeps GPUI's default
//! dilation of 0 and glyphs are always filled in black, without smoothing.

use core_graphics_text::{
    base::{CGFloat, CGGlyph, kCGImageAlphaPremultipliedLast},
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    geometry::CGPoint,
};
use font_kit::{canvas::RasterizationOptions, hinting::HintingOptions};
use gpui::{
    Bounds, DevicePixels, RenderGlyphParams, SUBPIXEL_VARIANTS_X, Size, swap_rgba_pa_to_bgra,
};
use pathfinder_geometry::transform2d::Transform2F;

use super::{
    IosTextSystemState,
    convert::{bitmap_dimensions, bitmap_size, bounds_from_rect_i},
    error::Error,
};

/// `kCGImageAlphaOnly`, which core-graphics 0.24 does not define.
#[allow(non_upper_case_globals, reason = "named as in Core Graphics")]
const kCGImageAlphaOnly: u32 = 7;

impl IosTextSystemState {
    pub(super) fn raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> Result<Bounds<DevicePixels>, Error> {
        let font = &self.fonts[params.font_id.0];
        let scale = Transform2F::from_scale(params.scale_factor);
        let bounds = bounds_from_rect_i(font.raster_bounds(
            params.glyph_id.0,
            params.font_size.into(),
            scale,
            HintingOptions::None,
            RasterizationOptions::GrayscaleAa,
        )?);

        // Expand the bounds by 1 pixel on each side to give CG room for
        // antialiasing.
        Ok(bounds.dilate(DevicePixels(1)))
    }

    pub(super) fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        glyph_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>), Error> {
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            return Err(Error::EmptyGlyphBounds);
        }
        let bitmap_size = bitmap_size(glyph_bounds.size, params.subpixel_variant);
        let (width, height) = bitmap_dimensions(bitmap_size).ok_or(Error::InvalidBitmapSize {
            width: bitmap_size.width.0,
            height: bitmap_size.height.0,
        })?;

        // Emoji are color bitmaps drawn as RGBA; every other glyph is coverage
        // only. `bytes` outlives `cx`, which `draw_glyphs` consumes.
        let bytes_per_pixel = if params.is_emoji { 4 } else { 1 };
        let mut bytes = vec![0; width * bytes_per_pixel * height];
        let cx = if params.is_emoji {
            CGContext::create_bitmap_context(
                Some(bytes.as_mut_ptr().cast()),
                width,
                height,
                8,
                width * 4,
                &CGColorSpace::create_device_rgb(),
                kCGImageAlphaPremultipliedLast,
            )
        } else {
            CGContext::create_bitmap_context(
                Some(bytes.as_mut_ptr().cast()),
                width,
                height,
                8,
                width,
                &CGColorSpace::create_device_gray(),
                kCGImageAlphaOnly,
            )
        };

        // Move the origin to bottom left and account for scaling, this makes
        // drawing text consistent with font-kit's raster_bounds.
        cx.translate(
            -glyph_bounds.origin.x.0 as CGFloat,
            (glyph_bounds.origin.y.0 + glyph_bounds.size.height.0) as CGFloat,
        );
        cx.scale(
            params.scale_factor as CGFloat,
            params.scale_factor as CGFloat,
        );

        let subpixel_shift = params
            .subpixel_variant
            .map(|v| v as f32 / SUBPIXEL_VARIANTS_X as f32);
        cx.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
        cx.set_allows_antialiasing(true);
        cx.set_should_antialias(true);
        cx.set_allows_font_subpixel_positioning(true);
        cx.set_should_subpixel_position_fonts(true);
        cx.set_allows_font_subpixel_quantization(false);
        cx.set_should_subpixel_quantize_fonts(false);
        cx.set_gray_fill_color(0.0, 1.0);

        self.fonts[params.font_id.0]
            .native_font()
            .clone_with_font_size(f32::from(params.font_size) as CGFloat)
            .draw_glyphs(
                &[params.glyph_id.0 as CGGlyph],
                &[CGPoint::new(
                    (subpixel_shift.x / params.scale_factor) as CGFloat,
                    (subpixel_shift.y / params.scale_factor) as CGFloat,
                )],
                cx,
            );

        if params.is_emoji {
            // Convert from RGBA with premultiplied alpha to BGRA with straight
            // alpha.
            for pixel in bytes.as_chunks_mut::<4>().0 {
                swap_rgba_pa_to_bgra(pixel);
            }
        }

        Ok((bitmap_size, bytes))
    }
}
