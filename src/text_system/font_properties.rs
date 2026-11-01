//! Core Text properties without font-kit's CFNumber-only trait accessors.
//! iOS may store a zero trait as CFBoolean(false). The CSS mappings follow
//! zed-font-kit 0.14.1-zed's Core Text loader (Apache-2.0; see NOTICE.md).

use core_foundation::{base::CFType, boolean::CFBoolean, number::CFNumber, string::CFStringRef};
use core_text::{
    font::CTFont,
    font_descriptor::{
        kCTFontItalicTrait, kCTFontSlantTrait, kCTFontWeightTrait, kCTFontWidthTrait,
    },
};
use font_kit::properties::{Properties, Stretch, Style, Weight};

// Core Text's normalized weights and the CSS weights font-kit matches them by.
const WEIGHTS: &[(f64, Weight)] = &[
    (-0.7, Weight::THIN),
    (-0.5, Weight::EXTRA_LIGHT),
    (-0.23, Weight::LIGHT),
    (0.0, Weight::NORMAL),
    (0.2, Weight::MEDIUM),
    (0.3, Weight::SEMIBOLD),
    (0.4, Weight::BOLD),
    (0.6, Weight::EXTRA_BOLD),
    (0.8, Weight::BLACK),
];

pub(super) fn read(font: &CTFont) -> Option<Properties> {
    // SAFETY: the keys are immutable CFString constants Core Text exports.
    #[allow(unsafe_code)]
    let (weight, width, slant) = unsafe {
        (
            trait_number(font, kCTFontWeightTrait)?,
            trait_number(font, kCTFontWidthTrait)?,
            trait_number(font, kCTFontSlantTrait)?,
        )
    };
    if ![weight, width, slant].into_iter().all(f64::is_finite) || !(-1.0..=1.0).contains(&width) {
        return None;
    }
    let style = if font.symbolic_traits() & kCTFontItalicTrait != 0 {
        Style::Italic
    } else if slant > 0.0 {
        Style::Oblique
    } else {
        Style::Normal
    };
    Some(Properties {
        style,
        weight: css_weight(weight),
        stretch: css_stretch(width),
    })
}

pub(super) fn trait_number(font: &CTFont, key: CFStringRef) -> Option<f64> {
    let traits = font.all_traits();
    let value = traits.find(key)?;
    number(&value)
}

fn number(value: &CFType) -> Option<f64> {
    value
        .downcast::<CFNumber>()
        .and_then(|value| value.to_f64())
        .or_else(|| {
            value
                .downcast::<CFBoolean>()
                .map(|value| u8::from(bool::from(value)).into())
        })
}

fn css_weight(weight: f64) -> Weight {
    if weight <= WEIGHTS[0].0 {
        return Weight::THIN;
    }
    let Some(pair) = WEIGHTS.windows(2).find(|pair| weight <= pair[1].0) else {
        // The pinned loader maps weights above its last anchor to index 9.
        return Weight(1_000.0);
    };
    let ((low, low_css), (high, high_css)) = (pair[0], pair[1]);
    let progress = ((weight - low) / (high - low)) as f32;
    Weight(low_css.0 + (high_css.0 - low_css.0) * progress)
}

fn css_stretch(width: f64) -> Stretch {
    // These are the linear segments through font-kit's nine CSS width anchors.
    let stretch = if width <= 0.5 {
        1.0 + width * 0.5
    } else if width <= 0.75 {
        width + 0.75
    } else {
        width * 2.0
    };
    Stretch(stretch as f32)
}

#[cfg(test)]
mod tests;
