//! OpenType features and fallback fonts applied to a loaded font, ported from
//! `gpui-pre-macos`'s `open_type` (NOTICE.md). The dictionaries and arrays are
//! core-foundation's owned wrappers rather than `CFDictionaryCreate` and
//! `CFRelease` pairs, so every reference is released by `Drop`.

#![allow(
    unsafe_code,
    reason = "Core Text's attribute keys, cascade list, and font copy are C APIs"
)]

use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::{CFType, TCFType},
    dictionary::CFDictionary,
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_graphics_text::{base::CGFloat, geometry::CGAffineTransform};
use core_text::{
    font::{CTFont, CTFontRef},
    font_descriptor::{
        CTFontDescriptor, CTFontDescriptorRef, kCTFontCascadeListAttribute,
        kCTFontFamilyNameAttribute, kCTFontFeatureSettingsAttribute, kCTFontItalicTrait,
        kCTFontSlantTrait, kCTFontTraitsAttribute, kCTFontWeightTrait,
    },
};
use font_kit::font::Font as FontKitFont;
use gpui::{FontFallbacks, FontFeatures};

use super::{error::Error, fonts::trait_number};

/// Replaces `font` with a copy carrying `features` and, when there are any,
/// `fallbacks` ahead of the system's cascade list.
pub(super) fn apply_features_and_fallbacks(
    font: &mut FontKitFont,
    features: &FontFeatures,
    fallbacks: Option<&FontFallbacks>,
) -> Result<(), Error> {
    let native = font.native_font();
    // SAFETY: both keys are immutable CFString constants Core Text exports;
    // the Get rule retains them for the dictionary below.
    let (features_key, cascade_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCTFontFeatureSettingsAttribute),
            CFString::wrap_under_get_rule(kCTFontCascadeListAttribute),
        )
    };
    let mut attributes = vec![(features_key, feature_array(features).as_CFType())];
    if let Some(fallbacks) = fallbacks
        && !fallbacks.fallback_list().is_empty()
    {
        attributes.push((cascade_key, fallback_array(fallbacks, &native).as_CFType()));
    }
    let attributes = CFDictionary::from_CFType_pairs(&attributes);
    let descriptor = core_text::font_descriptor::new_from_attributes(&attributes);

    // SAFETY: `native` and `descriptor` are live Core Text objects; a size of 0
    // and a null matrix keep the font's own. The result is null or owned by
    // the caller (the Create rule).
    let copy = unsafe {
        CTFontCreateCopyWithAttributes(
            native.as_concrete_TypeRef(),
            0.0,
            std::ptr::null(),
            descriptor.as_concrete_TypeRef(),
        )
    };
    if copy.is_null() {
        return Err(Error::FontAttributes);
    }
    // SAFETY: `copy` is non-null and owned, per the Create rule above.
    let copy = unsafe { CTFont::wrap_under_create_rule(copy) };
    // SAFETY: font-kit marks its native constructors unsafe only because they
    // trust the handle; `copy` is a live CTFont, which the loader retains.
    *font = unsafe { FontKitFont::from_native_font(&copy) };
    Ok(())
}

fn feature_array(features: &FontFeatures) -> CFArray<CFDictionary<CFString, CFType>> {
    // SAFETY: both keys are immutable CFString constants Core Text exports;
    // the Get rule retains them for the dictionaries below.
    let (tag_key, value_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCTFontOpenTypeFeatureTag),
            CFString::wrap_under_get_rule(kCTFontOpenTypeFeatureValue),
        )
    };
    let settings: Vec<CFDictionary<CFString, CFType>> = features
        .tag_value_list()
        .iter()
        .map(|(tag, value)| {
            CFDictionary::from_CFType_pairs(&[
                (tag_key.clone(), CFString::new(tag).as_CFType()),
                (
                    value_key.clone(),
                    CFNumber::from(i64::from(*value)).as_CFType(),
                ),
            ])
        })
        .collect();
    CFArray::from_CFTypes(&settings)
}

/// The user's fallback families at `font`'s weight and slant, then the system's
/// default cascade for the user's languages.
fn fallback_array(fallbacks: &FontFallbacks, font: &CTFont) -> CFArray<CTFontDescriptor> {
    let italic = font.symbolic_traits() & kCTFontItalicTrait != 0;
    // SAFETY: the key is an immutable CFString constant Core Text exports.
    let weight = trait_number(font, unsafe { kCTFontWeightTrait }).unwrap_or(0.0);
    let mut descriptors: Vec<CTFontDescriptor> = fallbacks
        .fallback_list()
        .iter()
        .map(|family| fallback_descriptor(family, weight, italic))
        .collect();
    descriptors.extend(system_fallbacks(font));
    CFArray::from_CFTypes(&descriptors)
}

fn fallback_descriptor(family: &str, weight: f64, italic: bool) -> CTFontDescriptor {
    // SAFETY: the keys are immutable CFString constants Core Text exports; the
    // Get rule retains them for the dictionaries below.
    let (weight_key, slant_key, family_key, traits_key) = unsafe {
        (
            CFString::wrap_under_get_rule(kCTFontWeightTrait),
            CFString::wrap_under_get_rule(kCTFontSlantTrait),
            CFString::wrap_under_get_rule(kCTFontFamilyNameAttribute),
            CFString::wrap_under_get_rule(kCTFontTraitsAttribute),
        )
    };
    let slant = if italic { 1.0 } else { 0.0 };
    let traits: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[
        (weight_key, CFNumber::from(weight).as_CFType()),
        (slant_key, CFNumber::from(slant).as_CFType()),
    ]);
    let attributes: CFDictionary<CFString, CFType> = CFDictionary::from_CFType_pairs(&[
        (family_key, CFString::new(family).as_CFType()),
        (traits_key, traits.as_CFType()),
    ]);
    core_text::font_descriptor::new_from_attributes(&attributes)
}

/// Core Text's default cascade for `font` and the user's preferred languages,
/// limited to fonts with a file, as on macOS.
fn system_fallbacks(font: &CTFont) -> Vec<CTFontDescriptor> {
    // SAFETY: takes no arguments; the result is null or owned by the caller
    // (the Create rule).
    let languages = unsafe { CFLocaleCopyPreferredLanguages() };
    if languages.is_null() {
        return Vec::new();
    }
    // SAFETY: `languages` is a non-null CFArray of CFStrings we own.
    let languages: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(languages) };
    // SAFETY: both arguments are live; the result is null or owned by the
    // caller (the Create rule).
    let cascade = unsafe {
        CTFontCopyDefaultCascadeListForLanguages(
            font.as_concrete_TypeRef(),
            languages.as_concrete_TypeRef(),
        )
    };
    if cascade.is_null() {
        return Vec::new();
    }
    // SAFETY: `cascade` is a non-null CFArray of CTFontDescriptors we own.
    let cascade: CFArray<CTFontDescriptor> = unsafe { CFArray::wrap_under_create_rule(cascade) };
    cascade
        .iter()
        .filter(|descriptor| descriptor.font_path().is_some())
        .map(|descriptor| (*descriptor).clone())
        .collect()
}

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    static kCTFontOpenTypeFeatureTag: CFStringRef;
    static kCTFontOpenTypeFeatureValue: CFStringRef;

    fn CTFontCreateCopyWithAttributes(
        font: CTFontRef,
        size: CGFloat,
        matrix: *const CGAffineTransform,
        attributes: CTFontDescriptorRef,
    ) -> CTFontRef;
    fn CTFontCopyDefaultCascadeListForLanguages(
        font: CTFontRef,
        language_pref_list: CFArrayRef,
    ) -> CFArrayRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFLocaleCopyPreferredLanguages() -> CFArrayRef;
}
