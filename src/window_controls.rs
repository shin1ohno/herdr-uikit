//! Horizontal room for header controls, separate from the content's safe area.
#![allow(unsafe_code, reason = "borrowed UIKit raw window handle")]

use gpui::{Pixels, px};
use objc2::MainThreadMarker;
use objc2_ui_kit::{UIEdgeInsets, UIView, UIViewLayoutRegion, UIViewLayoutRegionAdaptivityAxis};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Extra room on the physical left of this window's first row. The caller
/// reserves it only in its header, never around the entire content viewport.
/// Returns `None` when no live UIKit handle is available on the main thread.
pub fn left_inset(window: &impl HasWindowHandle) -> Option<Pixels> {
    let _mtm = MainThreadMarker::new()?;
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: the borrowed UIKit handle guarantees a live UIView during this
    // call; no pointer or UIKit object escapes its borrow or the main thread.
    let view = unsafe { handle.ui_view.cast::<UIView>().as_ref() };
    Some(layout(view).1)
}

pub(crate) fn layout(view: &UIView) -> (UIEdgeInsets, Pixels) {
    let safe = view.safeAreaInsets();
    let corner = if objc2::available!(ios = 26.0) {
        let region = UIViewLayoutRegion::safeAreaLayoutRegionWithCornerAdaptation(
            UIViewLayoutRegionAdaptivityAxis::Horizontal,
        );
        Some(view.edgeInsetsForLayoutRegion(&region))
    } else {
        None
    };
    geometry(safe, corner)
}

pub(crate) fn geometry(safe: UIEdgeInsets, corner: Option<UIEdgeInsets>) -> (UIEdgeInsets, Pixels) {
    // GPUI already removes the ordinary safe area from the viewport. Only
    // the additional horizontal obstruction belongs in the header.
    let extra = corner.map_or(0., |corner| (corner.left - safe.left).max(0.));
    (safe, px(extra as f32))
}
