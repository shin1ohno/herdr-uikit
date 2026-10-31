//! Rectangles between GPUI and UIKit for the IME candidate window, the
//! dictation and caps lock indicators, and the software keyboard's caret
//! avoidance.
//!
//! GPUI's `Pixels` are logical units and its scale factor maps them to device
//! pixels, exactly as UIKit points relate to device pixels, so a window
//! coordinate in `Pixels` is a point in the `GPUIView`, which this view
//! reports as its `textInputView`. The scale factor snaps rectangles outward
//! to whole device pixels so UIKit's indicators line up with what GPUI drew.

use gpui::{Bounds, Pixels, Point, px};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};

/// UIKit's caret is two points wide; GPUI reports a zero-width range.
const CARET_WIDTH: f32 = 2.0;

/// `bounds` as a UIKit rectangle, snapped outward to the device-pixel grid.
pub(super) fn rect(bounds: Bounds<Pixels>, scale_factor: f32) -> CGRect {
    let left = bounds.origin.x.as_f32();
    let top = bounds.origin.y.as_f32();
    let right = left + bounds.size.width.as_f32().max(0.0);
    let bottom = top + bounds.size.height.as_f32().max(0.0);
    let (left, top) = (snap_down(left, scale_factor), snap_down(top, scale_factor));
    let (right, bottom) = (snap_up(right, scale_factor), snap_up(bottom, scale_factor));
    CGRect::new(
        CGPoint::new(CGFloat::from(left), CGFloat::from(top)),
        CGSize::new(CGFloat::from(right - left), CGFloat::from(bottom - top)),
    )
}

/// The caret at the start of `bounds`, as wide as UIKit draws one.
pub(super) fn caret(bounds: Bounds<Pixels>, scale_factor: f32) -> CGRect {
    let mut caret = bounds;
    caret.size.width = px(CARET_WIDTH);
    rect(caret, scale_factor)
}

/// A point UIKit reports in the `textInputView`, in GPUI window coordinates.
pub(super) fn point(point: CGPoint) -> Point<Pixels> {
    // GPUI geometry is f32; the narrowing is the precision GPUI works in.
    #[allow(clippy::cast_possible_truncation, reason = "GPUI coordinates are f32")]
    let (x, y) = (point.x as f32, point.y as f32);
    gpui::point(px(x), px(y))
}

/// The point at the middle of the line `lines` away from the caret in
/// `caret`, for moving up and down by lines.
pub(super) fn line_point(caret: Bounds<Pixels>, lines: isize) -> Option<Point<Pixels>> {
    let height = caret.size.height.as_f32();
    if height <= 0.0 {
        return None;
    }
    // Line counts from UIKit are small; precision loss past 2^24 lines is moot.
    #[allow(clippy::cast_precision_loss, reason = "line counts are small")]
    let lines = lines as f32;
    let y = caret.origin.y.as_f32() + height * (lines + 0.5);
    Some(gpui::point(caret.origin.x, px(y)))
}

fn snap_down(value: f32, scale_factor: f32) -> f32 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        (value * scale_factor).floor() / scale_factor
    } else {
        value
    }
}

fn snap_up(value: f32, scale_factor: f32) -> f32 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        (value * scale_factor).ceil() / scale_factor
    } else {
        value
    }
}
