#![allow(
    unsafe_code,
    reason = "a Mach-O test initializer exercises UIKit on the main thread before libtest starts workers"
)]

use std::{
    rc::Weak,
    sync::atomic::{AtomicU8, Ordering},
};

use objc2::MainThreadMarker;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_ui_kit::UIView;

use crate::window::{input_target::InputTarget, text_input::TextInputView, view::GpuiView};

static HIT_TARGET: AtomicU8 = AtomicU8::new(0);

// libtest blocks the main thread while its test workers run, so dispatching
// this UIKit check from a #[test] would deadlock. Mach-O initializers run on
// the main thread before the harness; retain only their scalar result.
// SAFETY: __mod_init_func contains C-ABI functions taking no arguments and
// returning nothing. This function has that signature and stores no UIKit
// object beyond its invocation.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CHECK_HIT_TARGET: extern "C" fn() = check_hit_target;

extern "C" fn check_hit_target() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let outcome = std::panic::catch_unwind(|| {
        objc2::rc::autoreleasepool(|_| {
            let frame = CGRect::new(CGPoint::ZERO, CGSize::new(400.0, 300.0));
            let view = GpuiView::new(mtm, Weak::new(), frame);
            let text_input = TextInputView::new(mtm, InputTarget::new(Weak::new()));
            let text_view = text_input.view();
            text_view.setFrame(view.bounds());
            view.addSubview(&text_view);

            let result = view.hitTest_withEvent(CGPoint::new(200.0, 150.0), None);
            let drawing_view: &UIView = &view;
            result
                .as_deref()
                .is_some_and(|hit| std::ptr::eq(hit, drawing_view))
        })
    });
    let status = match outcome {
        Ok(true) => 1,
        Ok(false) => 2,
        Err(_) => 3,
    };
    HIT_TARGET.store(status, Ordering::Release);
}

#[test]
fn full_bounds_text_client_leaves_touches_to_the_drawing_view() {
    assert_eq!(
        HIT_TARGET.load(Ordering::Acquire),
        1,
        "the interior hit test must return GPUIView, not its text-input subview"
    );
}
