//! The system UI font family, built from Core Text's UI font API.
//!
//! `CTFontCreateUIFontForLanguage` gives the regular system font; copying its
//! descriptor with a weight trait gives each weight, as `UIFont.systemFont(
//! ofSize:weight:)` does, and the italic symbolic trait gives each italic.
//! Every variant reports the family `.AppleSystemUIFont` and its own
//! PostScript name (on macOS 26, `.SFNS-Semibold` and `.SFNS-SemiboldItalic`),
//! so the fonts Core Text picks while shaping map back to these by name.
//!
//! Each font comes with the CSS weight GPUI matches it by. font-kit reads a
//! Core Text weight back through its own piecewise mapping, on which the
//! system weights land between the CSS hundreds: medium (0.23) at 530, heavy
//! (0.56) at 780, black (0.62) at 810. Matched on those, `FontWeight::MEDIUM`
//! would get regular and `EXTRA_BOLD` black, so the system family is matched
//! by the rank of the `UIFont.Weight` each font was made with instead.

#![allow(
    unsafe_code,
    reason = "Core Text's UI font constructor and trait keys are C APIs"
)]

use core_foundation::{
    base::{CFType, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_graphics_text::base::CGFloat;
use core_text::{
    font::{CTFont, CTFontRef, CTFontUIFontType, kCTFontSystemFontType},
    font_descriptor::{kCTFontItalicTrait, kCTFontTraitsAttribute, kCTFontWeightTrait},
};
use font_kit::properties::Weight;

/// `UIFont.Weight` from ultralight to black as Core Text's normalized
/// weights, each with the CSS weight of the same rank, which is what
/// `gpui::FontWeight`'s nine constants ask for.
const WEIGHTS: [(f64, Weight); 9] = [
    (-0.8, Weight::THIN),
    (-0.6, Weight::EXTRA_LIGHT),
    (-0.4, Weight::LIGHT),
    (0.0, Weight::NORMAL),
    (0.23, Weight::MEDIUM),
    (0.3, Weight::SEMIBOLD),
    (0.4, Weight::BOLD),
    (0.56, Weight::EXTRA_BOLD),
    (0.62, Weight::BLACK),
];

/// The system UI font in every weight, upright and italic, each with the CSS
/// weight to match it by. Empty only if Core Text cannot make a system font
/// at all.
pub(super) fn family() -> Vec<(CTFont, Weight)> {
    let Some(regular) = ui_font(kCTFontSystemFontType) else {
        tracing::error!("Core Text returned no system UI font");
        return Vec::new();
    };
    let mut fonts = Vec::with_capacity(WEIGHTS.len() * 2);
    for (weight, css_weight) in WEIGHTS {
        let Some(upright) = with_weight(&regular, weight) else {
            tracing::warn!(weight, "system UI font has no variant for this weight");
            continue;
        };
        if let Some(italic) =
            upright.clone_with_symbolic_traits(kCTFontItalicTrait, kCTFontItalicTrait)
        {
            fonts.push((italic, css_weight));
        }
        fonts.push((upright, css_weight));
    }
    fonts
}

/// A UI font at the system's default size; callers resize as they draw.
fn ui_font(ui_type: CTFontUIFontType) -> Option<CTFont> {
    // SAFETY: `ui_type` is one of Core Text's UI font constants, a size of 0
    // asks for the default size, and a null language uses the user's. The
    // result is null or a font the caller owns (the Create rule).
    let font = unsafe { CTFontCreateUIFontForLanguage(ui_type, 0.0, std::ptr::null()) };
    if font.is_null() {
        return None;
    }
    // SAFETY: `font` is non-null and owned, per the Create rule above.
    Some(unsafe { CTFont::wrap_under_create_rule(font) })
}

fn with_weight(font: &CTFont, weight: f64) -> Option<CTFont> {
    // SAFETY: the trait keys are immutable CFString constants Core Text
    // exports; the Get rule retains them for the dictionaries below.
    let (weight_key, traits_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCTFontWeightTrait),
            CFString::wrap_under_get_rule(kCTFontTraitsAttribute),
        )
    };
    let traits: CFDictionary<CFString, CFType> =
        CFDictionary::from_CFType_pairs(&[(weight_key, CFNumber::from(weight).as_CFType())]);
    let attributes: CFDictionary<CFString, CFType> =
        CFDictionary::from_CFType_pairs(&[(traits_key, traits.as_CFType())]);
    let descriptor = font
        .copy_descriptor()
        .create_copy_with_attributes(attributes.to_untyped())
        .ok()?;
    Some(core_text::font::new_from_descriptor(&descriptor, 0.0))
}

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    fn CTFontCreateUIFontForLanguage(
        ui_type: CTFontUIFontType,
        size: CGFloat,
        language: CFStringRef,
    ) -> CTFontRef;
}
