use crate::text_system::family::{Fallback, FamilyQuery, FamilySource, SYSTEM_UI_FAMILY};

fn query(source: FamilySource<'_>, fallback: Option<Fallback>) -> FamilyQuery<'_> {
    FamilyQuery { source, fallback }
}

#[test]
fn system_ui_names_use_the_ui_font_api() {
    for name in [".SystemUIFont", SYSTEM_UI_FAMILY, "system-ui"] {
        assert_eq!(
            FamilyQuery::new(name),
            query(FamilySource::SystemUi, None),
            "{name}"
        );
    }
}

#[test]
fn gpui_mono_alias_names_its_bundled_font_then_falls_back_to_monospace() {
    for name in [".ZedMono", "Zed Plex Mono"] {
        assert_eq!(
            FamilyQuery::new(name),
            query(FamilySource::Named("Lilex"), Some(Fallback::Monospace)),
            "{name}"
        );
    }
    assert_eq!(
        FamilyQuery::new("monospace"),
        query(FamilySource::Named("monospace"), Some(Fallback::Monospace))
    );
}

#[test]
fn gpui_sans_alias_names_its_bundled_font_then_falls_back_to_the_system_font() {
    for name in [".ZedSans", "Zed Plex Sans"] {
        assert_eq!(
            FamilyQuery::new(name),
            query(
                FamilySource::Named("IBM Plex Sans"),
                Some(Fallback::SystemUi)
            ),
            "{name}"
        );
    }
}

#[test]
fn installed_families_are_looked_up_by_name_without_a_fallback() {
    for name in ["Menlo", "Helvetica", "JetBrains Mono", "Lilex"] {
        assert_eq!(
            FamilyQuery::new(name),
            query(FamilySource::Named(name), None),
            "{name}"
        );
    }
}

#[test]
fn monospace_falls_back_to_menlo_first() {
    assert_eq!(
        Fallback::Monospace.sources().first(),
        Some(&FamilySource::Named("Menlo"))
    );
    assert_eq!(Fallback::SystemUi.sources(), &[FamilySource::SystemUi]);
}
