use std::{collections::HashSet, path::PathBuf};

use objc2_ui_kit::UIPasteboard;

use super::*;

/// A private pasteboard, so tests neither read nor clobber the user's.
fn unique() -> Pasteboard {
    Pasteboard {
        inner: UIPasteboard::pasteboardWithUniqueName(),
    }
}

#[test]
fn every_image_format_has_its_own_type() {
    let types: HashSet<_> = IMAGE_FORMATS
        .iter()
        .map(|&format| image_type(format))
        .collect();

    assert_eq!(types.len(), IMAGE_FORMATS.len());
    assert!(!types.contains(TEXT_TYPE));
}

#[test]
fn image_formats_are_listed_once() {
    let formats: HashSet<_> = IMAGE_FORMATS.iter().collect();

    assert_eq!(formats.len(), IMAGE_FORMATS.len());
}

#[test]
fn text_round_trips_with_its_metadata() {
    let pasteboard = unique();
    let item = ClipboardItem::new_string_with_metadata("copied".into(), r#"{"lines":1}"#.into());

    pasteboard.write(item.clone());

    assert_eq!(pasteboard.read(), Some(item));
}

#[test]
fn metadata_for_other_text_is_dropped() {
    let pasteboard = unique();
    pasteboard.write(ClipboardItem::new_string_with_metadata(
        "first".into(),
        "meta".into(),
    ));

    assert_eq!(pasteboard.metadata_for("first"), Some("meta".into()));
    assert_eq!(pasteboard.metadata_for("second"), None);
}

#[test]
fn an_image_round_trips_under_its_type() {
    let pasteboard = unique();
    let image = Image::from_bytes(ImageFormat::Png, vec![0x89, b'P', b'N', b'G']);

    pasteboard.write(ClipboardItem::new_image(&image));

    assert_eq!(pasteboard.read(), Some(ClipboardItem::new_image(&image)));
}

#[test]
fn several_strings_are_joined() {
    let pasteboard = unique();
    let item = ClipboardItem {
        entries: vec![
            ClipboardEntry::String(ClipboardString::new("a".into())),
            ClipboardEntry::String(ClipboardString::new("b".into())),
        ],
    };

    pasteboard.write(item);

    assert_eq!(
        pasteboard.read(),
        Some(ClipboardItem::new_string("ab".into()))
    );
}

#[test]
fn an_empty_item_clears_the_pasteboard() {
    let pasteboard = unique();
    pasteboard.write(ClipboardItem::new_string("stale".into()));

    pasteboard.write(ClipboardItem {
        entries: Vec::new(),
    });

    assert_eq!(pasteboard.read(), None);
}

#[test]
fn file_paths_are_copied_as_text() {
    let pasteboard = unique();
    pasteboard.write(ClipboardItem::new_string("stale".into()));

    pasteboard.write(ClipboardItem {
        entries: vec![ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
            [PathBuf::from("/srv/a.txt"), PathBuf::from("/srv/b.txt")]
                .into_iter()
                .collect(),
        ))],
    });

    assert_eq!(
        pasteboard.read(),
        Some(ClipboardItem::new_string("/srv/a.txt\n/srv/b.txt".into()))
    );
}

#[test]
fn paths_from_several_entries_are_copied_when_there_is_nothing_else() {
    let pasteboard = unique();
    let paths = |path: &str| {
        ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
            [PathBuf::from(path)].into_iter().collect(),
        ))
    };

    pasteboard.write(ClipboardItem {
        entries: vec![paths("/srv/a.txt"), paths("/srv/b.txt")],
    });

    assert_eq!(
        pasteboard.read(),
        Some(ClipboardItem::new_string("/srv/a.txt\n/srv/b.txt".into()))
    );
}

#[test]
fn text_wins_over_paths_in_a_mixed_item() {
    let pasteboard = unique();

    pasteboard.write(ClipboardItem {
        entries: vec![
            ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                [PathBuf::from("/srv/a.txt")].into_iter().collect(),
            )),
            ClipboardEntry::String(ClipboardString::new("text".into())),
        ],
    });

    assert_eq!(
        pasteboard.read(),
        Some(ClipboardItem::new_string("text".into()))
    );
}

#[test]
fn text_without_metadata_has_no_metadata_types() {
    let pasteboard = unique();

    pasteboard.write(ClipboardItem::new_string("plain".into()));

    assert_eq!(pasteboard.data(TEXT_HASH_TYPE), None);
    assert_eq!(pasteboard.data(METADATA_TYPE), None);
    assert_eq!(pasteboard.metadata_for("plain"), None);
}
