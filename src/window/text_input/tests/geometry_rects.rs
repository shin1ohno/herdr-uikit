use super::*;

use gpui::{Bounds, point, px, size};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};

fn bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

fn cg(x: f64, y: f64, width: f64, height: f64) -> CGRect {
    CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
}

#[test]
fn pixels_are_points() {
    assert_eq!(
        geometry::rect(bounds(10.0, 20.0, 30.0, 16.0), 2.0),
        cg(10.0, 20.0, 30.0, 16.0)
    );
}

#[test]
fn rects_snap_outward_to_device_pixels() {
    // At 2x, 10.3 snaps down to 10.0 and 40.6 up to 41.0.
    assert_eq!(
        geometry::rect(bounds(10.3, 0.2, 30.3, 15.9), 2.0),
        cg(10.0, 0.0, 31.0, 16.5)
    );
}

#[test]
fn an_unusable_scale_factor_leaves_rects_unsnapped() {
    let rect = geometry::rect(bounds(1.25, 2.5, 3.0, 4.0), 0.0);
    assert_eq!(rect, cg(1.25, 2.5, 3.0, 4.0));
    let rect = geometry::rect(bounds(1.25, 2.5, 3.0, 4.0), f32::NAN);
    assert_eq!(rect, cg(1.25, 2.5, 3.0, 4.0));
}

#[test]
fn negative_sizes_collapse_to_empty() {
    assert_eq!(
        geometry::rect(bounds(5.0, 5.0, -3.0, -1.0), 1.0),
        cg(5.0, 5.0, 0.0, 0.0)
    );
}

#[test]
fn carets_are_two_points_wide() {
    assert_eq!(
        geometry::caret(bounds(12.0, 40.0, 0.0, 18.0), 3.0),
        cg(12.0, 40.0, 2.0, 18.0)
    );
}

#[test]
fn points_convert_to_window_pixels() {
    assert_eq!(
        geometry::point(CGPoint::new(12.5, 7.0)),
        point(px(12.5), px(7.0))
    );
}

#[test]
fn line_points_land_mid_line() {
    let caret = bounds(8.0, 100.0, 0.0, 20.0);
    assert_eq!(
        geometry::line_point(caret, 1),
        Some(point(px(8.0), px(130.0)))
    );
    assert_eq!(
        geometry::line_point(caret, -2),
        Some(point(px(8.0), px(70.0)))
    );
    assert_eq!(geometry::line_point(bounds(0.0, 0.0, 0.0, 0.0), 1), None);
}
