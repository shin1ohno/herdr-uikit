//! GPUI's menus in UIKit's main menu: the menus the platform keeps between
//! `set_menus` and `buildMenuWithBuilder:`, the `UICommand`s built from them,
//! and the answers to UIKit's `validateCommand:` and to a performed command.
//! `crate::menu` holds the logic that needs no UIKit object, including the
//! keyboard precedence the commands keep.
//!
//! Every command GPUI dispatches has one selector, `herdrMenuAction:`, and
//! carries a [`MenuTag`] as an `NSNumber` property list. The drawing view and
//! the application delegate implement the selector, so the command reaches
//! the focused window's view first and the delegate when no window has focus.
//!
//! UIKit's own application menu items (About, Settings, Services, Hide,
//! Quit, as iPadOS provides them) stay: the system performs them. GPUI items
//! whose platform call does nothing on iPadOS (`quit`, `hide`,
//! `hide_other_apps`) would look usable and do nothing, so the app leaves
//! them out of its iOS menus, as herdr-gpui does for Hide. UIKit's Undo and
//! Redo give way to GPUI's when GPUI's menus have them, so the menu has one
//! of each and GPUI's show the shortcuts.

#![allow(
    unsafe_code,
    reason = "reads UIKit's menu identifier and key input constants, and builds \
              UICommands from selectors"
)]

use gpui::{Keymap, Menu, OwnedMenu};
use objc2::{
    MainThreadMarker, Message,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject, Sel},
    sel,
};
use objc2_foundation::{NSArray, NSNumber, NSString};
use objc2_ui_kit::{
    UICommand, UIKeyCommand, UIKeyInputDelete, UIKeyInputDownArrow, UIKeyInputEnd,
    UIKeyInputEscape, UIKeyInputF1, UIKeyInputF2, UIKeyInputF3, UIKeyInputF4, UIKeyInputF5,
    UIKeyInputF6, UIKeyInputF7, UIKeyInputF8, UIKeyInputF9, UIKeyInputF10, UIKeyInputF11,
    UIKeyInputF12, UIKeyInputHome, UIKeyInputLeftArrow, UIKeyInputPageDown, UIKeyInputPageUp,
    UIKeyInputRightArrow, UIKeyInputUpArrow, UIMenu, UIMenuApplication, UIMenuBuilder, UIMenuEdit,
    UIMenuElement, UIMenuElementAttributes, UIMenuElementState, UIMenuFile, UIMenuHelp,
    UIMenuIdentifier, UIMenuOptions, UIMenuRoot, UIMenuSystem, UIMenuUndoRedo, UIMenuView,
    UIMenuWindow,
};

use super::{
    shared::{self, Shared},
    slot::Called,
};
use crate::gpui_call;
use crate::menu::{
    self, Command, Element, KeyInput, MenuModel, MenuTag, NamedInput, Placement, Shortcut,
    ShortcutSet, StandardEdit, StandardMenu, Submenu,
};

/// GPUI's menus as last set, numbered by generation.
#[derive(Default)]
pub(super) struct Menus {
    generation: u32,
    model: MenuModel,
    owned: Option<Vec<OwnedMenu>>,
}

impl Menus {
    /// Replaces the menus. The keymap is only available here, so shortcuts
    /// are chosen now; UIKit builds the commands when it next rebuilds.
    pub(super) fn set(&mut self, menus: Vec<Menu>, keymap: &Keymap) {
        self.generation = self.generation.wrapping_add(1);
        self.model = MenuModel::new(&menus, keymap);
        self.owned = Some(menus.into_iter().map(Menu::owned).collect());
    }

    pub(super) fn owned(&self) -> Option<Vec<OwnedMenu>> {
        self.owned.clone()
    }

    /// The command a tag names, unless the tag is from older menus.
    fn command(&self, tag: MenuTag) -> Option<&Command> {
        (tag.generation == self.generation)
            .then(|| self.model.command(tag.index()))
            .flatten()
    }
}

/// The selector of every command GPUI dispatches.
pub(crate) fn action_selector() -> Sel {
    sel!(herdrMenuAction:)
}

/// Asks UIKit to call `buildMenuWithBuilder:` again at a suitable time; safe
/// inside a GPUI update, as nothing runs before this returns.
pub(crate) fn request_rebuild(mtm: MainThreadMarker) {
    UIMenuSystem::mainSystem(mtm).setNeedsRebuild();
}

/// A command UIKit performed.
pub(crate) struct Performed {
    /// The item's shortcut, when the command is a key command.
    shortcut: Option<Shortcut>,
    tag: MenuTag,
}

impl Performed {
    /// What `sender`, the command UIKit passed to `herdrMenuAction:`, names.
    pub(crate) fn new(sender: Option<&AnyObject>) -> Option<Self> {
        let command = sender?.downcast_ref::<UICommand>()?;
        let tag = tag(command)?;
        // A command built without a key input (its shortcut was taken) can
        // only have been chosen from the menu.
        let keyed = sender
            .and_then(|sender| sender.downcast_ref::<UIKeyCommand>())
            .and_then(UIKeyCommand::input)
            .is_some_and(|input| input.length() > 0);
        let shortcut = keyed
            .then(|| {
                let shared = shared::current()?;
                let menus = shared.menus.borrow();
                menus.command(tag)?.shortcut.clone()
            })
            .flatten();
        Some(Self { shortcut, tag })
    }

    pub(crate) fn shortcut(&self) -> Option<&Shortcut> {
        self.shortcut.as_ref()
    }

    /// Dispatches the command's action through GPUI's menu callback, on the
    /// main queue after the UIKit call on the stack returns, so GPUI's app
    /// borrow is never nested.
    pub(crate) fn dispatch_action(&self) {
        let tag = self.tag;
        shared::later(move |shared| {
            deliver(shared, tag);
        });
    }
}

/// Hands the action of the command `tag` names to GPUI. Returns whether
/// GPUI's callback ran.
fn deliver(shared: &Shared, tag: MenuTag) -> bool {
    let action = {
        let menus = shared.menus.borrow();
        menus
            .command(tag)
            .map(|command| command.action.boxed_clone())
    };
    let Some(action) = action else {
        tracing::debug!("a menu command from replaced menus was performed");
        return false;
    };
    let delivered = shared
        .app_menu_action
        .call(|callback| callback(action.as_ref()))
        .is_some();
    if !delivered {
        tracing::warn!("a menu command was performed before GPUI registered for it");
    }
    delivered
}

/// What `validateCommand:` shows for one of GPUI's commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Validation {
    /// Leave the command as it is: GPUI cannot be asked now.
    Keep,
    Show {
        enabled: bool,
        checked: bool,
    },
}

/// Decides [`Validation`] for a command, given `entry`, its GPUI `disabled`
/// and `checked` flags (`None` when its menus were replaced or the platform
/// is not running), whether GPUI is running further up the stack, and
/// `available`, which asks GPUI's callback whether the action is available.
///
/// As on macOS, a command is disabled when GPUI registered no callback. When
/// GPUI is already running, its app is borrowed and its callback would
/// panic, so the command keeps what it showed last.
fn validation(
    entry: Option<(bool, bool)>,
    in_gpui: bool,
    available: impl FnOnce() -> Called<bool>,
) -> Validation {
    let Some((disabled, checked)) = entry else {
        return Validation::Show {
            enabled: false,
            checked: false,
        };
    };
    if disabled {
        return Validation::Show {
            enabled: false,
            checked,
        };
    }
    if in_gpui {
        return Validation::Keep;
    }
    match available() {
        Called::Done(enabled) => Validation::Show { enabled, checked },
        Called::Unregistered => Validation::Show {
            enabled: false,
            checked,
        },
        Called::Running => Validation::Keep,
    }
}

/// `validateCommand:` for a command this platform built: disabled when GPUI
/// disabled it or reports its action unavailable, and checked as GPUI said.
/// Other commands are left as they are.
///
/// GPUI's callback runs synchronously, as macOS GPUI's `validateMenuItem:`
/// does, and borrows GPUI's app, so it is skipped while GPUI runs further up
/// the stack (`crate::gpui_call`), such as when UIKit validates inside a
/// focus change GPUI made.
///
/// Whether UIKit still lets a key command it was told is disabled pass its
/// press on to the responder chain, rather than swallowing it, has to be
/// verified on a device (README, "Menus"): GPUI's keymap must see the press
/// whatever the menu shows.
pub(crate) fn validate(command: &UICommand) {
    // SAFETY: every UICommand has an action selector; UIKit rejects one
    // without when it is created.
    if unsafe { command.action() } != action_selector() {
        return;
    }
    let Some(tag) = tag(command) else {
        return;
    };
    let shared = shared::current();
    let entry = shared.as_ref().and_then(|shared| {
        let menus = shared.menus.borrow();
        let command = menus.command(tag)?;
        Some((command.disabled, command.checked))
    });
    let decided = validation(entry, gpui_call::is_running(), || {
        let Some(shared) = &shared else {
            return Called::Unregistered;
        };
        let action = {
            let menus = shared.menus.borrow();
            menus
                .command(tag)
                .map(|command| command.action.boxed_clone())
        };
        let Some(action) = action else {
            return Called::Unregistered;
        };
        shared
            .validate_app_menu_command
            .try_call(|callback| callback(action.as_ref()))
    });
    let Validation::Show { enabled, checked } = decided else {
        return;
    };
    let mut attributes = command.attributes();
    attributes.set(UIMenuElementAttributes::Disabled, !enabled);
    command.setAttributes(attributes);
    command.setState(state(checked));
}

fn state(checked: bool) -> UIMenuElementState {
    if checked {
        UIMenuElementState::On
    } else {
        UIMenuElementState::Off
    }
}

fn tag(command: &UICommand) -> Option<MenuTag> {
    let number = command.propertyList()?.downcast::<NSNumber>().ok()?;
    Some(MenuTag::from(number.unsignedLongLongValue()))
}

/// Adds GPUI's menus to the main menu UIKit is building. Called from the
/// application delegate's `buildMenuWithBuilder:` for the main system.
pub(super) fn build(builder: &ProtocolObject<dyn UIMenuBuilder>, mtm: MainThreadMarker) {
    let Some(shared) = shared::current() else {
        return;
    };
    let menus = shared.menus.borrow();
    // SAFETY: the identifier is an immutable string UIKit exports.
    let undo_redo = unsafe { UIMenuUndoRedo };
    if menus.model.has_undo_redo() && builder.menuForIdentifier(undo_redo).is_some() {
        builder.removeMenuForIdentifier(undo_redo);
    }
    let mut context = Context {
        generation: menus.generation,
        has_edit: builder
            .menuForIdentifier(identifier(StandardMenu::Edit))
            .is_some(),
        model: &menus.model,
        mtm,
        taken: existing_shortcuts(builder),
    };
    // SAFETY: the identifiers are immutable strings UIKit exports.
    let (application, root) = unsafe { (UIMenuApplication, UIMenuRoot) };
    let mut anchor: Option<Retained<UIMenuIdentifier>> = None;
    for (position, menu) in menus.model.menus().iter().enumerate() {
        let groups = context.groups(menu);
        if groups.is_empty() {
            continue;
        }
        let uikit_title = |standard: StandardMenu| {
            builder
                .menuForIdentifier(identifier(standard))
                .map(|menu| menu.title().to_string())
        };
        let parent = match menu::placement(position, &menu.title, uikit_title) {
            Placement::Application => builder
                .menuForIdentifier(application)
                .map(|_| application.retain()),
            Placement::Standard(standard) => Some(identifier(standard).retain()),
            Placement::Sibling => None,
        };
        if let Some(parent) = parent {
            // In reverse, so each group lands before the ones after it.
            for group in groups.iter().rev() {
                builder.insertChildMenu_atStartOfMenuForIdentifier(group, &parent);
            }
            anchor = Some(parent);
            continue;
        }
        let elements: Vec<Retained<UIMenuElement>> =
            groups.into_iter().map(Retained::into_super).collect();
        let sibling = UIMenu::menuWithTitle_children(
            &NSString::from_str(&menu.title),
            &NSArray::from_retained_slice(&elements),
            mtm,
        );
        match &anchor {
            Some(anchor) => builder.insertSiblingMenu_afterMenuForIdentifier(&sibling, anchor),
            None => builder.insertChildMenu_atEndOfMenuForIdentifier(&sibling, root),
        }
        anchor = Some(sibling.identifier());
    }
}

fn identifier(menu: StandardMenu) -> &'static UIMenuIdentifier {
    // SAFETY: the identifiers are immutable strings UIKit exports.
    unsafe {
        match menu {
            StandardMenu::Edit => UIMenuEdit,
            StandardMenu::File => UIMenuFile,
            StandardMenu::Help => UIMenuHelp,
            StandardMenu::View => UIMenuView,
            StandardMenu::Window => UIMenuWindow,
        }
    }
}

/// The shortcuts of the key commands already in the main menu, UIKit's own.
fn existing_shortcuts(builder: &ProtocolObject<dyn UIMenuBuilder>) -> ShortcutSet {
    fn collect(menu: &UIMenu, taken: &mut ShortcutSet) {
        for child in menu.children().iter() {
            if let Some(submenu) = child.downcast_ref::<UIMenu>() {
                collect(submenu, taken);
            } else if let Some(command) = child.downcast_ref::<UIKeyCommand>()
                && let Some(input) = command.input()
            {
                taken.insert(&input.to_string(), command.modifierFlags());
            }
        }
    }
    let mut taken = ShortcutSet::default();
    // SAFETY: the identifier is an immutable string UIKit exports.
    if let Some(root) = builder.menuForIdentifier(unsafe { UIMenuRoot }) {
        collect(&root, &mut taken);
    }
    taken
}

/// What building the commands of one rebuild needs.
struct Context<'a> {
    generation: u32,
    /// UIKit's Edit menu exists, with the standard editing commands.
    has_edit: bool,
    model: &'a MenuModel,
    mtm: MainThreadMarker,
    taken: ShortcutSet,
}

impl Context<'_> {
    /// A menu's groups as inline menus, which UIKit separates with lines.
    fn groups(&mut self, menu: &Submenu) -> Vec<Retained<UIMenu>> {
        menu.groups
            .iter()
            .filter_map(|group| {
                let elements: Vec<_> = group
                    .iter()
                    .filter_map(|element| self.element(element))
                    .collect();
                (!elements.is_empty()).then(|| {
                    UIMenu::menuWithTitle_image_identifier_options_children(
                        &NSString::new(),
                        None,
                        None,
                        UIMenuOptions::DisplayInline,
                        &NSArray::from_retained_slice(&elements),
                        self.mtm,
                    )
                })
            })
            .collect()
    }

    fn element(&mut self, element: &Element) -> Option<Retained<UIMenuElement>> {
        match element {
            Element::Command(index) => self.command(*index),
            // UIKit's Edit menu already has these commands, with the same
            // selectors and their standard shortcuts.
            Element::Standard { .. } if self.has_edit => None,
            Element::Standard { edit, title } => {
                let selector = match edit {
                    StandardEdit::Copy => sel!(copy:),
                    StandardEdit::Cut => sel!(cut:),
                    StandardEdit::Paste => sel!(paste:),
                    StandardEdit::SelectAll => sel!(selectAll:),
                };
                // SAFETY: the selector is UIKit's standard editing action,
                // taking the sender; the command has no property list.
                let command = unsafe {
                    UICommand::commandWithTitle_image_action_propertyList(
                        &NSString::from_str(title),
                        None,
                        selector,
                        None,
                        self.mtm,
                    )
                };
                Some(Retained::into_super(command))
            }
            Element::Submenu(submenu) => {
                let groups = self.groups(submenu);
                if groups.is_empty() {
                    return None;
                }
                let elements: Vec<Retained<UIMenuElement>> =
                    groups.into_iter().map(Retained::into_super).collect();
                let menu = UIMenu::menuWithTitle_children(
                    &NSString::from_str(&submenu.title),
                    &NSArray::from_retained_slice(&elements),
                    self.mtm,
                );
                Some(Retained::into_super(menu))
            }
        }
    }

    fn command(&mut self, index: usize) -> Option<Retained<UIMenuElement>> {
        let command = self.model.command(index)?;
        let tag = MenuTag::new(self.generation, index)?;
        let property_list = NSNumber::numberWithUnsignedLongLong(u64::from(tag));
        let property_list: &AnyObject = &property_list;
        let title = NSString::from_str(&command.title);
        let shortcut = command.shortcut.as_ref().and_then(|shortcut| {
            let input = input(&shortcut.input)?;
            self.taken
                .insert(&input.to_string(), shortcut.flags)
                .then_some((input, shortcut.flags))
        });
        let element: Retained<UICommand> = match shortcut {
            Some((input, flags)) => {
                // SAFETY: the selector takes the sender and returns nothing,
                // as the drawing view's and the delegate's implementations
                // do; the property list is an NSNumber.
                let key_command = unsafe {
                    UIKeyCommand::commandWithTitle_image_action_input_modifierFlags_propertyList(
                        &title,
                        None,
                        action_selector(),
                        &input,
                        flags,
                        Some(property_list),
                        self.mtm,
                    )
                };
                // GPUI's bindings name keys as typed on the US layout, and
                // the app mirrors its own UI; UIKit must not remap either.
                key_command.setAllowsAutomaticLocalization(false);
                key_command.setAllowsAutomaticMirroring(false);
                Retained::into_super(key_command)
            }
            // SAFETY: as above.
            None => unsafe {
                UICommand::commandWithTitle_image_action_propertyList(
                    &title,
                    None,
                    action_selector(),
                    Some(property_list),
                    self.mtm,
                )
            },
        };
        if command.disabled {
            element.setAttributes(UIMenuElementAttributes::Disabled);
        }
        element.setState(state(command.checked));
        Some(Retained::into_super(element))
    }
}

/// A key input as UIKit spells it, `None` for a function key UIKit has no
/// constant for.
fn input(input: &KeyInput) -> Option<Retained<NSString>> {
    let named = match input {
        KeyInput::Text(text) => return Some(NSString::from_str(text)),
        KeyInput::Named(named) => named,
    };
    // SAFETY: the inputs are immutable strings UIKit exports.
    let constant: &NSString = unsafe {
        match named {
            NamedInput::Delete => UIKeyInputDelete,
            NamedInput::DownArrow => UIKeyInputDownArrow,
            NamedInput::End => UIKeyInputEnd,
            NamedInput::Escape => UIKeyInputEscape,
            NamedInput::Function(1) => UIKeyInputF1,
            NamedInput::Function(2) => UIKeyInputF2,
            NamedInput::Function(3) => UIKeyInputF3,
            NamedInput::Function(4) => UIKeyInputF4,
            NamedInput::Function(5) => UIKeyInputF5,
            NamedInput::Function(6) => UIKeyInputF6,
            NamedInput::Function(7) => UIKeyInputF7,
            NamedInput::Function(8) => UIKeyInputF8,
            NamedInput::Function(9) => UIKeyInputF9,
            NamedInput::Function(10) => UIKeyInputF10,
            NamedInput::Function(11) => UIKeyInputF11,
            NamedInput::Function(12) => UIKeyInputF12,
            NamedInput::Function(_) => return None,
            NamedInput::Home => UIKeyInputHome,
            NamedInput::LeftArrow => UIKeyInputLeftArrow,
            NamedInput::PageDown => UIKeyInputPageDown,
            NamedInput::PageUp => UIKeyInputPageUp,
            NamedInput::RightArrow => UIKeyInputRightArrow,
            NamedInput::UpArrow => UIKeyInputUpArrow,
        }
    };
    Some(constant.retain())
}

#[cfg(test)]
mod tests;
