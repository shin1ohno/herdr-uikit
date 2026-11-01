use objc2::{runtime::AnyClass, sel};

use super::*;

#[test]
fn the_application_is_named_without_the_crate_version() {
    assert_eq!(class_name().to_string(), "HerdrApplication");
}

#[test]
fn the_application_is_a_uikit_application_that_sees_every_event() {
    let class: &AnyClass = Application::class();

    assert!(
        class
            .superclass()
            .is_some_and(|superclass| std::ptr::eq(superclass, UIApplication::class()))
    );
    assert!(class.instance_method(sel!(sendEvent:)).is_some());
}

#[test]
fn no_event_is_being_delivered_outside_send_event() {
    assert_eq!(event_source(), EventSource::Unknown);
}
