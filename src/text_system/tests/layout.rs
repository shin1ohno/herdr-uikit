//! Line layout through Core Text (simulator), ported from `gpui-pre-macos`'s
//! text system tests. Glyph ids come from the font rather than macOS's
//! Helvetica glyph table, which iPadOS's copy need not share.

use gpui::{FontRun, GlyphId, LineLayout, PlatformTextSystem, font, px};

use super::helvetica;
use crate::text_system::IosTextSystem;

fn assert_indices_in_bounds(layout: &LineLayout, text: &str) {
    for glyph in layout.runs.iter().flat_map(|run| &run.glyphs) {
        assert!(
            glyph.index < text.len(),
            "glyph index {} is out of bounds for text length {}",
            glyph.index,
            text.len()
        );
    }
}

fn glyph(system: &IosTextSystem, font_id: gpui::FontId, ch: char) -> anyhow::Result<GlyphId> {
    system
        .glyph_for_char(font_id, ch)
        .ok_or_else(|| anyhow::anyhow!("no glyph for {ch:?}"))
}

#[test]
fn byte_order_marks_produce_no_glyphs() -> anyhow::Result<()> {
    let (system, font_id) = helvetica()?;
    let (a, b) = (glyph(&system, font_id, 'a')?, glyph(&system, font_id, 'b')?);

    let line = "\u{feff}";
    let layout = system.layout_line(
        line,
        px(16.),
        &[FontRun {
            font_id,
            len: line.len(),
        }],
    );
    assert_eq!(layout.len, line.len());
    assert!(layout.runs.is_empty());

    let line = "a\u{feff}b";
    let layout = system.layout_line(
        line,
        px(16.),
        &[FontRun {
            font_id,
            len: line.len(),
        }],
    );
    assert_eq!(layout.len, line.len());
    assert_eq!(layout.runs.len(), 1);
    let ids: Vec<_> = layout.runs[0].glyphs.iter().map(|glyph| glyph.id).collect();
    assert_eq!(ids, [a, b]);

    let line = "\u{feff}ab";
    let runs = [
        FontRun {
            len: "\u{feff}".len(),
            font_id,
        },
        FontRun {
            len: "ab".len(),
            font_id,
        },
    ];
    let layout = system.layout_line(line, px(16.), &runs);
    assert_eq!(layout.len, line.len());
    assert_eq!(layout.runs.len(), 1);
    let ids: Vec<_> = layout.runs[0].glyphs.iter().map(|glyph| glyph.id).collect();
    assert_eq!(ids, [a, b]);
    Ok(())
}

#[test]
fn adjacent_runs_keep_glyph_indices_in_bounds() -> anyhow::Result<()> {
    let (system, font_id) = helvetica()?;
    let text = "hello world";
    let runs = [FontRun { font_id, len: 5 }, FontRun { font_id, len: 6 }];
    let layout = system.layout_line(text, px(16.), &runs);
    assert_eq!(layout.len, text.len());
    assert_indices_in_bounds(&layout, text);

    let other = system.font_id(&font("Courier")).unwrap_or(font_id);
    let runs = [
        FontRun { font_id, len: 5 },
        FontRun {
            font_id: other,
            len: 6,
        },
    ];
    let layout = system.layout_line(text, px(16.), &runs);
    assert_eq!(layout.len, text.len());
    assert_indices_in_bounds(&layout, text);
    Ok(())
}

#[test]
fn single_character_runs_and_empty_text() -> anyhow::Result<()> {
    let (system, font_id) = helvetica()?;
    let text = "abc";
    let runs = [FontRun { font_id, len: 1 }; 3];
    let layout = system.layout_line(text, px(16.), &runs);
    assert_eq!(layout.len, text.len());
    assert_indices_in_bounds(&layout, text);

    let layout = system.layout_line("", px(16.), &[]);
    assert_eq!(layout.len, 0);
    assert!(layout.runs.is_empty());
    Ok(())
}

#[test]
fn runs_off_a_character_boundary_are_truncated_without_panicking() -> anyhow::Result<()> {
    let (system, font_id) = helvetica()?;
    let text = "é";
    let layout = system.layout_line(text, px(16.), &[FontRun { font_id, len: 1 }]);
    assert_eq!(layout.len, text.len());
    assert!(layout.runs.is_empty());
    Ok(())
}

#[test]
fn emoji_substituted_by_the_cascade_are_marked_emoji() -> anyhow::Result<()> {
    let (system, font_id) = helvetica()?;
    let text = "a😀";
    let layout = system.layout_line(
        text,
        px(16.),
        &[FontRun {
            font_id,
            len: text.len(),
        }],
    );
    let emoji = layout
        .runs
        .iter()
        .flat_map(|run| &run.glyphs)
        .find(|glyph| glyph.index == 1)
        .ok_or_else(|| anyhow::anyhow!("no glyph for the emoji"))?;
    assert!(emoji.is_emoji);
    assert!(layout.runs.iter().any(|run| run.font_id != font_id));
    Ok(())
}
