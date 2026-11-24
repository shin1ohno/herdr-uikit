//! The Mac's control-letter key commands (`control_keys`), built as real
//! `UIKeyCommand`s on the main thread. On a Mac they make UIKit deliver
//! these keys as presses; that delivery is the Mac check recorded in the
//! pull request, not something the Simulator can show.

#![allow(
    unsafe_code,
    reason = "a Mach-O test initializer exercises UIKit on the main thread before libtest starts workers"
)]

use std::sync::OnceLock;

use objc2::MainThreadMarker;
use objc2_ui_kit::UIKeyModifierFlags;

use super::*;
use crate::window::text_input::control_keys::{
    LETTERS, action, control_character, key_commands, on_mac,
};

/// What the initializer found, `None` until it ran on the main thread.
static OUTCOME: OnceLock<Result<Vec<String>, String>> = OnceLock::new();

// libtest blocks the main thread while its workers run, so this runs as a
// Mach-O initializer on the main thread before the harness starts.
// SAFETY: __mod_init_func contains C-ABI functions taking no arguments and
// returning nothing. This function has that signature and keeps no UIKit
// object beyond its invocation.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CHECK_CONTROL_KEY_COMMANDS: extern "C" fn() = check_control_key_commands;

extern "C" fn check_control_key_commands() {
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

fn run(mtm: MainThreadMarker, failures: &mut Vec<String>) {
    if key_commands(false, mtm).is_some() {
        failures.push("off a Mac the view lists key commands".to_owned());
    }
    let Some(commands) = key_commands(true, mtm) else {
        failures.push("on a Mac the view lists no key commands".to_owned());
        return;
    };
    let inputs: Vec<String> = commands
        .iter()
        .filter_map(|command| command.input().map(|input| input.to_string()))
        .collect();
    let expected: Vec<String> = LETTERS.iter().map(char::to_string).collect();
    if inputs != expected {
        failures.push(format!("inputs {inputs:?}, expected {expected:?}"));
    }
    for command in commands.iter() {
        let input = command.input().map(|input| input.to_string());
        if command.modifierFlags() != UIKeyModifierFlags::Control {
            failures.push(format!("{input:?} is not control alone"));
        }
        if !command.wantsPriorityOverSystemBehavior() {
            failures.push(format!("{input:?} yields to the system"));
        }
        if command.allowsAutomaticLocalization() {
            failures.push(format!("{input:?} may be remapped"));
        }
        // SAFETY: a key command always has an action selector.
        if unsafe { command.action() } != Some(action()) {
            failures.push(format!("{input:?} has another action"));
        }
    }
}

#[test]
fn a_mac_lists_priority_key_commands_for_the_bound_control_letters() {
    match OUTCOME.get() {
        Some(Ok(failures)) => assert!(failures.is_empty(), "{failures:#?}"),
        Some(Err(error)) => panic!("{error}"),
        None => panic!("the main-thread check did not run"),
    }
}

#[test]
fn the_ipad_lists_no_control_key_commands() {
    // The Simulator is an iPad, not an iPad app on a Mac: the view's
    // `keyCommands` is `key_commands(on_mac(), ..)`, which is empty here,
    // so the iPad's press route is unchanged.
    assert!(!on_mac());
}

#[test]
fn key_command_inputs_become_their_control_characters() {
    let expected = [
        ('a', '\u{1}'),
        ('b', '\u{2}'),
        ('d', '\u{4}'),
        ('e', '\u{5}'),
        ('f', '\u{6}'),
        ('h', '\u{8}'),
        ('k', '\u{b}'),
        ('l', '\u{c}'),
        ('n', '\u{e}'),
        ('o', '\u{f}'),
        ('p', '\u{10}'),
        ('t', '\u{14}'),
        ('v', '\u{16}'),
        ('y', '\u{19}'),
    ];
    assert_eq!(LETTERS.len(), expected.len());
    for (letter, control) in expected {
        assert_eq!(control_character(&letter.to_string()), Some(control));
    }
    // Control-A reaches a terminal as control-A, which a Herdr prefix is.
    let key = control_character("a").and_then(|c| keymap::control_keystroke(&c.to_string()));
    assert!(
        key.as_ref()
            .is_some_and(|key| key.key == "a" && key.modifiers.control),
        "{key:?}"
    );
    for input in ["c", "A", "ab", "", "1"] {
        assert_eq!(control_character(input), None, "{input:?}");
    }
}
