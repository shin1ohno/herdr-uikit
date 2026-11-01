use super::*;

fn uikit(
    menus: &'static [(StandardMenu, &'static str)],
) -> impl Fn(StandardMenu) -> Option<String> {
    move |wanted| {
        menus
            .iter()
            .find(|(menu, _)| *menu == wanted)
            .map(|(_, title)| (*title).to_owned())
    }
}

const ENGLISH: &[(StandardMenu, &str)] = &[
    (StandardMenu::File, "File"),
    (StandardMenu::Edit, "Edit"),
    (StandardMenu::View, "View"),
    (StandardMenu::Window, "Window"),
    (StandardMenu::Help, "Help"),
];

#[test]
fn the_first_menu_is_the_application_menu() {
    assert_eq!(
        placement(0, "Herdr", uikit(ENGLISH)),
        Placement::Application
    );
    assert_eq!(placement(0, "View", uikit(ENGLISH)), Placement::Application);
}

#[test]
fn standard_menus_join_uikits() {
    for (title, menu) in [
        ("File", StandardMenu::File),
        ("Edit", StandardMenu::Edit),
        ("View", StandardMenu::View),
        ("Window", StandardMenu::Window),
        ("Help", StandardMenu::Help),
    ] {
        assert_eq!(
            placement(1, title, uikit(ENGLISH)),
            Placement::Standard(menu)
        );
    }
}

#[test]
fn other_menus_stand_beside_them() {
    assert_eq!(placement(3, "Terminal", uikit(ENGLISH)), Placement::Sibling);
}

#[test]
fn a_menu_uikit_lacks_stands_on_its_own() {
    const NO_WINDOW: &[(StandardMenu, &str)] = &[(StandardMenu::View, "View")];
    assert_eq!(placement(2, "Window", uikit(NO_WINDOW)), Placement::Sibling);
}

#[test]
fn localized_titles_match_uikits_localized_menu() {
    const GERMAN: &[(StandardMenu, &str)] = &[
        (StandardMenu::View, "Darstellung"),
        (StandardMenu::Window, "Fenster"),
    ];
    assert_eq!(
        placement(1, "Darstellung", uikit(GERMAN)),
        Placement::Standard(StandardMenu::View)
    );
    assert_eq!(
        placement(2, "Fenster", uikit(GERMAN)),
        Placement::Standard(StandardMenu::Window)
    );
    assert_eq!(placement(3, "Ansicht", uikit(GERMAN)), Placement::Sibling);
}
