use super::*;
use geometry::{Shape, contains, path, shape};
use objc2::{ClassType, ProtocolType};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_ui_kit::UIPointerInteractionDelegate;

#[test]
fn the_delegate_adopts_the_pointer_interaction_protocol() {
    let Some(protocol) = <dyn UIPointerInteractionDelegate>::protocol() else {
        panic!("UIKit has no UIPointerInteractionDelegate protocol");
    };
    assert!(PointerDelegate::class().conforms_to(protocol));
}

#[test]
fn text_cursors_use_the_matching_beam_axis() {
    assert_eq!(shape(CursorStyle::IBeam), Shape::VerticalBeam);
    assert_eq!(
        shape(CursorStyle::IBeamCursorForVerticalLayout),
        Shape::HorizontalBeam
    );
    assert!(path(Shape::VerticalBeam).is_none());
    assert!(path(Shape::HorizontalBeam).is_none());
}

#[test]
fn resize_cursors_preserve_their_axis() {
    for style in [
        CursorStyle::ResizeLeft,
        CursorStyle::ResizeRight,
        CursorStyle::ResizeLeftRight,
        CursorStyle::ResizeColumn,
    ] {
        assert_eq!(shape(style), Shape::ResizeHorizontal);
    }
    for style in [
        CursorStyle::ResizeUp,
        CursorStyle::ResizeDown,
        CursorStyle::ResizeUpDown,
        CursorStyle::ResizeRow,
    ] {
        assert_eq!(shape(style), Shape::ResizeVertical);
    }
    assert_eq!(shape(CursorStyle::ResizeUpLeftDownRight), Shape::ResizeNwSe);
    assert_eq!(shape(CursorStyle::ResizeUpRightDownLeft), Shape::ResizeNeSw);
}

#[test]
fn desktop_only_shapes_fall_back_to_the_system_pointer() {
    for style in [
        CursorStyle::Arrow,
        CursorStyle::ClosedHand,
        CursorStyle::OpenHand,
        CursorStyle::PointingHand,
        CursorStyle::OperationNotAllowed,
        CursorStyle::DragCopy,
        CursorStyle::DragLink,
        CursorStyle::ContextualMenu,
    ] {
        assert_eq!(shape(style), Shape::System);
    }
}

#[test]
fn locations_are_in_view_points_without_screen_scaling() {
    let bounds = CGRect {
        origin: CGPoint::new(100.0, 200.0),
        size: CGSize::new(500.0, 300.0),
    };
    assert!(contains(bounds, CGPoint::new(100.0, 200.0)));
    assert!(contains(bounds, CGPoint::new(600.0, 500.0)));
    assert!(!contains(bounds, CGPoint::new(99.0, 200.0)));
    assert!(!contains(bounds, CGPoint::new(600.5, 500.0)));
    assert!(!contains(bounds, CGPoint::new(1_000.0, 900.0)));
}

#[test]
fn unusable_regions_and_locations_are_refused() {
    let bounds = CGRect {
        origin: CGPoint::ZERO,
        size: CGSize::new(100.0, 200.0),
    };
    assert!(!contains(CGRect::ZERO, CGPoint::ZERO));
    assert!(!contains(bounds, CGPoint::new(f64::NAN, 0.0)));
    assert!(!contains(bounds, CGPoint::new(0.0, f64::INFINITY)));
    let overflowing = CGRect {
        origin: CGPoint::new(f64::MAX, 0.0),
        size: CGSize::new(f64::MAX, 10.0),
    };
    assert!(!contains(overflowing, CGPoint::new(f64::MAX, 0.0)));
}

#[test]
fn vector_paths_are_centered_bounded_and_keep_their_rotation() {
    for kind in [
        Shape::Crosshair,
        Shape::ResizeHorizontal,
        Shape::ResizeVertical,
        Shape::ResizeNwSe,
        Shape::ResizeNeSw,
    ] {
        let Some(points) = path(kind) else {
            panic!("missing path for {kind:?}");
        };
        let points: Vec<_> = points.collect();
        assert!(!points.is_empty());
        assert!(
            points
                .iter()
                .all(|point| point.x.abs() <= 10.0 && point.y.abs() <= 10.0)
        );
        let center_x = points.iter().map(|point| point.x).sum::<f64>();
        let center_y = points.iter().map(|point| point.y).sum::<f64>();
        assert!(center_x.abs() < 1e-9 && center_y.abs() < 1e-9);
    }
    let Some(mut horizontal) = path(Shape::ResizeHorizontal) else {
        panic!("missing resize path");
    };
    let Some(mut vertical) = path(Shape::ResizeVertical) else {
        panic!("missing resize path");
    };
    assert_eq!(horizontal.next(), Some(CGPoint::new(-10.0, 0.0)));
    assert_eq!(vertical.next(), Some(CGPoint::new(0.0, -10.0)));
}

#[test]
fn an_old_windows_exit_does_not_clear_a_new_hover_target() {
    let old = Rc::new(());
    let new = Rc::new(());
    let mut target = Rc::downgrade(&new);
    clear_if_owner(&mut target, &*old);
    assert!(
        target
            .upgrade()
            .is_some_and(|target| Rc::ptr_eq(&target, &new))
    );
    clear_if_owner(&mut target, &*new);
    assert!(target.upgrade().is_none());
}

#[test]
fn a_closed_windows_weak_target_cannot_retain_it() {
    let target = {
        let window = Rc::new(());
        Rc::downgrade(&window)
    };
    assert!(target.upgrade().is_none());
}

#[test]
fn entering_another_window_settles_only_the_previous_hover() {
    let old = Rc::new(());
    let new = Rc::new(());
    let mut target = Rc::downgrade(&old);
    assert!(replace_target(&mut target, &old).is_none());
    assert!(replace_target(&mut target, &new).is_some_and(|previous| Rc::ptr_eq(&previous, &old)));
    assert!(
        target
            .upgrade()
            .is_some_and(|current| Rc::ptr_eq(&current, &new))
    );
    assert!(replace_target(&mut target, &new).is_none());
}
