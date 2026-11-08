//! GPUI's menus as the platform keeps them between `set_menus` and UIKit's
//! next `buildMenuWithBuilder:`: titles, separators turned into groups, and
//! one command per action item, numbered so a `UICommand`'s property list
//! can name it.

use gpui::{Action, KeyContext, Keymap, Menu, MenuItem, OsAction};

use super::shortcut::Shortcut;

/// One action item, as the menu shows it and dispatches it.
pub(crate) struct Command {
    pub(crate) action: Box<dyn Action>,
    pub(crate) checked: bool,
    /// Disabled by GPUI, or inside a disabled menu.
    pub(crate) disabled: bool,
    pub(crate) shortcut: Option<Shortcut>,
    pub(crate) title: String,
}

/// UIKit's standard editing commands. Their selectors reach the text input
/// view's `cut:`, `copy:`, `paste:`, and `selectAll:`, which send the app's
/// own key bindings (`text_input::actions`), so they stay UIKit's commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StandardEdit {
    Copy,
    Cut,
    Paste,
    SelectAll,
}

impl StandardEdit {
    fn from_os_action(action: OsAction) -> Option<Self> {
        match action {
            OsAction::Copy => Some(Self::Copy),
            OsAction::Cut => Some(Self::Cut),
            OsAction::Paste => Some(Self::Paste),
            OsAction::SelectAll => Some(Self::SelectAll),
            // The text input view has no undo stack; GPUI's action does it.
            OsAction::Undo | OsAction::Redo => None,
        }
    }
}

/// One entry of a group.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Element {
    /// A command, by its index in [`MenuModel::command`].
    Command(usize),
    /// A standard editing command and the title GPUI gave it.
    Standard {
        edit: StandardEdit,
        title: String,
    },
    Submenu(Submenu),
}

/// A titled menu: its items in groups, where GPUI put separators.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Submenu {
    pub(crate) groups: Vec<Vec<Element>>,
    pub(crate) title: String,
}

/// GPUI's menus, prepared for UIKit.
#[derive(Default)]
pub(crate) struct MenuModel {
    commands: Vec<Command>,
    menus: Vec<Submenu>,
    /// The standard editing commands already placed: UIKit refuses two
    /// commands with one selector and property list, so a repeat becomes an
    /// ordinary command dispatching GPUI's action.
    standard: Vec<StandardEdit>,
    /// GPUI's menus have Undo or Redo, which replace UIKit's.
    undo_redo: bool,
}

impl MenuModel {
    /// Prepares `menus`, taking each item's shortcut from `keymap`, which
    /// GPUI only hands over here.
    pub(crate) fn new(menus: &[Menu], keymap: &Keymap) -> Self {
        let contexts = default_contexts();
        let mut model = Self::default();
        for menu in menus {
            let submenu = model.submenu(menu, false, keymap, &contexts);
            model.menus.push(submenu);
        }
        model
    }

    /// The top-level menus, in GPUI's order.
    pub(crate) fn menus(&self) -> &[Submenu] {
        &self.menus
    }

    pub(crate) fn command(&self, index: usize) -> Option<&Command> {
        self.commands.get(index)
    }

    /// Whether GPUI's menus have an Undo or Redo item (`OsAction`), so
    /// UIKit's own Undo and Redo give way.
    pub(crate) fn has_undo_redo(&self) -> bool {
        self.undo_redo
    }

    fn submenu(
        &mut self,
        menu: &Menu,
        parent_disabled: bool,
        keymap: &Keymap,
        contexts: &[KeyContext],
    ) -> Submenu {
        let disabled = parent_disabled || menu.disabled;
        let mut groups = vec![Vec::new()];
        for item in &menu.items {
            let element = match item {
                MenuItem::Separator => {
                    groups.push(Vec::new());
                    continue;
                }
                // Services and other macOS system menus have no iPadOS
                // counterpart.
                MenuItem::SystemMenu(_) => continue,
                MenuItem::Submenu(submenu) => {
                    Element::Submenu(self.submenu(submenu, disabled, keymap, contexts))
                }
                MenuItem::Action {
                    name,
                    action,
                    os_action,
                    checked,
                    disabled: item_disabled,
                } => match self.standard_edit(*os_action) {
                    Some(edit) => Element::Standard {
                        edit,
                        title: name.to_string(),
                    },
                    None => {
                        self.commands.push(Command {
                            action: action.boxed_clone(),
                            checked: *checked,
                            disabled: disabled || *item_disabled,
                            shortcut: shortcut(keymap, action.as_ref(), contexts),
                            title: name.to_string(),
                        });
                        Element::Command(self.commands.len() - 1)
                    }
                },
            };
            if let Some(group) = groups.last_mut() {
                group.push(element);
            }
        }
        groups.retain(|group| {
            group.iter().any(|element| match element {
                Element::Submenu(submenu) => !submenu.is_empty(),
                Element::Command(_) | Element::Standard { .. } => true,
            })
        });
        Submenu {
            groups,
            title: menu.name.to_string(),
        }
    }
}

impl MenuModel {
    /// The standard editing command an item with `os_action` becomes, unless
    /// an earlier item already became it.
    fn standard_edit(&mut self, os_action: Option<OsAction>) -> Option<StandardEdit> {
        if matches!(os_action, Some(OsAction::Undo | OsAction::Redo)) {
            self.undo_redo = true;
        }
        let edit = os_action.and_then(StandardEdit::from_os_action)?;
        if self.standard.contains(&edit) {
            return None;
        }
        self.standard.push(edit);
        Some(edit)
    }
}

impl Submenu {
    pub(crate) fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

/// The context macOS GPUI evaluates menu bindings in: an editor in a pane in
/// a workspace, so a binding scoped to any of them is shown.
fn default_contexts() -> Vec<KeyContext> {
    let mut workspace = KeyContext::new_with_defaults(gpui::OperatingSystem::Ios);
    workspace.add("Workspace");
    let mut pane = KeyContext::new_with_defaults(gpui::OperatingSystem::Ios);
    pane.add("Pane");
    let mut editor = KeyContext::new_with_defaults(gpui::OperatingSystem::Ios);
    editor.add("Editor");
    pane.extend(&editor);
    workspace.extend(&pane);
    vec![workspace]
}

/// The shortcut shown for `action`, chosen as macOS GPUI chooses its key
/// equivalent: among single-keystroke bindings, the first whose context
/// matches, else the first. Like macOS, this deliberately uses the earliest
/// binding, where display elsewhere prefers the latest
/// (zed-industries/zed#23621).
fn shortcut(keymap: &Keymap, action: &dyn Action, contexts: &[KeyContext]) -> Option<Shortcut> {
    let mut first = None;
    for binding in keymap.bindings_for_action(action) {
        let [keystroke] = binding.keystrokes() else {
            continue;
        };
        let matches = binding
            .predicate()
            .is_none_or(|predicate| predicate.eval(contexts));
        if matches {
            return Shortcut::new(keystroke.inner());
        }
        first.get_or_insert(keystroke.inner());
    }
    first.and_then(Shortcut::new)
}
