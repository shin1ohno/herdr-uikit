use std::ptr;

use objc2::runtime::AnyClass;

use super::*;

#[test]
fn the_scene_delegate_is_registered_under_its_stable_name() {
    register();

    let class = AnyClass::get(c"HerdrSceneDelegate");

    assert!(class.is_some_and(|class| ptr::eq(class, SceneDelegate::class())));
}

#[test]
fn a_mac_scene_asks_for_controls_in_its_content() {
    assert_eq!(
        windowing_controls::control_style(true),
        windowing_controls::ControlStyle::Unified
    );
}

#[test]
fn an_ipad_scene_keeps_the_automatic_style() {
    assert_eq!(
        windowing_controls::control_style(false),
        windowing_controls::ControlStyle::Automatic
    );
}

#[test]
fn the_simulator_is_not_a_mac_running_the_ipad_app() {
    assert!(!windowing_controls::ios_app_on_mac());
}

/// UIKit makes a new object per call and does not define equality for it,
/// so the test reads the type UIKit names in its description, which the
/// object's `Debug` output is.
#[test]
fn each_style_maps_to_its_uikit_object() {
    let automatic = format!(
        "{:?}",
        windowing_controls::uikit_style(windowing_controls::ControlStyle::Automatic)
    );
    let unified = format!(
        "{:?}",
        windowing_controls::uikit_style(windowing_controls::ControlStyle::Unified)
    );

    assert!(automatic.contains("type: automatic"), "{automatic}");
    assert!(unified.contains("type: unified"), "{unified}");
}

#[test]
fn the_scene_delegate_answers_the_windowing_control_style() {
    register();

    let delegate_class = SceneDelegate::class();

    assert!(delegate_class.responds_to(objc2::sel!(preferredWindowingControlStyleForScene:)));
}
