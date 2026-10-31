//! Core Text shaping and rasterization, ported from `gpui-pre-macos`'s
//! `text_system` and `open_type` (NOTICE.md). Core Text, Core Graphics, and
//! zed-font-kit's Core Text loader are the same on iOS as on macOS; what
//! differs is the system font set and that AppKit types are unavailable.
//!
//! - `fonts`: loading the app's fonts, families, and Core Text's substitutes.
//! - `family`: what a requested family name means on iPadOS, including the
//!   system UI font and fallbacks for GPUI's own aliases.
//! - `system_font`: the system UI family from Core Text's UI font API.
//! - `open_type`: OpenType features and fallback fonts.
//! - `layout`, `raster`: shaping a line, and a glyph's bounds and bitmap.
//!
//! Core Text and Core Graphics objects are thread-safe, so GPUI may call this
//! system from any thread; it needs no `MainThreadMarker`.

mod convert;
mod error;
mod family;
mod fonts;
mod layout;
mod open_type;
mod raster;
mod system_font;
#[cfg(test)]
mod tests;

use std::{borrow::Cow, collections::HashMap};

use font_kit::{
    font::Font as FontKitFont, properties::Properties, source::SystemSource,
    sources::mem::MemSource,
};
use gpui::{
    Bounds, DevicePixels, Font, FontFallbacks, FontFeatures, FontId, FontMetrics, FontRun, GlyphId,
    LineLayout, Pixels, PlatformTextSystem, RenderGlyphParams, Result, SharedString, Size,
    TextRenderingMode,
};
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use smallvec::SmallVec;

pub(crate) use self::error::Error;
use self::{
    convert::{
        bounds_from_rect, font_kit_metrics_to_metrics, fontkit_style, fontkit_weight,
        size_from_vector2f,
    },
    fonts::Candidate,
};

/// iPadOS text system using Core Text for font shaping.
pub(crate) struct IosTextSystem(RwLock<IosTextSystemState>);

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontKey {
    font_family: SharedString,
    font_features: FontFeatures,
    font_fallbacks: Option<FontFallbacks>,
}

struct IosTextSystemState {
    memory_source: MemSource,
    system_source: SystemSource,
    /// Append-only: a `FontId` is an index into this list, minted here and
    /// never invalidated, so indexing with one this system returned is in
    /// bounds.
    fonts: Vec<FontKitFont>,
    font_selections: HashMap<Font, FontId>,
    font_ids_by_postscript_name: HashMap<String, FontId>,
    candidates_by_font_key: HashMap<FontKey, SmallVec<[Candidate; 4]>>,
    postscript_names_by_font_id: HashMap<FontId, String>,
}

impl IosTextSystem {
    pub(crate) fn new() -> Self {
        Self(RwLock::new(IosTextSystemState {
            memory_source: MemSource::empty(),
            system_source: SystemSource::new(),
            fonts: Vec::new(),
            font_selections: HashMap::default(),
            font_ids_by_postscript_name: HashMap::default(),
            candidates_by_font_key: HashMap::default(),
            postscript_names_by_font_id: HashMap::default(),
        }))
    }
}

impl PlatformTextSystem for IosTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        Ok(self.0.write().add_fonts(fonts)?)
    }

    fn all_font_names(&self) -> Vec<String> {
        self.0.read().all_font_names()
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let lock = self.0.upgradable_read();
        if let Some(font_id) = lock.font_selections.get(font) {
            return Ok(*font_id);
        }
        let mut lock = RwLockUpgradableReadGuard::upgrade(lock);
        Ok(lock.select_font(font)?)
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        font_kit_metrics_to_metrics(self.0.read().fonts[font_id.0].metrics())
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        let rect = self.0.read().fonts[font_id.0]
            .typographic_bounds(glyph_id.0)
            .map_err(Error::from)?;
        Ok(bounds_from_rect(rect))
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        let advance = self.0.read().fonts[font_id.0]
            .advance(glyph_id.0)
            .map_err(Error::from)?;
        Ok(size_from_vector2f(advance))
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.read().fonts[font_id.0]
            .glyph_for_char(ch)
            .map(GlyphId)
    }

    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        Ok(self.0.read().raster_bounds(params)?)
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        Ok(self.0.read().rasterize_glyph(params, raster_bounds)?)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        self.0.write().layout_line(text, font_size, font_runs)
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        TextRenderingMode::Grayscale
    }
}

impl IosTextSystemState {
    /// Loads `font`'s family on first use, picks the best match for its weight
    /// and style, and remembers the choice.
    fn select_font(&mut self, font: &Font) -> Result<FontId, Error> {
        let font_key = FontKey {
            font_family: font.family.clone(),
            font_features: font.features.clone(),
            font_fallbacks: font.fallbacks.clone(),
        };
        let candidates = match self.candidates_by_font_key.get(&font_key) {
            Some(candidates) => candidates.clone(),
            None => {
                let candidates =
                    self.load_family(&font.family, &font.features, font.fallbacks.as_ref())?;
                self.candidates_by_font_key
                    .insert(font_key, candidates.clone());
                candidates
            }
        };

        let candidate_properties = candidates
            .iter()
            .map(|candidate| candidate.properties)
            .collect::<SmallVec<[_; 4]>>();
        let ix = font_kit::matching::find_best_match(
            &candidate_properties,
            &Properties {
                style: fontkit_style(font.style),
                weight: fontkit_weight(font.weight),
                stretch: Default::default(),
            },
        )?;
        let font_id = candidates
            .get(ix)
            .map(|candidate| candidate.font_id)
            .ok_or(font_kit::error::SelectionError::NotFound)?;
        self.font_selections.insert(font.clone(), font_id);
        Ok(font_id)
    }
}
