//! Tests for the Core Text text system. `convert`, `family`, and
//! `index_converter` cover pure helpers; `fonts`, `layout`, and `raster` load
//! real fonts through Core Text, so they run in the iOS simulator.

mod convert;
mod family;
mod fonts;
mod index_converter;
mod layout;
mod raster;

use gpui::{FontId, PlatformTextSystem, font};

use super::IosTextSystem;

/// A fresh text system and the id of Helvetica, which iPadOS ships.
fn helvetica() -> anyhow::Result<(IosTextSystem, FontId)> {
    let system = IosTextSystem::new();
    let font_id = system.font_id(&font("Helvetica"))?;
    Ok((system, font_id))
}

/// The family Core Text reports for a font this system loaded.
fn family_of(system: &IosTextSystem, font_id: FontId) -> String {
    system.0.read().fonts[font_id.0].family_name()
}
