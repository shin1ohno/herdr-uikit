//! Host tests for the window fit after a scene geometry change (std only).
//! The scene delegate sets exactly the frames `fit` returns
//! (`src/window/scene.rs`, `scene_geometry_changed`).
#[path = "../src/window/scene_fit.rs"]
mod scene_fit;

use scene_fit::{Fit, Frame, TOLERANCE, fit};

/// The owner's external-display window (TestFlight 0.1.1, 2026-11-26),
/// measured from the screenshot at 1.44 px per point: a scene about 1408 pt
/// wide, with the drawing kept at about 1097 pt and shifted right by about
/// 311 pt.
const SCENE: Frame = Frame::new(0.0, 0.0, 1408.0, 720.0);
const STALE_WINDOW: Frame = Frame::new(311.0, 0.0, 1097.0, 720.0);

#[test]
fn a_window_left_behind_by_a_wider_scene_fills_it_again() {
    let fit = fit(SCENE, STALE_WINDOW, Frame::new(0.0, 0.0, 1097.0, 720.0));
    assert_eq!(fit.window, Some(SCENE));
    // The root view then takes the new window's bounds too.
    assert_eq!(fit.view, Some(SCENE));
}

#[test]
fn a_window_kept_at_the_origin_with_its_old_width_grows() {
    let old = Frame::new(0.0, 0.0, 1097.0, 720.0);
    let fit = fit(SCENE, old, old);
    assert_eq!(fit.window, Some(SCENE));
    assert_eq!(fit.view, Some(SCENE));
}

#[test]
fn a_root_view_left_behind_in_a_resized_window_fills_it() {
    let fit = fit(SCENE, SCENE, Frame::new(311.0, 0.0, 1097.0, 720.0));
    assert_eq!(fit.window, None);
    assert_eq!(fit.view, Some(SCENE));
}

#[test]
fn a_window_that_fills_its_scene_changes_nothing() {
    // Full screen on the iPad and a Mac window: the frames already match.
    for scene in [
        Frame::new(0.0, 0.0, 1376.0, 1032.0),
        Frame::new(0.0, 0.0, 1942.0, 1260.0),
        SCENE,
    ] {
        assert!(fit(scene, scene, scene).is_empty(), "{scene:?}");
    }
}

#[test]
fn rounding_to_pixels_is_not_a_left_behind_frame() {
    let rounded = Frame::new(
        TOLERANCE,
        -TOLERANCE,
        SCENE.width + TOLERANCE,
        SCENE.height - TOLERANCE,
    );
    assert!(fit(SCENE, rounded, rounded).is_empty());
    let past = Frame::new(0.0, 0.0, SCENE.width - 2.0 * TOLERANCE, SCENE.height);
    assert_eq!(fit(SCENE, past, SCENE).window, Some(SCENE));
}

#[test]
fn a_scene_with_no_size_changes_nothing() {
    for scene in [
        Frame::new(0.0, 0.0, 0.0, 720.0),
        Frame::new(0.0, 0.0, 1408.0, 0.0),
        Frame::new(0.0, 0.0, f64::NAN, 720.0),
        Frame::new(f64::INFINITY, 0.0, 1408.0, 720.0),
    ] {
        assert_eq!(fit(scene, STALE_WINDOW, STALE_WINDOW), Fit::default());
    }
}

#[test]
fn a_scene_bounds_origin_still_puts_the_window_at_zero() {
    // A window's frame is in its scene's coordinates, so it starts at the
    // origin however the scene's bounds are reported.
    let scene = Frame::new(311.0, 40.0, 1408.0, 720.0);
    let fit = fit(scene, scene, Frame::new(0.0, 0.0, 1408.0, 720.0));
    assert_eq!(fit.window, Some(SCENE));
    assert_eq!(fit.view, None);
}
