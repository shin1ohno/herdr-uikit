//! How far the corner-adapted safe area extends past the plain safe area.

use gpui::{Edges, Pixels};
use objc2::sel;
use objc2_foundation::NSObjectProtocol;
use objc2_ui_kit::{UIEdgeInsets, UIView, UIViewLayoutRegion, UIViewLayoutRegionAdaptivityAxis};

#[cfg(test)]
mod tests;

/// Whether a sample of (safe area, window controls) changes what GPUI shows.
/// A change in the controls alone counts: they appear or go while the safe
/// area stays put.
pub(super) fn needs_update(
    stored: (&Edges<Pixels>, &Edges<Pixels>),
    sampled: (&Edges<Pixels>, &Edges<Pixels>),
) -> bool {
    stored != sampled
}

pub(super) fn sample(view: &UIView, safe_area: UIEdgeInsets) -> UIEdgeInsets {
    // The selector and class both arrived in iOS 26. Check the selector
    // before looking up the class, which would panic on an older system.
    if !view.respondsToSelector(sel!(edgeInsetsForLayoutRegion:)) {
        return UIEdgeInsets {
            top: 0.0,
            left: 0.0,
            bottom: 0.0,
            right: 0.0,
        };
    }
    let region = UIViewLayoutRegion::safeAreaLayoutRegionWithCornerAdaptation(
        UIViewLayoutRegionAdaptivityAxis::Horizontal,
    );
    beyond(view.edgeInsetsForLayoutRegion(&region), safe_area)
}

fn beyond(region: UIEdgeInsets, safe: UIEdgeInsets) -> UIEdgeInsets {
    UIEdgeInsets {
        top: (region.top - safe.top).max(0.0),
        left: (region.left - safe.left).max(0.0),
        bottom: (region.bottom - safe.bottom).max(0.0),
        right: (region.right - safe.right).max(0.0),
    }
}
