//! UIKit's side of the menus, exercised with real `UICommand`s and
//! `UIKeyCommand`s on the main thread: validation, what a performed command
//! names, delivery of its action, and what `get_menus` returns.

#![allow(
    unsafe_code,
    reason = "a Mach-O test initializer exercises UIKit on the main thread before libtest starts workers"
)]

use std::{cell::RefCell, rc::Rc, sync::OnceLock};

use gpui::{Action, KeyBinding, Keymap, Menu, MenuItem, OwnedMenuItem};
use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSNumber, NSString};
use objc2_ui_kit::{UICommand, UIKeyCommand, UIKeyModifierFlags, UIMenuElementAttributes};

use super::*;
use crate::gpui_call::InGpui;

gpui::actions!(herdr_uikit_menu_command_tests, [CloseTab, NewTab, Zoom]);

/// What the initializer found, `None` until it ran on the main thread.
static OUTCOME: OnceLock<Result<Vec<String>, String>> = OnceLock::new();

// libtest blocks the main thread while its test workers run, so dispatching
// this UIKit check from a #[test] would deadlock. Mach-O initializers run on
// the main thread before the harness; keep only the failures as strings.
// SAFETY: __mod_init_func contains C-ABI functions taking no arguments and
// returning nothing. This function has that signature and stores no UIKit
// object beyond its invocation.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CHECK_MENU_COMMANDS: extern "C" fn() = check_menu_commands;

extern "C" fn check_menu_commands() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let outcome = std::panic::catch_unwind(|| {
        objc2::rc::autoreleasepool(|_| {
            let mut failures = Vec::new();
            run(mtm, &mut failures);
            failures
        })
    })
    .map_err(|_| "the check panicked".to_owned());
    let _ = OUTCOME.set(outcome);
}

fn property_list(generation: u32, index: usize) -> Option<Retained<NSNumber>> {
    let tag = MenuTag::new(generation, index)?;
    Some(NSNumber::numberWithUnsignedLongLong(u64::from(tag)))
}

fn command(mtm: MainThreadMarker, generation: u32, index: usize) -> Option<Retained<UICommand>> {
    let list = property_list(generation, index)?;
    let list: &AnyObject = &list;
    // SAFETY: as in `Context::command`: the selector takes the sender and
    // the property list is an NSNumber.
    Some(unsafe {
        UICommand::commandWithTitle_image_action_propertyList(
            &NSString::from_str("Item"),
            None,
            action_selector(),
            Some(list),
            mtm,
        )
    })
}

fn key_command(
    mtm: MainThreadMarker,
    generation: u32,
    index: usize,
) -> Option<Retained<UIKeyCommand>> {
    let list = property_list(generation, index)?;
    let list: &AnyObject = &list;
    // SAFETY: as in `Context::command`.
    Some(unsafe {
        UIKeyCommand::commandWithTitle_image_action_input_modifierFlags_propertyList(
            &NSString::from_str("Item"),
            None,
            action_selector(),
            &NSString::from_str("t"),
            UIKeyModifierFlags::Command,
            Some(list),
            mtm,
        )
    })
}

fn disabled(command: &UICommand) -> bool {
    command
        .attributes()
        .contains(UIMenuElementAttributes::Disabled)
}

fn check(failures: &mut Vec<String>, ok: bool, what: &str) {
    if !ok {
        failures.push(what.to_owned());
    }
}

fn run(mtm: MainThreadMarker, failures: &mut Vec<String>) {
    let Some(stray) = command(mtm, 1, 0) else {
        failures.push("no command".to_owned());
        return;
    };
    validate(&stray);
    check(
        failures,
        disabled(&stray),
        "with no platform, a command is disabled",
    );

    let shared = Rc::new(Shared::default());
    if !shared::install(Rc::clone(&shared)) {
        failures.push("the test platform was already installed".to_owned());
        return;
    }
    let menus = vec![Menu::new("File").items([
        MenuItem::action("New Tab", NewTab),
        MenuItem::action("Close Tab", CloseTab),
        MenuItem::action("Zoom", Zoom).checked(true),
    ])];
    let keymap = Keymap::new(vec![KeyBinding::new("cmd-t", NewTab, None)]);
    shared.menus.borrow_mut().set(menus, &keymap);
    let generation = shared.menus.borrow().generation;

    let owned = shared.menus.borrow().owned();
    check(
        failures,
        owned.as_ref().is_some_and(|menus| {
            menus.len() == 1
                && menus[0].name.as_ref() == "File"
                && matches!(menus[0].items.as_slice(), [OwnedMenuItem::Action { name, .. }, ..] if name == "New Tab")
        }),
        "get_menus returns the menus as set",
    );

    let (Some(new_tab), Some(close_tab), Some(zoom), Some(old)) = (
        command(mtm, generation, 0),
        command(mtm, generation, 1),
        command(mtm, generation, 2),
        command(mtm, generation.wrapping_sub(1), 0),
    ) else {
        failures.push("no commands".to_owned());
        return;
    };

    validate(&new_tab);
    check(
        failures,
        disabled(&new_tab),
        "with no callback, a command is disabled",
    );

    shared
        .validate_app_menu_command
        .set(Box::new(|action| !action.partial_eq(&CloseTab)));
    for command in [&new_tab, &close_tab, &zoom, &old] {
        validate(command);
    }
    check(
        failures,
        !disabled(&new_tab),
        "an available action is enabled",
    );
    check(
        failures,
        disabled(&close_tab),
        "an unavailable action is disabled",
    );
    check(
        failures,
        zoom.state() == UIMenuElementState::On,
        "a checked item shows a check mark",
    );
    check(
        failures,
        disabled(&old),
        "a command from older menus is disabled",
    );

    // While GPUI runs, its callback is not asked and the command keeps what
    // it showed.
    shared.validate_app_menu_command.set(Box::new(|_| false));
    {
        let _in_gpui = InGpui::enter();
        validate(&new_tab);
    }
    check(
        failures,
        !disabled(&new_tab),
        "validation inside GPUI keeps the command",
    );

    let keyed = key_command(mtm, generation, 0)
        .and_then(|command| Performed::new(Some(&command)))
        .and_then(|performed| {
            performed
                .shortcut()
                .map(|shortcut| shortcut.keystroke.key.clone())
        });
    check(
        failures,
        keyed.as_deref() == Some("t"),
        "a key command carries its item's shortcut",
    );
    let picked = Performed::new(Some(&new_tab));
    check(
        failures,
        picked
            .as_ref()
            .is_some_and(|performed| performed.shortcut().is_none()),
        "a command without a key input has no shortcut",
    );
    check(
        failures,
        Performed::new(None).is_none(),
        "no sender names nothing",
    );

    let Some(new_tab_tag) = MenuTag::new(generation, 0) else {
        failures.push("no tag".to_owned());
        return;
    };
    check(
        failures,
        !deliver(&shared, new_tab_tag),
        "with no action callback, nothing is delivered",
    );
    let delivered = Rc::new(RefCell::new(Vec::new()));
    let record = Rc::clone(&delivered);
    shared.app_menu_action.set(Box::new(move |action| {
        record.borrow_mut().push(action.name())
    }));
    check(
        failures,
        deliver(&shared, new_tab_tag),
        "an action is delivered",
    );
    let stale = MenuTag::new(generation.wrapping_sub(1), 0);
    check(
        failures,
        stale.is_some_and(|tag| !deliver(&shared, tag)),
        "a command from older menus delivers nothing",
    );
    check(
        failures,
        *delivered.borrow() == [NewTab.name()],
        "the item's own action reaches GPUI's callback",
    );
}

fn outcome() -> Vec<String> {
    match OUTCOME.get() {
        Some(Ok(failures)) => failures.clone(),
        Some(Err(error)) => vec![error.clone()],
        None => vec!["the main-thread check did not run".to_owned()],
    }
}

#[test]
fn uikit_commands_validate_name_and_deliver_gpuis_actions() {
    assert_eq!(outcome(), Vec::<String>::new());
}
