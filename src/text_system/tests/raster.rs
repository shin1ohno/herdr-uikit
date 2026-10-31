//! Glyph rasterization through Core Graphics (simulator).

use gpui::{DevicePixels, PlatformTextSystem, RenderGlyphParams, point, px};

use super::helvetica;

#[test]
fn glyphs_rasterize_to_coverage_of_their_bounds() -> anyhow::Result<()> {
    let (system, font_id) = helvetica()?;
    let glyph_id = system
        .glyph_for_char(font_id, 'm')
        .ok_or_else(|| anyhow::anyhow!("no glyph for 'm'"))?;
    let params = RenderGlyphParams {
        font_id,
        glyph_id,
        font_size: px(16.),
        subpixel_variant: point(1, 0),
        scale_factor: 2.0,
        is_emoji: false,
        subpixel_rendering: false,
        dilation: 0,
    };
    let bounds = system.glyph_raster_bounds(&params)?;
    assert!(bounds.size.width > DevicePixels(0) && bounds.size.height > DevicePixels(0));

    let (size, bytes) = system.rasterize_glyph(&params, bounds)?;
    assert_eq!(size.width, bounds.size.width + DevicePixels(1));
    assert_eq!(size.height, bounds.size.height);
    let width = usize::try_from(size.width.0)?;
    let height = usize::try_from(size.height.0)?;
    assert_eq!(bytes.len(), width * height);
    assert!(bytes.iter().any(|&coverage| coverage > 0));
    Ok(())
}

#[test]
fn empty_bounds_are_an_error() -> anyhow::Result<()> {
    let (system, font_id) = helvetica()?;
    let params = RenderGlyphParams {
        font_id,
        glyph_id: gpui::GlyphId(0),
        font_size: px(16.),
        subpixel_variant: point(0, 0),
        scale_factor: 1.0,
        is_emoji: false,
        subpixel_rendering: false,
        dilation: 0,
    };
    let empty = gpui::Bounds::default();
    assert!(system.rasterize_glyph(&params, empty).is_err());
    Ok(())
}
