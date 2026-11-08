use super::*;
use crate::menu::Shortcut;

fn key(source: &str) -> Keystroke {
    Keystroke::parse_with_os(source, gpui::OperatingSystem::Ios)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}

#[test]
fn language_switch_reaches_uikit_before_terminal_and_custom_bindings() {
    for source in ["ctrl-space", "ctrl-shift-space"] {
        for composing in [false, true] {
            assert!(
                uikit_first(&key(source), || composing, || false),
                "{source}, composing={composing}"
            );
        }
    }
}

#[test]
fn language_switch_cannot_be_installed_as_a_menu_key_command() {
    for source in ["ctrl-space", "ctrl-shift-space"] {
        assert!(Shortcut::new(&key(source)).is_none(), "{source}");
    }
}

#[test]
fn terminal_controls_and_app_shortcuts_keep_their_route() {
    for source in ["ctrl-c", "ctrl-b", "ctrl-enter", "cmd-c", "ctrl-alt-space"] {
        assert!(!uikit_first(&key(source), || false, || false), "{source}");
    }
    for source in ["enter", "escape", "left", "right", "up", "down", "space"] {
        assert!(uikit_first(&key(source), || true, || true), "{source}");
    }
    let mut printable = key("n");
    printable.key_char = Some("n".into());
    assert!(uikit_first(&printable, || false, || true));
    assert!(!uikit_first(&printable, || false, || false));
}
