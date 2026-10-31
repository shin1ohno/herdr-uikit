//! Which fonts a requested family name means on iPadOS.
//!
//! macOS looks every family up by name, including the hidden system UI
//! family. Core Text restricts name lookup of `.`-prefixed system fonts and
//! directs clients to `CTFontCreateUIFontForLanguage`, so on iPadOS the system
//! UI font comes from that API (`system_font`). GPUI's own aliases name fonts
//! it bundles on desktop; when the app has not added them through
//! `add_fonts`, they fall back to a font of the same kind that iPadOS ships
//! rather than through GPUI's fallback stack, whose first installed entry on
//! iPadOS is proportional Helvetica.

/// The family GPUI's `.SystemUIFont` stands for; Core Text reports this name
/// for the fonts `system_font` builds.
pub(super) const SYSTEM_UI_FAMILY: &str = ".AppleSystemUIFont";

/// Where a family's fonts come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FamilySource<'a> {
    /// The system UI font and its weight and italic variants.
    SystemUi,
    /// A family looked up by name, first among the app's fonts, then the
    /// system's.
    Named(&'a str),
}

/// What stands in for a GPUI alias whose bundled font the app did not add.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fallback {
    /// A fixed-pitch family iPadOS ships.
    Monospace,
    /// The system UI font.
    SystemUi,
}

impl Fallback {
    /// The sources to try, in order. Menlo and Courier are both in iPadOS's
    /// system font set; Courier only matters if Menlo ever leaves it.
    pub(super) fn sources(self) -> &'static [FamilySource<'static>] {
        match self {
            Self::Monospace => &[FamilySource::Named("Menlo"), FamilySource::Named("Courier")],
            Self::SystemUi => &[FamilySource::SystemUi],
        }
    }
}

/// A requested family resolved to where its fonts come from, and what to use
/// when they are unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FamilyQuery<'a> {
    pub(super) source: FamilySource<'a>,
    pub(super) fallback: Option<Fallback>,
}

impl<'a> FamilyQuery<'a> {
    pub(super) fn new(requested: &'a str) -> Self {
        // GPUI maps `.SystemUIFont` to the platform's name for it and its own
        // aliases to the families it bundles, such as `.ZedMono` to Lilex.
        let name = gpui::font_name_with_fallbacks(requested, SYSTEM_UI_FAMILY);
        let source = match name {
            SYSTEM_UI_FAMILY | "system-ui" => FamilySource::SystemUi,
            name => FamilySource::Named(name),
        };
        let fallback = match requested {
            ".ZedMono" | "Zed Plex Mono" | "monospace" => Some(Fallback::Monospace),
            ".ZedSans" | "Zed Plex Sans" => Some(Fallback::SystemUi),
            _ => None,
        };
        Self { source, fallback }
    }
}
