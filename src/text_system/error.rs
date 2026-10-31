use font_kit::error::{FontLoadingError, GlyphLoadingError, SelectionError};
use thiserror::Error;

/// Failures of the Core Text text system. `gpui::PlatformTextSystem` returns
/// `anyhow::Result`, so these convert at that boundary and keep their source.
#[derive(Debug, Error)]
pub(crate) enum Error {
    /// Core Graphics could not read a font the app embeds.
    #[error("an embedded font could not be read")]
    EmbeddedFont,
    /// A requested family has no font this system can use.
    #[error("font family {family:?} has no usable fonts")]
    EmptyFamily { family: String },
    /// A glyph's raster bounds are empty, so there is nothing to draw.
    #[error("glyph raster bounds are empty")]
    EmptyGlyphBounds,
    /// Core Text could not derive a font with the requested features and
    /// fallbacks.
    #[error("Core Text could not apply font features and fallbacks")]
    FontAttributes,
    /// A font could not be loaded from its handle or data.
    #[error(transparent)]
    FontLoading(#[from] FontLoadingError),
    /// A glyph's outline, bounds, or advance could not be read.
    #[error(transparent)]
    GlyphLoading(#[from] GlyphLoadingError),
    /// A glyph's raster size does not fit a bitmap.
    #[error("glyph bitmap of {width}x{height} pixels is invalid")]
    InvalidBitmapSize { width: i32, height: i32 },
    /// A family was not found, or none of its fonts matched the request.
    #[error(transparent)]
    Selection(#[from] SelectionError),
}
