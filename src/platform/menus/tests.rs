use super::*;

mod commands;

const ENABLED: Validation = Validation::Show {
    enabled: true,
    checked: false,
};
const DISABLED: Validation = Validation::Show {
    enabled: false,
    checked: false,
};

fn never_asked() -> Called<bool> {
    panic!("GPUI's callback must not be asked");
}

#[test]
fn a_command_from_replaced_menus_is_disabled() {
    assert_eq!(validation(None, false, never_asked), DISABLED);
}

#[test]
fn a_command_gpui_disabled_stays_disabled_without_asking() {
    assert_eq!(
        validation(Some((true, false)), false, never_asked),
        DISABLED
    );
    assert_eq!(
        validation(Some((true, true)), true, never_asked),
        Validation::Show {
            enabled: false,
            checked: true,
        }
    );
}

#[test]
fn gpui_decides_whether_the_action_is_available() {
    assert_eq!(
        validation(Some((false, false)), false, || Called::Done(true)),
        ENABLED
    );
    assert_eq!(
        validation(Some((false, false)), false, || Called::Done(false)),
        DISABLED
    );
    assert_eq!(
        validation(Some((false, true)), false, || Called::Done(true)),
        Validation::Show {
            enabled: true,
            checked: true,
        }
    );
}

#[test]
fn with_no_callback_registered_a_command_is_disabled_as_on_macos() {
    assert_eq!(
        validation(Some((false, false)), false, || Called::Unregistered),
        DISABLED
    );
}

#[test]
fn while_gpui_runs_a_command_keeps_what_it_showed() {
    assert_eq!(
        validation(Some((false, false)), true, never_asked),
        Validation::Keep
    );
    assert_eq!(
        validation(Some((false, false)), false, || Called::Running),
        Validation::Keep
    );
}
