//! Actual Objective-C selectors and GPUI adapter, with injected callback order.
//! This does not simulate UIKit's internal queued-input routing across a swap.
#![allow(
    unsafe_code,
    reason = "main-thread Mach-O initializer and typed UIKit selectors"
)]
#![allow(clippy::expect_used)]

use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Bounds, ElementInputHandler, PlatformInputHandler, TestAppContext};
use objc2::{DefinedClass, MainThreadMarker};
use objc2_foundation::{NSRange, NSString};
use objc2_ui_kit::{UIKeyInput, UITextInput, UIView};

use super::{super::positions::TextRange, composition_commit::Terminal};
use crate::window::{input_target::InputTarget, text_input::TextInputView};
use crate::{TextInputRetirementError, TextInputRetirementStatus};

static RESULT: AtomicU8 = AtomicU8::new(0);
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CHECK: extern "C" fn() = check;

extern "C" fn check() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let result = std::panic::catch_unwind(|| objc2::rc::autoreleasepool(|_| exercise(mtm)));
    RESULT.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
}

fn exercise(mtm: MainThreadMarker) {
    let mut app = TestAppContext::single();
    let (entity, cx) = app.add_window_view(|_, _| Terminal::default());
    let handler = cx.update(|window, cx| {
        PlatformInputHandler::new(
            window.to_async(cx),
            Box::new(ElementInputHandler::new(Bounds::default(), entity.clone())),
        )
    });
    let target = InputTarget::with_test_handler(handler);
    let input = TextInputView::new(mtm, target.clone());
    let parent = UIView::new(mtm);
    parent.addSubview(&input.view());
    let old = input.current();
    old.setMarkedText_selectedRange(Some(&NSString::from_str("same")), NSRange::new(4, 0));
    assert_eq!(entity.read_with(cx, |v, _| v.marked.clone()), "same");

    // A cancellation inside the originating callback cannot move its tail.
    let callback = target.client.enter().expect("live client");
    assert!(matches!(
        input.begin_retirement(),
        Err(TextInputRetirementError::NativeCallbackInFlight)
    ));
    assert!(target.client.live());
    drop(callback);
    old.schedule_flush();
    let (retained_old, next, receipt) = input.begin_retirement().expect("retirement starts");
    assert!(matches!(
        input.begin_retirement(),
        Err(TextInputRetirementError::InProgress)
    ));
    assert_eq!(receipt.status(), TextInputRetirementStatus::Pending);

    // Same old text is rejected both before and after new client activation.
    old.insertText(&NSString::from_str("same"));
    old.unmarkText();
    assert!(entity.read_with(cx, |v, _| v.commits.is_empty()));
    cx.update(|_, cx| entity.update(cx, |v, _| v.marked.clear()));
    input.finish_retirement(retained_old, next, &receipt);
    let new = input.current();
    assert_ne!(
        old.ivars().target.client.identity,
        new.ivars().target.client.identity
    );
    assert!(old.superview().is_none());
    assert!(!old.ivars().flush_scheduled.get());
    assert!(!old.canBecomeFirstResponder());
    assert_eq!(parent.subviews().len(), 1);

    // Direct English and identical fresh text go through UIKeyInput once.
    new.insertText(&NSString::from_str("a"));
    new.insertText(&NSString::from_str("same"));
    old.insertText(&NSString::from_str("same"));
    {
        old.setMarkedText_selectedRange(Some(&NSString::from_str("stale")), NSRange::new(5, 0));
        old.replaceRange_withText(
            &TextRange::text_range(mtm, 0..0),
            &NSString::from_str("stale"),
        );
    }
    old.unmarkText();
    old.deleteBackward();
    old.flush();
    assert_eq!(
        entity.read_with(cx, |v, _| v.commits.clone()),
        ["a", "same"]
    );
    assert!(entity.read_with(cx, |v, _| v.marked.is_empty()));

    // Fresh composition, a partial emoji edit, and unmark commit exactly once.
    new.setMarkedText_selectedRange(Some(&NSString::from_str("前😀")), NSRange::new(3, 0));
    new.deleteBackward();
    assert_eq!(entity.read_with(cx, |v, _| v.marked.clone()), "前");
    new.unmarkText();
    new.unmarkText();
    assert_eq!(
        entity.read_with(cx, |v, _| v.commits.clone()),
        ["a", "same", "前"]
    );

    // Detach/reconnect-style repeated retirement keeps every old object closed.
    let (second, next, receipt2) = input.begin_retirement().expect("second retirement");
    cx.update(|_, cx| entity.update(cx, |v, _| v.marked.clear()));
    input.finish_retirement(second, next, &receipt2);
    new.insertText(&NSString::from_str("late"));
    old.insertText(&NSString::from_str("late"));
    input.current().insertText(&NSString::from_str("fresh"));
    assert_eq!(
        entity.read_with(cx, |v, _| v.commits.clone()),
        ["a", "same", "前", "fresh"]
    );
    assert_eq!(parent.subviews().len(), 1);
    // No UIWindow/UIApplicationMain here: never call this responder proof.
    assert_eq!(
        receipt.status(),
        TextInputRetirementStatus::Replaced {
            first_responder: false,
            responder_requested: false,
        }
    );
}

#[test]
fn native_retirement_selectors_isolate_old_callbacks_and_deliver_new_text_once() {
    assert_eq!(
        RESULT.load(Ordering::Acquire),
        1,
        "native retirement adapter initializer failed"
    );
}
