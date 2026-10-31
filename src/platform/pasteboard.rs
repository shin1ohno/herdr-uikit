//! The general pasteboard in GPUI's clipboard model, ported from
//! `gpui-pre-macos`'s pasteboard. Text carries GPUI's metadata beside it
//! under the same private types macOS uses, so copying and pasting inside the
//! app keeps it, and images travel under their uniform type identifiers. File
//! paths are copied as their text, one per line, and never read back as
//! files: the iPad hands files between apps through sharing, not paths.

#![allow(
    unsafe_code,
    reason = "UIPasteboard's non-atomic properties are unsafe accessors in objc2"
)]

use std::path::Path;

use gpui::{ClipboardEntry, ClipboardItem, ClipboardString, Image, ImageFormat};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSString};
use objc2_ui_kit::UIPasteboard;

const TEXT_TYPE: &str = "public.utf8-plain-text";
const TEXT_HASH_TYPE: &str = "zed-text-hash";
const METADATA_TYPE: &str = "zed-metadata";

/// The image formats a paste looks for, most likely first, as GPUI orders
/// them.
const IMAGE_FORMATS: &[ImageFormat] = &[
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Webp,
    ImageFormat::Gif,
    ImageFormat::Svg,
    ImageFormat::Bmp,
    ImageFormat::Tiff,
    ImageFormat::Ico,
    ImageFormat::Pnm,
];

/// The uniform type identifier of an image format.
fn image_type(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Bmp => "com.microsoft.bmp",
        ImageFormat::Gif => "com.compuserve.gif",
        ImageFormat::Ico => "com.microsoft.ico",
        ImageFormat::Jpeg => "public.jpeg",
        ImageFormat::Png => "public.png",
        ImageFormat::Pnm => "public.pbm",
        ImageFormat::Svg => "public.svg-image",
        ImageFormat::Tiff => "public.tiff",
        ImageFormat::Webp => "org.webmproject.webp",
    }
}

/// Paths as plain text, one per line. Another app cannot open them as files,
/// but writing them still replaces an earlier copy, which would otherwise be
/// what the next paste inserts.
fn paths_text(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> ClipboardString {
    let lines: Vec<String> = paths
        .into_iter()
        .map(|path| path.as_ref().display().to_string())
        .collect();
    ClipboardString::new(lines.join("\n"))
}

/// A pasteboard owned by the platform. `IosPlatform` is neither `Send` nor
/// `Sync`, so the pasteboard's non-atomic properties are only ever used from
/// the main thread that owns it.
pub(super) struct Pasteboard {
    inner: Retained<UIPasteboard>,
}

impl Pasteboard {
    pub(super) fn general() -> Self {
        Self {
            inner: UIPasteboard::generalPasteboard(),
        }
    }

    /// Text first, then the first image format present.
    pub(super) fn read(&self) -> Option<ClipboardItem> {
        self.read_text()
            .or_else(|| self.read_image())
            .map(ClipboardItem::from)
    }

    pub(super) fn write(&self, item: ClipboardItem) {
        match item.entries.as_slice() {
            [] => self.set_items(&[]),
            [ClipboardEntry::String(string)] => self.write_text(string),
            [ClipboardEntry::Image(image)] => self.write_image(image),
            [ClipboardEntry::ExternalPaths(paths)] => self.write_text(&paths_text(paths.paths())),
            entries => self.write_combined(entries),
        }
    }

    fn read_text(&self) -> Option<ClipboardEntry> {
        // `hasStrings` comes first because it does not count as reading
        // another app's content, which may ask the user for permission.
        // SAFETY: See `Pasteboard`; no other thread uses these properties.
        if !unsafe { self.inner.hasStrings() } {
            return None;
        }
        // SAFETY: As above.
        let text = unsafe { self.inner.string() }?.to_string();
        let metadata = self.metadata_for(&text);
        Some(ClipboardEntry::String(ClipboardString { text, metadata }))
    }

    /// GPUI's metadata, when the hash stored beside it matches `text`, so
    /// metadata from an older copy never attaches to newer text. Text another
    /// app copied has neither type, so reading it costs no access beyond the
    /// text itself (see `data`).
    fn metadata_for(&self, text: &str) -> Option<String> {
        let hash = self.data(TEXT_HASH_TYPE)?;
        let hash = u64::from_be_bytes(hash.as_slice().try_into().ok()?);
        if hash != ClipboardString::text_hash(text) {
            return None;
        }
        String::from_utf8(self.data(METADATA_TYPE)?).ok()
    }

    fn read_image(&self) -> Option<ClipboardEntry> {
        IMAGE_FORMATS.iter().find_map(|&format| {
            let bytes = self.data(image_type(format))?;
            Some(ClipboardEntry::Image(Image::from_bytes(format, bytes)))
        })
    }

    /// The content under `kind`, read only when the pasteboard holds that
    /// type. Checking the types reads no content, while reading another app's
    /// content may ask the user for permission, so an absent type asks for
    /// nothing.
    fn data(&self, kind: &str) -> Option<Vec<u8>> {
        let kind = NSString::from_str(kind);
        if !self
            .inner
            .containsPasteboardTypes(&NSArray::from_slice(&[&*kind]))
        {
            return None;
        }
        self.inner
            .dataForPasteboardType(&kind)
            .map(|data| data.to_vec())
    }

    /// One item holding the text and, when it has metadata, the hash and
    /// metadata beside it, as one item with several types.
    fn write_text(&self, string: &ClipboardString) {
        let text_type = NSString::from_str(TEXT_TYPE);
        let text = NSString::from_str(&string.text);
        let item = match &string.metadata {
            None => {
                let objects: [&AnyObject; 1] = [&text];
                NSDictionary::from_slices(&[&*text_type], &objects)
            }
            Some(metadata) => {
                let hash_type = NSString::from_str(TEXT_HASH_TYPE);
                let metadata_type = NSString::from_str(METADATA_TYPE);
                let hash =
                    NSData::with_bytes(&ClipboardString::text_hash(&string.text).to_be_bytes());
                let metadata = NSData::with_bytes(metadata.as_bytes());
                let objects: [&AnyObject; 3] = [&text, &hash, &metadata];
                NSDictionary::from_slices(&[&*text_type, &*hash_type, &*metadata_type], &objects)
            }
        };
        self.set_items(&[item]);
    }

    fn write_image(&self, image: &Image) {
        self.inner.setData_forPasteboardType(
            &NSData::with_bytes(&image.bytes),
            &NSString::from_str(image_type(image.format)),
        );
    }

    /// Several entries: their text joined, with the first metadata, as on
    /// macOS; with no text, the first image; with neither, the paths as text.
    /// Something is always written, so a copy never leaves an earlier one to
    /// be pasted.
    fn write_combined(&self, entries: &[ClipboardEntry]) {
        let mut combined = ClipboardString {
            text: String::new(),
            metadata: None,
        };
        let mut first_image = None;
        let mut paths = Vec::new();
        for entry in entries {
            match entry {
                ClipboardEntry::String(string) => {
                    combined.text.push_str(&string.text);
                    if combined.metadata.is_none() {
                        combined.metadata.clone_from(&string.metadata);
                    }
                }
                ClipboardEntry::Image(image) => {
                    first_image.get_or_insert(image);
                }
                ClipboardEntry::ExternalPaths(entry_paths) => paths.extend(entry_paths.paths()),
            }
        }
        if !combined.text.is_empty() {
            self.write_text(&combined);
        } else if let Some(image) = first_image {
            self.write_image(image);
        } else if !paths.is_empty() {
            self.write_text(&paths_text(paths));
        } else {
            self.write_text(&combined);
        }
    }

    fn set_items(&self, items: &[Retained<NSDictionary<NSString, AnyObject>>]) {
        let items = NSArray::from_retained_slice(items);
        // SAFETY: See `Pasteboard` for the threading. Each item maps uniform
        // type identifiers to `NSString` or `NSData`, the values UIPasteboard
        // stores for any type.
        unsafe { self.inner.setItems(&items) };
    }
}

#[cfg(test)]
mod tests;
