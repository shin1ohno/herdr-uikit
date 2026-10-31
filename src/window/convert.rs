//! Conversions between UIKit's geometry, key flags, and traits and GPUI's
//! types. UIKit measures in points with the origin at the top left, which is
//! what GPUI's logical `Pixels` are, so positions convert without scaling.

use gpui::{Bounds, Capslock, Edges, Modifiers, MouseButton, Pixels, Point, WindowAppearance, px};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_ui_kit::{UIEdgeInsets, UIEventButtonMask, UIKeyModifierFlags, UIUserInterfaceStyle};

pub(super) fn point(point: CGPoint) -> Point<Pixels> {
    gpui::point(pixels(point.x), pixels(point.y))
}

pub(super) fn edges(insets: UIEdgeInsets) -> Edges<Pixels> {
    Edges {
        top: pixels(insets.top),
        right: pixels(insets.right),
        bottom: pixels(insets.bottom),
        left: pixels(insets.left),
    }
}

pub(super) fn modifiers(flags: UIKeyModifierFlags) -> Modifiers {
    Modifiers {
        control: flags.contains(UIKeyModifierFlags::Control),
        alt: flags.contains(UIKeyModifierFlags::Alternate),
        shift: flags.contains(UIKeyModifierFlags::Shift),
        platform: flags.contains(UIKeyModifierFlags::Command),
        // UIKit reports the Globe/fn key only as a key press, never a flag.
        function: false,
    }
}

pub(super) fn capslock(flags: UIKeyModifierFlags) -> Capslock {
    Capslock {
        on: flags.contains(UIKeyModifierFlags::AlphaShift),
    }
}

/// The button an indirect pointer (trackpad or mouse) touch pressed. UIKit
/// names only the primary and secondary buttons; a third reports bit 2.
pub(super) fn mouse_button(mask: UIEventButtonMask) -> MouseButton {
    if mask.contains(UIEventButtonMask::Secondary) {
        MouseButton::Right
    } else if mask.0 & (1 << 2) != 0 {
        MouseButton::Middle
    } else {
        MouseButton::Left
    }
}

pub(super) fn appearance(style: UIUserInterfaceStyle) -> WindowAppearance {
    if style == UIUserInterfaceStyle::Dark {
        WindowAppearance::Dark
    } else {
        WindowAppearance::Light
    }
}

/// How much of `view` a keyboard at `keyboard` covers from the bottom, both
/// in the view's coordinates. A keyboard that does not reach the view's bottom
/// edge (floating, or split and undocked) or misses it horizontally covers
/// nothing that layout should avoid.
pub(super) fn keyboard_overlap(view: CGRect, keyboard: CGRect) -> CGFloat {
    let view_bottom = view.origin.y + view.size.height;
    let keyboard_bottom = keyboard.origin.y + keyboard.size.height;
    let overlaps_horizontally = keyboard.origin.x < view.origin.x + view.size.width
        && keyboard.origin.x + keyboard.size.width > view.origin.x;
    // UIKit's frames can land a fraction of a point short of the edge.
    let docked_at_bottom = keyboard_bottom >= view_bottom - 1.0;
    if keyboard.size.height <= 0.0 || !overlaps_horizontally || !docked_at_bottom {
        return 0.0;
    }
    (view_bottom - keyboard.origin.y).clamp(0.0, view.size.height)
}

/// Whether `point` is in `rect`, its edges included.
pub(super) fn contains(rect: CGRect, point: CGPoint) -> bool {
    point.x >= rect.origin.x
        && point.x <= rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y <= rect.origin.y + rect.size.height
}

pub(super) fn cg_rect(bounds: Bounds<Pixels>) -> CGRect {
    CGRect::new(
        CGPoint::new(points(bounds.origin.x), points(bounds.origin.y)),
        CGSize::new(points(bounds.size.width), points(bounds.size.height)),
    )
}

fn pixels(value: CGFloat) -> Pixels {
    px(value as f32)
}

fn points(value: Pixels) -> CGFloat {
    CGFloat::from(f32::from(value))
}
