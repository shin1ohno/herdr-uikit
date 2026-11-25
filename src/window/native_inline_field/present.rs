//! Presenting the inline field and moving it, on the main queue outside
//! GPUI's update. The order is the search panel's: the gate is already shut,
//! GPUI's views resign, the clear controller is presented without
//! animation, and only a field that actually became first responder sends
//! `Opened`; otherwise the session fails (`PresentationFailed`).

use std::rc::{Rc, Weak};

use gpui::{Bounds, Pixels};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};

use super::controller::{Binding, Controller};
use super::policy::Rect;
use super::{Session, SessionId};
use crate::native_inline_field::NativeInlineFieldRequest;
use crate::window::state::Window;

fn cg(bounds: Bounds<Pixels>) -> CGRect {
    CGRect::new(
        CGPoint::new(f64::from(bounds.origin.x), f64::from(bounds.origin.y)),
        CGSize::new(f64::from(bounds.size.width), f64::from(bounds.size.height)),
    )
}

fn rect(rect: CGRect) -> Rect {
    Rect {
        x: rect.origin.x as f32,
        y: rect.origin.y as f32,
        w: rect.size.width as f32,
        h: rect.size.height as f32,
    }
}

impl Window {
    pub(super) fn present_inline(
        &self,
        id: SessionId,
        request: NativeInlineFieldRequest,
        weak: Weak<Window>,
    ) {
        if !self.ready_to_present(id) {
            return;
        }
        let binding = || Binding {
            window: weak.clone(),
            session: id,
        };
        let controller = Rc::new(Controller::new(binding, &request, self.mtm));
        if let Some(session) = self
            .native_search
            .borrow_mut()
            .as_mut()
            .and_then(Session::inline_mut)
            .filter(|s| s.id == id)
        {
            session.controller = Some(controller.clone());
        }
        let done = weak.clone();
        let presented = controller.clone();
        let select_all = request.select_all;
        let completion = block2::RcBlock::new(move || {
            let Some(window) = done.upgrade() else {
                return;
            };
            window.place_inline(id);
            let focused = window.acquire_field(presented.field_view());
            if focused && select_all {
                presented.select_all();
            }
            let effects = window
                .state
                .borrow_mut()
                .native_modal
                .presented(id, focused);
            window.apply(id, effects);
            if !focused && window.state.borrow().is_active {
                // As the search field: an active scene's field may take
                // first responder a turn later; if it still refuses, the
                // session fails.
                let field = presented.clone();
                window.defer(move |w| {
                    if w.retry_field_focus(id, field.field_view()) && select_all {
                        field.select_all();
                    }
                });
            }
        });
        self.controller.presentViewController_animated_completion(
            &controller.controller,
            false,
            Some(&completion),
        );
    }

    /// Lays the field over the session's latest label rectangle, converted
    /// from the drawing view's points to the overlay's.
    pub(super) fn place_inline(&self, id: SessionId) {
        let placement = self
            .native_search
            .borrow()
            .as_ref()
            .and_then(Session::inline)
            .filter(|s| s.id == id)
            .and_then(|s| Some((s.controller.clone()?, s.frame, s.clip)));
        let Some((controller, frame, clip)) = placement else {
            return;
        };
        let overlay = &controller.overlay;
        let label = rect(overlay.convertRect_fromView(cg(frame), Some(&self.view)));
        let clip = clip.map(|clip| rect(overlay.convertRect_fromView(cg(clip), Some(&self.view))));
        controller.place(label, clip);
    }
}
