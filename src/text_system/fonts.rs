//! Loading fonts: the app's embedded fonts, families by name or the system UI
//! family, and the fonts Core Text substitutes while shaping. Ported from
//! `gpui-pre-macos`'s `MacTextSystemState` (NOTICE.md).

#![allow(
    unsafe_code,
    reason = "Core Text's font collections, descriptors, and trait keys are C APIs"
)]

use std::{borrow::Cow, collections::HashSet, sync::Arc};

use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::{CFType, TCFType},
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_graphics_text::{data_provider::CGDataProvider, font::CGFont};
use core_text::{
    font::CTFont,
    font_collection::CTFontCollectionRef,
    font_descriptor::{
        CTFontDescriptor, CTFontDescriptorCopyAttribute, CTFontTraits, kCTFontFamilyNameAttribute,
        kCTFontSlantTrait, kCTFontSymbolicTrait, kCTFontWeightTrait, kCTFontWidthTrait,
    },
};
use font_kit::{
    font::Font as FontKitFont,
    handle::Handle,
    properties::{Properties, Weight},
};
use gpui::{FontFallbacks, FontFeatures, FontId};
use smallvec::SmallVec;

use super::{
    IosTextSystemState,
    convert::is_emoji_postscript_name,
    error::Error,
    family::{FamilyQuery, FamilySource, SYSTEM_UI_FAMILY},
    open_type::apply_features_and_fallbacks,
    system_font,
};

/// A registered font of a family and the properties GPUI's weight and style
/// matching sees for it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Candidate {
    pub(super) font_id: FontId,
    pub(super) properties: Properties,
}

/// A font about to be registered, and the weight to match it by when that is
/// not the one font-kit reads back (`system_font`).
type FamilyFont = (FontKitFont, Option<Weight>);

impl IosTextSystemState {
    pub(super) fn add_fonts(&mut self, fonts: Vec<Cow<'static, [u8]>>) -> Result<(), Error> {
        let handles = fonts
            .into_iter()
            .map(|bytes| match bytes {
                Cow::Borrowed(embedded_font) => {
                    // SAFETY: the provider reads `embedded_font` without
                    // copying or owning it; it is `'static`, so it outlives
                    // the provider and every font made from it.
                    let data_provider = unsafe { CGDataProvider::from_slice(embedded_font) };
                    let font = CGFont::from_data_provider(data_provider)
                        .map_err(|()| Error::EmbeddedFont)?;
                    Ok(Handle::from_native(&FontKitFont::from_core_graphics_font(
                        font,
                    )))
                }
                Cow::Owned(bytes) => Ok(Handle::from_memory(Arc::new(bytes), 0)),
            })
            .collect::<Result<Vec<_>, Error>>()?;
        self.memory_source.add_fonts(handles.into_iter())?;
        Ok(())
    }

    /// The usable fonts of `requested`, registered under new `FontId`s, or of
    /// its platform fallback when it has none (`family`).
    pub(super) fn load_family(
        &mut self,
        requested: &str,
        features: &FontFeatures,
        fallbacks: Option<&FontFallbacks>,
    ) -> Result<SmallVec<[Candidate; 4]>, Error> {
        let query = FamilyQuery::new(requested);
        let error = match self.load_source(query.source, features, fallbacks) {
            Ok(candidates) => return Ok(candidates),
            Err(error) => error,
        };
        let Some(fallback) = query.fallback else {
            return Err(error);
        };
        for &source in fallback.sources() {
            match self.load_source(source, features, fallbacks) {
                Ok(candidates) => {
                    tracing::debug!(
                        family = requested,
                        ?source,
                        %error,
                        "font family unavailable; using the platform fallback"
                    );
                    return Ok(candidates);
                }
                Err(fallback_error) => tracing::warn!(
                    family = requested,
                    ?source,
                    error = %fallback_error,
                    "fallback font family unavailable"
                ),
            }
        }
        Err(error)
    }

    fn load_source(
        &mut self,
        source: FamilySource<'_>,
        features: &FontFeatures,
        fallbacks: Option<&FontFallbacks>,
    ) -> Result<SmallVec<[Candidate; 4]>, Error> {
        let (name, fonts) = match source {
            FamilySource::SystemUi => (SYSTEM_UI_FAMILY, system_ui_fonts()),
            FamilySource::Named(name) => (name, self.named_family(name)?),
        };
        let candidates = self.register_family(name, fonts, features, fallbacks);
        if candidates.is_empty() {
            return Err(Error::EmptyFamily {
                family: name.to_owned(),
            });
        }
        Ok(candidates)
    }

    /// The fonts of the family `name`, the app's own before the system's,
    /// matched by the weight font-kit reads back.
    fn named_family(&self, name: &str) -> Result<Vec<FamilyFont>, Error> {
        let family = self
            .memory_source
            .select_family_by_name(name)
            .or_else(|_| self.system_source.select_family_by_name(name))?;
        let fonts = family
            .fonts()
            .iter()
            .filter_map(|handle| {
                handle
                    .load()
                    .inspect_err(|error| {
                        tracing::warn!(family = name, %error, "font could not be loaded; skipping");
                    })
                    .ok()
            })
            .map(|font| (font, None))
            .collect();
        Ok(fonts)
    }

    /// Registers the fonts of one family that GPUI can use, with `features`
    /// and `fallbacks` applied, and returns their new ids with the properties
    /// to match them by.
    fn register_family(
        &mut self,
        name: &str,
        fonts: Vec<FamilyFont>,
        features: &FontFeatures,
        fallbacks: Option<&FontFallbacks>,
    ) -> SmallVec<[Candidate; 4]> {
        let mut candidates = SmallVec::new();
        // Dedup is scoped to this one load (upstream issue #55472): the same
        // family can be loaded again under another `FontKey`, with other
        // features or fallbacks, and must then get fonts of its own.
        let mut postscript_names_seen = HashSet::new();
        for (mut font, matching_weight) in fonts {
            // GPUI measures with 'm' in several places and fails without it.
            if font.glyph_for_char('m').is_none() {
                tracing::warn!(font = font.full_name(), "font has no 'm' glyph; skipping");
                continue;
            }
            // font-kit's `properties()` unwraps these traits as numbers, which
            // has panicked for some fonts on macOS; drop such a font instead.
            if !has_numeric_traits(&font.native_font()) {
                tracing::error!(
                    font = font.full_name(),
                    postscript_name = font.postscript_name(),
                    "font traits are not numbers; skipping"
                );
                continue;
            }
            let Some(postscript_name) = font.postscript_name() else {
                tracing::warn!(
                    font = font.full_name(),
                    family = name,
                    "font has no PostScript name; skipping"
                );
                continue;
            };
            if !postscript_names_seen.insert(postscript_name.clone()) {
                tracing::warn!(
                    font = font.full_name(),
                    postscript_name,
                    family = name,
                    "duplicate font in family; skipping"
                );
                continue;
            }
            if let Err(error) = apply_features_and_fallbacks(&mut font, features, fallbacks) {
                tracing::warn!(
                    font = font.full_name(),
                    %error,
                    "font features and fallbacks could not be applied; skipping"
                );
                continue;
            }
            // Read only after the numeric-trait check above, which keeps
            // font-kit's unwrapping accessors from panicking here.
            let mut properties = font.properties();
            if let Some(weight) = matching_weight {
                properties.weight = weight;
            }
            candidates.push(Candidate {
                font_id: self.push_font(font, postscript_name),
                properties,
            });
        }
        candidates
    }

    fn push_font(&mut self, font: FontKitFont, postscript_name: String) -> FontId {
        let font_id = FontId(self.fonts.len());
        self.font_ids_by_postscript_name
            .insert(postscript_name.clone(), font_id);
        self.postscript_names_by_font_id
            .insert(font_id, postscript_name);
        self.fonts.push(font);
        font_id
    }

    /// The id of a font Core Text chose while shaping, registering it the first
    /// time it appears, as a cascade fallback does.
    pub(super) fn id_for_native_font(&mut self, requested_font: &CTFont) -> FontId {
        let postscript_name = requested_font.postscript_name();
        if let Some(font_id) = self.font_ids_by_postscript_name.get(&postscript_name) {
            return *font_id;
        }
        let font = FontKitFont::from_core_graphics_font(requested_font.copy_to_CGFont());
        self.push_font(font, postscript_name)
    }

    pub(super) fn is_emoji(&self, font_id: FontId) -> bool {
        self.postscript_names_by_font_id
            .get(&font_id)
            .is_some_and(|postscript_name| is_emoji_postscript_name(postscript_name))
    }

    /// Every installed family's name, then the families the app added.
    pub(super) fn all_font_names(&self) -> Vec<String> {
        let mut names = installed_family_names();
        if let Ok(fonts_in_memory) = self.memory_source.all_families() {
            names.extend(fonts_in_memory);
        }
        names
    }
}

/// A numeric entry of `font`'s traits, or `None` when the font lacks it or
/// stores something else there. font-kit's own accessors unwrap both.
pub(super) fn trait_number(font: &CTFont, key: CFStringRef) -> Option<f64> {
    numeric_trait(&font.all_traits(), key)
}

fn numeric_trait(traits: &CTFontTraits, key: CFStringRef) -> Option<f64> {
    traits.find(key)?.downcast::<CFNumber>()?.to_f64()
}

fn has_numeric_traits(font: &CTFont) -> bool {
    let traits = font.all_traits();
    // SAFETY: the keys are immutable CFString constants Core Text exports.
    let keys = unsafe {
        [
            kCTFontSymbolicTrait,
            kCTFontWidthTrait,
            kCTFontWeightTrait,
            kCTFontSlantTrait,
        ]
    };
    keys.into_iter()
        .all(|key| numeric_trait(&traits, key).is_some())
}

fn system_ui_fonts() -> Vec<FamilyFont> {
    system_font::family()
        .iter()
        .map(|(font, weight)| {
            // SAFETY: font-kit marks its native constructors unsafe only
            // because they trust the handle; each is a live CTFont the loader
            // retains.
            let font = unsafe { FontKitFont::from_native_font(font) };
            (font, Some(*weight))
        })
        .collect()
}

fn installed_family_names() -> Vec<String> {
    let collection = core_text::font_collection::create_for_all_families();
    // core-text 21's `get_descriptors` wraps this Create-rule result under the
    // Get rule and leaks it, so the function is called directly.
    // SAFETY: `collection` is live; the result is null or owned by the caller
    // (the Create rule).
    let array =
        unsafe { CTFontCollectionCreateMatchingFontDescriptors(collection.as_concrete_TypeRef()) };
    if array.is_null() {
        return Vec::new();
    }
    // SAFETY: `array` is a non-null CFArray of CTFontDescriptors we own.
    let descriptors: CFArray<CTFontDescriptor> = unsafe { CFArray::wrap_under_create_rule(array) };
    descriptors
        .iter()
        .filter_map(|descriptor| family_name(&descriptor))
        .collect()
}

/// A descriptor's family name, or `None` where core-text's own accessor would
/// panic: some fonts have no family name, or a value that is not a string.
fn family_name(descriptor: &CTFontDescriptor) -> Option<String> {
    // SAFETY: `descriptor` is live and the key is an immutable Core Text
    // constant; the result is null or owned by the caller (the Create rule).
    let value = unsafe {
        CTFontDescriptorCopyAttribute(descriptor.as_concrete_TypeRef(), kCTFontFamilyNameAttribute)
    };
    if value.is_null() {
        return None;
    }
    // SAFETY: `value` is non-null and owned, per the Create rule above.
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    value
        .downcast_into::<CFString>()
        .map(|name| name.to_string())
}

#[link(name = "CoreText", kind = "framework")]
unsafe extern "C" {
    fn CTFontCollectionCreateMatchingFontDescriptors(collection: CTFontCollectionRef)
    -> CFArrayRef;
}
