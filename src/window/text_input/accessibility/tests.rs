use super::*;

use objc2::ClassType;
use objc2::sel;
use objc2_core_foundation::{CGPoint, CGSize};

mod hit_testing;
mod selection;

#[test]
fn accessibility_caret_is_nonzero_before_and_after_layout() {
    assert_eq!(
        caret_frame(None, 2.0),
        CGRect::new(CGPoint::ZERO, CGSize::new(2.0, 1.0))
    );
    let caret = Bounds::new(point(px(12.0), px(35.0)), size(px(0.0), px(18.0)));
    assert_eq!(
        caret_frame(Some(caret), 2.0),
        CGRect::new(CGPoint::new(12.0, 35.0), CGSize::new(2.0, 18.0))
    );
}

#[test]
fn inaccessible_empty_and_invalid_carets_have_a_finite_frame() {
    let caret = Bounds::new(
        point(px(f32::NAN), px(f32::INFINITY)),
        size(px(0.0), px(-1.0)),
    );
    assert_eq!(
        caret_frame(Some(caret), 1.0),
        CGRect::new(CGPoint::ZERO, CGSize::new(2.0, 1.0))
    );
}

#[test]
fn text_input_view_exposes_accessibility_and_standard_edit_actions() {
    let class = GPUITextInputView::class();
    let methods = class.instance_methods();
    for selector in [
        sel!(isAccessibilityElement),
        sel!(accessibilityLabel),
        sel!(accessibilityValue),
        sel!(accessibilityFrame),
        sel!(accessibilityTextInputResponder),
        sel!(pointInside:withEvent:),
        sel!(canPerformAction:withSender:),
        sel!(paste:),
        sel!(copy:),
        sel!(cut:),
        sel!(selectAll:),
    ] {
        // Inherited UIView methods do not provide our text-client metadata
        // or touch passthrough; these must be this class's own overrides.
        assert!(
            methods.iter().any(|method| method.name() == selector),
            "{selector:?}"
        );
    }
}

#[test]
fn text_input_view_inherits_dynamic_responder_accessibility_traits() {
    let class = GPUITextInputView::class();
    let selector = sel!(accessibilityTraits);
    assert!(class.instance_method(selector).is_some());
    assert!(
        !class
            .instance_methods()
            .iter()
            .any(|method| method.name() == selector),
        "UIKit must derive editing traits from this live text responder"
    );
}
