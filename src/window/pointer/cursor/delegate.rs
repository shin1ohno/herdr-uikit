//! The Objective-C UIPointerInteractionDelegate and its object-return bridge.
#![allow(
    unsafe_code,
    reason = "defines a typed Objective-C delegate and calls NSObject's designated initializer"
)]

use super::{
    Window,
    geometry::{self, Shape},
};
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, rc::Retained,
};
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_ui_kit::{
    UIAxis, UIBezierPath, UIPointerInteraction, UIPointerInteractionDelegate, UIPointerRegion,
    UIPointerRegionRequest, UIPointerShape, UIPointerStyle,
};
use std::{
    ptr,
    rc::{Rc, Weak},
};

pub(super) struct DelegateIvars {
    owner: Weak<Window>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. The delegate has no
    // custom Drop; its weak window handle drops with the Objective-C object.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrUIKitPointerDelegate"]
    #[ivars = DelegateIvars]
    pub(super) struct PointerDelegate;

    // SAFETY: NSObject's inherited implementation fulfills this protocol.
    unsafe impl NSObjectProtocol for PointerDelegate {}

    // SAFETY: The implemented optional methods have the signatures declared
    // by UIPointerInteractionDelegate, and run only on the main thread.
    unsafe impl UIPointerInteractionDelegate for PointerDelegate {
        #[unsafe(method_id(pointerInteraction:regionForRequest:defaultRegion:))]
        fn region(
            &self,
            interaction: &UIPointerInteraction,
            request: &UIPointerRegionRequest,
            default_region: &UIPointerRegion,
        ) -> Option<Retained<UIPointerRegion>> {
            self.region_for(interaction, request, default_region)
        }

        #[unsafe(method_id(pointerInteraction:styleForRegion:))]
        fn style(
            &self,
            interaction: &UIPointerInteraction,
            _region: &UIPointerRegion,
        ) -> Option<Retained<UIPointerStyle>> {
            self.style_for(interaction)
        }
    }
);

impl PointerDelegate {
    // method_id rewrites the Objective-C ABI return type, so early returns
    // and `?` live in ordinary Rust helpers rather than the bridge body.
    fn region_for(
        &self,
        interaction: &UIPointerInteraction,
        request: &UIPointerRegionRequest,
        default_region: &UIPointerRegion,
    ) -> Option<Retained<UIPointerRegion>> {
        let window = self.owner(interaction)?;
        if !geometry::contains(window.view.bounds(), request.location()) {
            return None;
        }
        Some(default_region.retain())
    }

    fn style_for(&self, interaction: &UIPointerInteraction) -> Option<Retained<UIPointerStyle>> {
        let window = self.owner(interaction)?;
        let style = window.state.borrow().cursor_style;
        ui_style(geometry::shape(style), self.mtm())
    }

    pub(super) fn new(owner: Weak<Window>, mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars { owner });
        // SAFETY: NSObject's designated initializer takes no arguments; the
        // ivars were initialized before calling it.
        unsafe { msg_send![super(this), init] }
    }

    fn owner(&self, interaction: &UIPointerInteraction) -> Option<Rc<Window>> {
        let window = self.ivars().owner.upgrade()?;
        let state = window.state.borrow();
        if state.closed
            || !state.hovered
            || state.scene.is_none()
            || !state.visibility.is_visible()
            || !ptr::eq(&*window.pointer.interaction, interaction)
        {
            return None;
        }
        drop(state);
        Some(window)
    }
}

fn ui_style(shape: Shape, mtm: MainThreadMarker) -> Option<Retained<UIPointerStyle>> {
    let shape = match shape {
        // A nil style can select UIKit's automatic content effect. An
        // explicit system style leaves GPUI's whole drawing view unmodified.
        Shape::System => return Some(UIPointerStyle::systemPointerStyle(mtm)),
        Shape::VerticalBeam => UIPointerShape::beamWithPreferredLength_axis(
            geometry::BEAM_LENGTH,
            UIAxis::Vertical,
            mtm,
        ),
        Shape::HorizontalBeam => UIPointerShape::beamWithPreferredLength_axis(
            geometry::BEAM_LENGTH,
            UIAxis::Horizontal,
            mtm,
        ),
        path_shape => {
            let mut points = geometry::path(path_shape)?;
            let path = UIBezierPath::bezierPath();
            path.moveToPoint(points.next()?);
            for point in points {
                path.addLineToPoint(point);
            }
            path.closePath();
            UIPointerShape::shapeWithPath(&path, mtm)
        }
    };
    Some(UIPointerStyle::styleWithShape_constrainedAxes(
        &shape,
        UIAxis::Neither,
    ))
}
