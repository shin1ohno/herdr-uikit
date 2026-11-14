//! Deferred calls on the main run loop. The flush and the key repeat run as
//! `performSelector:withObject:afterDelay:` requests, which fire on the main
//! thread from the run loop: outside GPUI's updates and the window's state,
//! and without a `Send` closure, which a main-thread-only view cannot be in.

#![allow(
    unsafe_code,
    reason = "NSObject's delayed-perform methods have no safe binding without the NSDate feature"
)]

use objc2::runtime::{AnyObject, NSObject, Sel};
use objc2::{ClassType, DefinedClass, msg_send, sel};

use super::view_class::GPUITextInputView;

impl GPUITextInputView {
    /// Queues `herdrFlushInputState:` once per run loop turn.
    pub(super) fn schedule_flush(&self) {
        if !self.ivars().target.client.live() {
            return;
        }
        if self.ivars().flush_scheduled.replace(true) {
            return;
        }
        self.perform_after(sel!(herdrFlushInputState:), 0.0);
    }

    /// Calls `selector`, a method of this class taking one optional object,
    /// from the run loop after `delay` seconds. The run loop retains the view
    /// until then.
    pub(super) fn perform_after(&self, selector: Sel, delay: f64) {
        // SAFETY: `selector` names a method of this class that takes one
        // optional object and returns nothing, which is what
        // `performSelector:withObject:afterDelay:` calls.
        let () = unsafe {
            msg_send![
                self,
                performSelector: selector,
                withObject: None::<&AnyObject>,
                afterDelay: delay,
            ]
        };
    }

    /// Cancels queued calls of `selector`, or of every selector when `None`.
    pub(super) fn cancel_performs(&self, selector: Option<Sel>) {
        let target: &AnyObject = self;
        let class = NSObject::class();
        match selector {
            // SAFETY: this class method takes the target, the selector, and the
            // argument the requests were queued with (none), and returns
            // nothing.
            Some(selector) => unsafe {
                let () = msg_send![
                    class,
                    cancelPreviousPerformRequestsWithTarget: target,
                    selector: selector,
                    object: None::<&AnyObject>,
                ];
            },
            // SAFETY: this class method takes the target and returns nothing.
            None => unsafe {
                let () = msg_send![class, cancelPreviousPerformRequestsWithTarget: target];
            },
        }
    }
}
