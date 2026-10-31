use std::ptr;

use objc2::runtime::AnyClass;

use super::*;

#[test]
fn the_scene_delegate_is_registered_under_its_stable_name() {
    register();

    let class = AnyClass::get(c"HerdrSceneDelegate");

    assert!(class.is_some_and(|class| ptr::eq(class, SceneDelegate::class())));
}
