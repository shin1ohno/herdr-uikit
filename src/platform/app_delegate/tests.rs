use super::*;

#[test]
fn the_app_delegate_is_named_without_the_crate_version() {
    assert_eq!(class_name().to_string(), "HerdrAppDelegate");
}
