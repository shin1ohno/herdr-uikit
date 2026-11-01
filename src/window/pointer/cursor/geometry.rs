//! Pointer shapes and locations use UIKit points, matching GPUI logical
//! pixels. Screen scale changes rasterization, never these coordinates.

use gpui::CursorStyle;
use objc2_core_foundation::{CGPoint, CGRect};

pub(super) const BEAM_LENGTH: f64 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    // keep-sorted start
    Crosshair,
    HorizontalBeam,
    ResizeHorizontal,
    ResizeNeSw,
    ResizeNwSe,
    ResizeVertical,
    System,
    VerticalBeam,
    // keep-sorted end
}

pub(super) fn shape(style: CursorStyle) -> Shape {
    match style {
        CursorStyle::IBeam => Shape::VerticalBeam,
        CursorStyle::IBeamCursorForVerticalLayout => Shape::HorizontalBeam,
        CursorStyle::Crosshair => Shape::Crosshair,
        CursorStyle::ResizeLeft
        | CursorStyle::ResizeRight
        | CursorStyle::ResizeLeftRight
        | CursorStyle::ResizeColumn => Shape::ResizeHorizontal,
        CursorStyle::ResizeUp
        | CursorStyle::ResizeDown
        | CursorStyle::ResizeUpDown
        | CursorStyle::ResizeRow => Shape::ResizeVertical,
        CursorStyle::ResizeUpLeftDownRight => Shape::ResizeNwSe,
        CursorStyle::ResizeUpRightDownLeft => Shape::ResizeNeSw,
        CursorStyle::Arrow
        | CursorStyle::ClosedHand
        | CursorStyle::OpenHand
        | CursorStyle::PointingHand
        | CursorStyle::OperationNotAllowed
        | CursorStyle::DragLink
        | CursorStyle::DragCopy
        | CursorStyle::ContextualMenu => Shape::System,
    }
}

pub(super) fn contains(bounds: CGRect, location: CGPoint) -> bool {
    let values = [
        bounds.origin.x,
        bounds.origin.y,
        bounds.size.width,
        bounds.size.height,
        location.x,
        location.y,
    ];
    values.into_iter().all(f64::is_finite)
        && bounds.size.width > 0.0
        && bounds.size.height > 0.0
        && (bounds.origin.x + bounds.size.width).is_finite()
        && (bounds.origin.y + bounds.size.height).is_finite()
        && crate::window::convert::contains(bounds, location)
}

// Closed polygons centered at the physical pointer location. UIKit fills
// their paths; a resize indicator points in both directions along its axis.
const ARROW: &[(f64, f64)] = &[
    (-10.0, 0.0),
    (-5.0, -5.0),
    (-5.0, -2.0),
    (5.0, -2.0),
    (5.0, -5.0),
    (10.0, 0.0),
    (5.0, 5.0),
    (5.0, 2.0),
    (-5.0, 2.0),
    (-5.0, 5.0),
];
const CROSSHAIR: &[(f64, f64)] = &[
    (-1.0, -10.0),
    (1.0, -10.0),
    (1.0, -1.0),
    (10.0, -1.0),
    (10.0, 1.0),
    (1.0, 1.0),
    (1.0, 10.0),
    (-1.0, 10.0),
    (-1.0, 1.0),
    (-10.0, 1.0),
    (-10.0, -1.0),
    (-1.0, -1.0),
];

pub(super) fn path(shape: Shape) -> Option<impl Iterator<Item = CGPoint>> {
    use std::f64::consts::FRAC_1_SQRT_2;
    let (vertices, cos, sin): (&[(f64, f64)], f64, f64) = match shape {
        Shape::Crosshair => (CROSSHAIR, 1.0, 0.0),
        Shape::ResizeHorizontal => (ARROW, 1.0, 0.0),
        Shape::ResizeVertical => (ARROW, 0.0, 1.0),
        Shape::ResizeNwSe => (ARROW, FRAC_1_SQRT_2, FRAC_1_SQRT_2),
        Shape::ResizeNeSw => (ARROW, FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
        Shape::System | Shape::VerticalBeam | Shape::HorizontalBeam => return None,
    };
    Some(
        vertices
            .iter()
            .map(move |&(x, y)| CGPoint::new(x * cos - y * sin, x * sin + y * cos)),
    )
}
