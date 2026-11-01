use gpui::{KeyBinding, Keymap, Menu, MenuItem, Modifiers, OsAction, OsMenu, SystemMenuType};

use super::*;

gpui::actions!(
    herdr_uikit_menu_tests,
    [
        CloseTab, Copy, Find, NewTab, Paste, Quit, Redo, Undo, Unbound
    ]
);

fn keymap() -> Keymap {
    Keymap::new(vec![
        KeyBinding::new("cmd-t", NewTab, None),
        // Multi-keystroke bindings cannot be key commands; a binding in a
        // context the default one does not match loses to one that does.
        KeyBinding::new("cmd-k w", CloseTab, None),
        KeyBinding::new("ctrl-w", CloseTab, Some("Terminal")),
        KeyBinding::new("cmd-w", CloseTab, Some("Pane")),
        // With no binding in a matching context, the first one is shown.
        KeyBinding::new("ctrl-f", Find, Some("Terminal")),
        KeyBinding::new("cmd-f", Find, Some("Terminal")),
        // Keys that keep going to the text system first get no shortcut.
        KeyBinding::new("escape", Quit, None),
        KeyBinding::new("cmd-z", Undo, None),
    ])
}

/// The shortcut of command `index` as modifiers and key; `Keystroke::unparse`
/// spells the command modifier only on macOS.
fn shortcut_of(model: &MenuModel, index: usize) -> Option<(Modifiers, String)> {
    let shortcut = model.command(index)?.shortcut.as_ref()?;
    Some((shortcut.keystroke.modifiers, shortcut.keystroke.key.clone()))
}

fn bound(source: &str) -> Option<(Modifiers, String)> {
    let keystroke = keystroke(source);
    Some((keystroke.modifiers, keystroke.key))
}

fn edit_menu() -> Menu {
    Menu::new("Edit").items([
        MenuItem::os_action("Undo", Undo, OsAction::Undo),
        MenuItem::os_action("Redo", Redo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Copy", Copy, OsAction::Copy),
        MenuItem::os_action("Paste", Paste, OsAction::Paste),
    ])
}

#[test]
fn separators_split_items_into_groups() {
    let menu = Menu::new("File").items([
        MenuItem::separator(),
        MenuItem::action("New Tab", NewTab),
        MenuItem::separator(),
        MenuItem::separator(),
        MenuItem::action("Close Tab", CloseTab),
        MenuItem::action("Find", Find),
        MenuItem::separator(),
    ]);
    let model = MenuModel::new(&[menu], &keymap());

    let [file] = model.menus() else {
        panic!("one menu was set");
    };
    assert_eq!(file.title, "File");
    assert_eq!(
        file.groups,
        vec![
            vec![Element::Command(0)],
            vec![Element::Command(1), Element::Command(2)],
        ],
        "empty groups from leading, doubled, and trailing separators are dropped"
    );
}

#[test]
fn commands_are_numbered_across_menus_and_submenus() {
    let menus = [
        Menu::new("Herdr").items([MenuItem::action("Quit", Quit)]),
        Menu::new("File").items([
            MenuItem::action("New Tab", NewTab),
            MenuItem::submenu(Menu::new("More").items([MenuItem::action("Close Tab", CloseTab)])),
        ]),
    ];
    let model = MenuModel::new(&menus, &keymap());

    let titles: Vec<_> = (0..3)
        .filter_map(|index| model.command(index))
        .map(|command| command.title.as_str())
        .collect();
    assert_eq!(titles, ["Quit", "New Tab", "Close Tab"]);
    assert!(model.command(3).is_none());
    assert_eq!(
        model.menus()[1].groups,
        vec![vec![
            Element::Command(1),
            Element::Submenu(Submenu {
                groups: vec![vec![Element::Command(2)]],
                title: "More".to_owned(),
            }),
        ]]
    );
}

#[test]
fn shortcuts_come_from_single_keystroke_bindings_in_a_matching_context() {
    let menu = Menu::new("File").items([
        MenuItem::action("New Tab", NewTab),
        MenuItem::action("Close Tab", CloseTab),
        MenuItem::action("Find", Find),
        MenuItem::action("Quit", Quit),
        MenuItem::action("Unbound", Unbound),
    ]);
    let model = MenuModel::new(&[menu], &keymap());

    assert_eq!(shortcut_of(&model, 0), bound("cmd-t"));
    assert_eq!(shortcut_of(&model, 1), bound("cmd-w"));
    assert_eq!(shortcut_of(&model, 2), bound("ctrl-f"));
    assert_eq!(shortcut_of(&model, 3), None, "escape stays with the IME");
    assert_eq!(shortcut_of(&model, 4), None);
}

#[test]
fn standard_editing_items_stay_uikits_and_undo_stays_gpuis() {
    let model = MenuModel::new(&[edit_menu()], &keymap());

    assert_eq!(
        model.menus()[0].groups,
        vec![
            vec![Element::Command(0), Element::Command(1)],
            vec![
                Element::Standard {
                    edit: StandardEdit::Copy,
                    title: "Copy".to_owned(),
                },
                Element::Standard {
                    edit: StandardEdit::Paste,
                    title: "Paste".to_owned(),
                },
            ],
        ]
    );
    assert_eq!(shortcut_of(&model, 0), bound("cmd-z"));
}

#[test]
fn a_disabled_menu_disables_everything_in_it() {
    let menu = Menu::new("File")
        .items([
            MenuItem::action("New Tab", NewTab),
            MenuItem::submenu(Menu::new("More").items([MenuItem::action("Find", Find)])),
        ])
        .disabled(true);
    let enabled = Menu::new("View").items([
        MenuItem::action("Close Tab", CloseTab).disabled(true),
        MenuItem::action("Quit", Quit).checked(true),
    ]);
    let model = MenuModel::new(&[menu, enabled], &keymap());

    let flags: Vec<_> = (0..4)
        .filter_map(|index| model.command(index))
        .map(|command| (command.disabled, command.checked))
        .collect();
    assert_eq!(
        flags,
        [(true, false), (true, false), (true, false), (false, true)]
    );
}

#[test]
fn system_menus_and_empty_submenus_are_left_out() {
    let menu = Menu::new("Herdr").items([
        MenuItem::SystemMenu(OsMenu {
            name: "Services".into(),
            menu_type: SystemMenuType::Services,
        }),
        MenuItem::submenu(Menu::new("Empty")),
    ]);
    let model = MenuModel::new(&[menu], &keymap());

    assert!(model.menus()[0].is_empty());
}

#[test]
fn a_repeated_standard_editing_item_becomes_a_gpui_command() {
    let other = Menu::new("Terminal").items([
        MenuItem::os_action("Copy", Copy, OsAction::Copy),
        MenuItem::submenu(Menu::new("More").items([MenuItem::os_action(
            "Paste",
            Paste,
            OsAction::Paste,
        )])),
    ]);
    let model = MenuModel::new(&[edit_menu(), other], &keymap());

    // Undo and Redo are commands 0 and 1; the repeats follow.
    assert_eq!(
        model.menus()[1].groups,
        vec![vec![
            Element::Command(2),
            Element::Submenu(Submenu {
                groups: vec![vec![Element::Command(3)]],
                title: "More".to_owned(),
            }),
        ]]
    );
    let titles: Vec<_> = (2..4)
        .filter_map(|index| model.command(index))
        .map(|command| command.title.as_str())
        .collect();
    assert_eq!(titles, ["Copy", "Paste"]);
}

#[test]
fn undo_or_redo_items_replace_uikits() {
    assert!(MenuModel::new(&[edit_menu()], &keymap()).has_undo_redo());

    let without = Menu::new("Edit").items([MenuItem::os_action("Copy", Copy, OsAction::Copy)]);
    assert!(!MenuModel::new(&[without], &keymap()).has_undo_redo());
}
