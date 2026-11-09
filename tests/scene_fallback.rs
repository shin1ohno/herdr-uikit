#[path = "../src/scene_fallback.rs"]
mod scene_fallback;

#[test]
fn before_application_no_scene_query_runs() {
    let scenes: Vec<u8> = scene_fallback::scenes_or_empty(None::<()>, |_| {
        panic!("must not query UIKit before the application exists")
    });
    assert!(scenes.is_empty());
}

#[test]
fn existing_application_preserves_scene_order_and_duplicates() {
    let scenes = scene_fallback::scenes_or_empty(Some(vec![3, 1, 3]), |scenes| scenes);
    assert_eq!(scenes, vec![3, 1, 3]);
}
