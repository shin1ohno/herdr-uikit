//! Font resolution against iPadOS's real font set (simulator).

use std::borrow::Cow;

use anyhow::Context as _;
use core_text::font_descriptor::kCTFontItalicTrait;
use gpui::{FontStyle, FontWeight, PlatformTextSystem, font};

use super::family_of;
use crate::text_system::{IosTextSystem, family::SYSTEM_UI_FAMILY};

#[test]
fn system_ui_font_resolves_with_italics() -> anyhow::Result<()> {
    let system = IosTextSystem::new();
    let regular = system.font_id(&font(".SystemUIFont"))?;
    assert_eq!(family_of(&system, regular), SYSTEM_UI_FAMILY);

    let mut italic = font(".SystemUIFont");
    italic.style = FontStyle::Italic;
    let italic = system.font_id(&italic)?;
    assert_ne!(regular, italic);
    assert!(
        system.0.read().fonts[italic.0]
            .native_font()
            .symbolic_traits()
            & kCTFontItalicTrait
            != 0
    );
    Ok(())
}

#[test]
fn every_gpui_weight_gets_its_own_system_ui_weight() -> anyhow::Result<()> {
    let system = IosTextSystem::new();
    let regular = system.font_id(&font(".SystemUIFont"))?;
    let weights = [
        FontWeight::THIN,
        FontWeight::EXTRA_LIGHT,
        FontWeight::LIGHT,
        FontWeight::NORMAL,
        FontWeight::MEDIUM,
        FontWeight::SEMIBOLD,
        FontWeight::BOLD,
        FontWeight::EXTRA_BOLD,
        FontWeight::BLACK,
    ];
    let mut font_ids = Vec::with_capacity(weights.len());
    for weight in weights {
        let mut weighted = font(".SystemUIFont");
        weighted.weight = weight;
        font_ids.push(system.font_id(&weighted)?);
    }
    assert_eq!(font_ids[3], regular);

    // font-kit's readback rises with Core Text's weight, so a strictly rising
    // sequence means nine distinct fonts in `UIFont.Weight` order: medium is
    // not regular, and extra bold is heavy rather than black.
    let state = system.0.read();
    let read_back = font_ids
        .iter()
        .map(|font_id| {
            crate::text_system::font_properties::read(&state.fonts[font_id.0].native_font())
                .map(|properties| properties.weight.0)
                .unwrap_or(f32::NAN)
        })
        .collect::<Vec<_>>();
    assert!(
        read_back.windows(2).all(|pair| pair[0] < pair[1]),
        "{read_back:?}"
    );
    Ok(())
}

#[test]
fn mono_alias_falls_back_to_menlo_without_the_bundled_font() -> anyhow::Result<()> {
    let system = IosTextSystem::new();
    let font_id = system.font_id(&font(".ZedMono"))?;
    assert_eq!(family_of(&system, font_id), "Menlo");
    assert!(system.0.read().fonts[font_id.0].is_monospace());
    Ok(())
}

#[test]
fn sans_alias_falls_back_to_the_system_font() -> anyhow::Result<()> {
    let system = IosTextSystem::new();
    let font_id = system.font_id(&font(".ZedSans"))?;
    assert_eq!(family_of(&system, font_id), SYSTEM_UI_FAMILY);
    Ok(())
}

#[test]
fn installed_monospace_families_resolve_by_name() -> anyhow::Result<()> {
    let system = IosTextSystem::new();
    for family in ["Menlo", "Courier"] {
        let font_id = system.font_id(&font(family))?;
        assert_eq!(family_of(&system, font_id), family);
    }
    Ok(())
}

#[test]
fn unknown_families_fail_so_gpui_can_fall_back() {
    let system = IosTextSystem::new();
    assert!(system.font_id(&font("No Such Family 4c1f")).is_err());
}

#[test]
fn all_font_names_lists_installed_families() {
    let names = IosTextSystem::new().all_font_names();
    assert!(names.iter().any(|name| name == "Menlo"), "{names:?}");
    assert!(names.iter().any(|name| name == "Helvetica"), "{names:?}");
}

#[test]
fn embedded_fonts_load_from_owned_and_static_bytes() -> anyhow::Result<()> {
    // A single-font file: Core Graphics reads one face, not a collection.
    let courier_new = core_text::font::new_from_name("CourierNewPSMT", 12.0)
        .map_err(|()| anyhow::anyhow!("Courier New is not installed"))?;
    let path = courier_new
        .url()
        .and_then(|url| url.to_path())
        .context("Courier New has no file")?;
    let bytes = std::fs::read(path)?;

    let system = IosTextSystem::new();
    system.add_fonts(vec![Cow::Owned(bytes.clone())])?;
    let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    system.add_fonts(vec![Cow::Borrowed(leaked)])?;

    let families = system.0.read().memory_source.all_families()?;
    assert_eq!(families, ["Courier New"]);
    let font_id = system.font_id(&font("Courier New"))?;
    assert!(system.0.read().fonts[font_id.0].is_monospace());
    Ok(())
}

#[test]
fn bold_selection_picks_a_bold_face() -> anyhow::Result<()> {
    let system = IosTextSystem::new();
    let mut bold = font("Menlo");
    bold.weight = FontWeight::BOLD;
    let regular = system.font_id(&font("Menlo"))?;
    let bold = system.font_id(&bold)?;
    assert_ne!(regular, bold);
    assert!(
        crate::text_system::font_properties::read(&system.0.read().fonts[bold.0].native_font())
            .is_some_and(|properties| properties.weight.0 >= 600.0)
    );
    Ok(())
}
