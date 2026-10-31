//! Conversions between font-kit's and pathfinder's geometry and GPUI's.

use font_kit::{
    metrics::Metrics,
    properties::{Style as FontkitStyle, Weight as FontkitWeight},
};
use gpui::{Bounds, DevicePixels, FontMetrics, FontStyle, FontWeight, Point, Size, point, size};
use pathfinder_geometry::{
    rect::{RectF, RectI},
    vector::Vector2F,
};

/// The PostScript names Apple Color Emoji appears under, as `gpui-pre-macos`
/// matches them. Its glyphs are color bitmaps, rasterized as RGBA.
const EMOJI_POSTSCRIPT_NAMES: [&str; 2] = ["AppleColorEmoji", ".AppleColorEmojiUI"];

pub(super) fn font_kit_metrics_to_metrics(metrics: Metrics) -> FontMetrics {
    FontMetrics {
        units_per_em: metrics.units_per_em,
        ascent: metrics.ascent,
        descent: metrics.descent,
        line_gap: metrics.line_gap,
        underline_position: metrics.underline_position,
        underline_thickness: metrics.underline_thickness,
        cap_height: metrics.cap_height,
        x_height: metrics.x_height,
        bounding_box: bounds_from_rect(metrics.bounding_box),
    }
}

pub(super) fn bounds_from_rect(rect: RectF) -> Bounds<f32> {
    Bounds {
        origin: point(rect.origin_x(), rect.origin_y()),
        size: size(rect.width(), rect.height()),
    }
}

pub(super) fn bounds_from_rect_i(rect: RectI) -> Bounds<DevicePixels> {
    Bounds {
        origin: point(DevicePixels(rect.origin_x()), DevicePixels(rect.origin_y())),
        size: size(DevicePixels(rect.width()), DevicePixels(rect.height())),
    }
}

pub(super) fn size_from_vector2f(vec: Vector2F) -> Size<f32> {
    size(vec.x(), vec.y())
}

pub(super) fn fontkit_weight(value: FontWeight) -> FontkitWeight {
    FontkitWeight(value.0)
}

pub(super) fn fontkit_style(style: FontStyle) -> FontkitStyle {
    match style {
        FontStyle::Normal => FontkitStyle::Normal,
        FontStyle::Italic => FontkitStyle::Italic,
        FontStyle::Oblique => FontkitStyle::Oblique,
    }
}

pub(super) fn is_emoji_postscript_name(postscript_name: &str) -> bool {
    EMOJI_POSTSCRIPT_NAMES.contains(&postscript_name)
}

/// The bitmap a glyph is drawn into: its raster bounds, one pixel wider or
/// taller when it is shifted by a subpixel variant on that axis, so the shifted
/// antialiasing has room.
pub(super) fn bitmap_size(
    raster_bounds: Size<DevicePixels>,
    subpixel_variant: Point<u8>,
) -> Size<DevicePixels> {
    let mut bitmap_size = raster_bounds;
    if subpixel_variant.x > 0 {
        bitmap_size.width += DevicePixels(1);
    }
    if subpixel_variant.y > 0 {
        bitmap_size.height += DevicePixels(1);
    }
    bitmap_size
}

/// The bitmap's width and height as buffer dimensions, or `None` when either
/// is negative.
pub(super) fn bitmap_dimensions(bitmap_size: Size<DevicePixels>) -> Option<(usize, usize)> {
    let width = usize::try_from(bitmap_size.width.0).ok()?;
    let height = usize::try_from(bitmap_size.height.0).ok()?;
    Some((width, height))
}
