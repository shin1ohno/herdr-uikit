//! Shaping a line with Core Text, ported from `gpui-pre-macos`'s
//! `MacTextSystemState::layout_line` (NOTICE.md). Where upstream unwraps a
//! run's attributes, font, or string index, a malformed run or glyph is
//! skipped instead.

#![allow(unsafe_code, reason = "Core Text's font attribute key is a C constant")]

use core_foundation::{
    attributed_string::CFMutableAttributedString,
    base::{CFRange, CFType, TCFType},
    dictionary::CFDictionary,
    string::CFString,
};
use core_text::{font::CTFont, line::CTLine, string_attributes::kCTFontAttributeName};
use gpui::{FontRun, GlyphId, LineLayout, Pixels, ShapedGlyph, ShapedRun, point, px};

use super::IosTextSystemState;

impl IosTextSystemState {
    pub(super) fn layout_line(
        &mut self,
        text: &str,
        font_size: Pixels,
        font_runs: &[FontRun],
    ) -> LineLayout {
        // Construct the attributed string, converting UTF8 ranges to UTF16
        // ranges.
        let mut string = CFMutableAttributedString::new();
        let mut max_ascent = 0.0f32;
        let mut max_descent = 0.0f32;
        {
            let mut text = text;
            let mut break_ligature = true;
            for run in font_runs {
                let Some((text_run, rest)) = text.split_at_checked(run.len) else {
                    tracing::warn!(
                        run_len = run.len,
                        remaining = text.len(),
                        "font run does not end on a character boundary of the line; truncating"
                    );
                    break;
                };
                text = rest;

                let utf16_start = string.char_len(); // insert at end of string
                // `replace_str` may silently drop codepoints it dislikes, such
                // as a byte order mark at the start of the string.
                string.replace_str(&CFString::new(text_run), CFRange::init(utf16_start, 0));
                let utf16_end = string.char_len();

                let cf_range = CFRange::init(utf16_start, utf16_end - utf16_start);
                let font = &self.fonts[run.font_id.0];

                let font_metrics = font.metrics();
                let font_scale = f32::from(font_size) / font_metrics.units_per_em as f32;
                max_ascent = max_ascent.max(font_metrics.ascent * font_scale);
                max_descent = max_descent.max(-font_metrics.descent * font_scale);

                // Alternating a size one ulp apart keeps Core Text from forming
                // ligatures across adjacent runs of the same font.
                let font_size = if break_ligature {
                    px(f32::from(font_size).next_up())
                } else {
                    font_size
                };
                // SAFETY: `kCTFontAttributeName` is an immutable CFString
                // constant Core Text exports.
                let font_attribute = unsafe { kCTFontAttributeName };
                string.set_attribute(
                    cf_range,
                    font_attribute,
                    &font.native_font().clone_with_font_size(font_size.into()),
                );
                break_ligature = !break_ligature;
            }
        }

        // Retrieve the glyphs from the shaped line, converting UTF16 offsets to
        // UTF8 offsets.
        let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
        let glyph_runs = line.glyph_runs();
        let mut runs = Vec::<ShapedRun>::with_capacity(glyph_runs.len().try_into().unwrap_or(0));
        let mut ix_converter = StringIndexConverter::new(text);
        for run in glyph_runs.iter() {
            let Some(font) = run.attributes().as_ref().and_then(run_font) else {
                tracing::warn!("shaped run has no font; skipping its glyphs");
                continue;
            };
            let font_id = self.id_for_native_font(&font);
            let is_emoji = self.is_emoji(font_id);

            if runs.last().is_none_or(|last| last.font_id != font_id) {
                runs.push(ShapedRun {
                    font_id,
                    glyphs: Vec::with_capacity(run.glyph_count().try_into().unwrap_or(0)),
                });
            }
            let Some(shaped) = runs.last_mut() else {
                continue;
            };
            for ((&glyph_id, position), &glyph_utf16_ix) in run
                .glyphs()
                .iter()
                .zip(run.positions().iter())
                .zip(run.string_indices().iter())
            {
                let Ok(glyph_utf16_ix) = usize::try_from(glyph_utf16_ix) else {
                    continue;
                };
                if ix_converter.utf16_ix > glyph_utf16_ix {
                    // The converter only seeks forward; restart the search.
                    ix_converter = StringIndexConverter::new(text);
                }
                ix_converter.advance_to_utf16_ix(glyph_utf16_ix);
                shaped.glyphs.push(ShapedGlyph {
                    id: GlyphId(u32::from(glyph_id)),
                    position: point(position.x as f32, position.y as f32).map(px),
                    index: ix_converter.utf8_ix,
                    is_emoji,
                });
            }
        }
        let typographic_bounds = line.get_typographic_bounds();
        LineLayout {
            runs,
            font_size,
            width: typographic_bounds.width.into(),
            ascent: max_ascent.into(),
            descent: max_descent.into(),
            len: text.len(),
        }
    }
}

/// The font Core Text shaped a run with.
fn run_font(attributes: &CFDictionary<CFString, CFType>) -> Option<CTFont> {
    // SAFETY: `kCTFontAttributeName` is an immutable CFString constant Core
    // Text exports.
    let font_attribute = unsafe { kCTFontAttributeName };
    attributes.find(font_attribute)?.downcast::<CTFont>()
}

/// Maps UTF-16 offsets, which Core Text reports, to UTF-8 offsets into the same
/// text, seeking forward only.
#[derive(Debug, Clone)]
pub(super) struct StringIndexConverter<'a> {
    text: &'a str,
    /// Index in UTF-8 bytes
    pub(super) utf8_ix: usize,
    /// Index in UTF-16 code units
    pub(super) utf16_ix: usize,
}

impl<'a> StringIndexConverter<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        Self {
            text,
            utf8_ix: 0,
            utf16_ix: 0,
        }
    }

    pub(super) fn advance_to_utf16_ix(&mut self, utf16_target: usize) {
        for (ix, c) in self.text[self.utf8_ix..].char_indices() {
            if self.utf16_ix >= utf16_target {
                self.utf8_ix += ix;
                return;
            }
            self.utf16_ix += c.len_utf16();
        }
        self.utf8_ix = self.text.len();
    }
}
