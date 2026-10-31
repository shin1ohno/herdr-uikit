use font_kit::properties::{Style as FontkitStyle, Weight as FontkitWeight};
use gpui::{DevicePixels, FontStyle, FontWeight, point, size};
use pathfinder_geometry::{
    rect::{RectF, RectI},
    vector::{Vector2F, Vector2I},
};

use crate::text_system::convert::{
    bitmap_dimensions, bitmap_size, bounds_from_rect, bounds_from_rect_i, fontkit_style,
    fontkit_weight, is_emoji_postscript_name, size_from_vector2f,
};

#[test]
fn styles_map_one_to_one() {
    assert_eq!(fontkit_style(FontStyle::Normal), FontkitStyle::Normal);
    assert_eq!(fontkit_style(FontStyle::Italic), FontkitStyle::Italic);
    assert_eq!(fontkit_style(FontStyle::Oblique), FontkitStyle::Oblique);
}

#[test]
fn weights_keep_their_css_value() {
    for weight in [
        FontWeight::THIN,
        FontWeight::NORMAL,
        FontWeight::SEMIBOLD,
        FontWeight::BLACK,
    ] {
        assert_eq!(fontkit_weight(weight), FontkitWeight(weight.0));
    }
}

#[test]
fn rects_become_bounds_with_the_same_origin_and_size() {
    let rect = RectF::new(Vector2F::new(1.5, -2.0), Vector2F::new(3.0, 4.25));
    let bounds = bounds_from_rect(rect);
    assert_eq!(bounds.origin, point(1.5, -2.0));
    assert_eq!(bounds.size, size(3.0, 4.25));

    let rect = RectI::new(Vector2I::new(-1, 2), Vector2I::new(5, 7));
    let bounds = bounds_from_rect_i(rect);
    assert_eq!(bounds.origin, point(DevicePixels(-1), DevicePixels(2)));
    assert_eq!(bounds.size, size(DevicePixels(5), DevicePixels(7)));

    assert_eq!(size_from_vector2f(Vector2F::new(8.0, 0.5)), size(8.0, 0.5));
}

#[test]
fn bitmaps_grow_by_a_pixel_on_each_shifted_axis() {
    let raster = size(DevicePixels(10), DevicePixels(12));
    assert_eq!(bitmap_size(raster, point(0, 0)), raster);
    assert_eq!(
        bitmap_size(raster, point(2, 0)),
        size(DevicePixels(11), DevicePixels(12))
    );
    assert_eq!(
        bitmap_size(raster, point(0, 1)),
        size(DevicePixels(10), DevicePixels(13))
    );
    assert_eq!(
        bitmap_size(raster, point(3, 1)),
        size(DevicePixels(11), DevicePixels(13))
    );
}

#[test]
fn bitmap_dimensions_refuse_negative_sizes() {
    assert_eq!(
        bitmap_dimensions(size(DevicePixels(3), DevicePixels(4))),
        Some((3, 4))
    );
    assert_eq!(
        bitmap_dimensions(size(DevicePixels(-1), DevicePixels(4))),
        None
    );
    assert_eq!(
        bitmap_dimensions(size(DevicePixels(3), DevicePixels(-4))),
        None
    );
}

#[test]
fn only_apple_color_emoji_is_emoji() {
    assert!(is_emoji_postscript_name("AppleColorEmoji"));
    assert!(is_emoji_postscript_name(".AppleColorEmojiUI"));
    assert!(!is_emoji_postscript_name("Menlo-Regular"));
    assert!(!is_emoji_postscript_name("applecoloremoji"));
}
