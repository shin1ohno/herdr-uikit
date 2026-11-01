use objc2::{runtime::AnyClass, sel};
use objc2_ui_kit::UIResponder;

use super::*;

#[test]
fn the_app_delegate_is_named_without_the_crate_version() {
    assert_eq!(class_name().to_string(), "HerdrAppDelegate");
}

#[test]
fn the_app_delegate_is_a_responder_so_uikit_asks_it_for_the_main_menu() {
    let class: &AnyClass = AppDelegate::class();

    assert!(
        class
            .superclass()
            .is_some_and(|superclass| ptr::eq(superclass, UIResponder::class()))
    );
    for selector in [
        sel!(buildMenuWithBuilder:),
        sel!(validateCommand:),
        sel!(canPerformAction:withSender:),
        menus::action_selector(),
    ] {
        assert!(class.instance_method(selector).is_some(), "{selector:?}");
    }
}
