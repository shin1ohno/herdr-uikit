//! The cursor view the selection display interaction shows in place of the
//! system caret: an empty, transparent `UITextCursorView`. GPUI draws the
//! terminal's own cursor, so a second, blue system caret would be wrong.
//! UIKit puts the input mode indicator in its own accessory host view on the
//! drawing view, not in this view, so the indicator still shows (issue #139,
//! Design 1 a). `hidden` and `alpha` stay untouched, in case a later UIKit
//! puts accessories inside the cursor view.
#![allow(
    unsafe_code,
    reason = "an Objective-C UIView subclass implementing UITextCursorView"
)]

use std::cell::Cell;

use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_ui_kit::{UIColor, UICoordinateSpace, UIResponder, UITextCursorView, UIView};

define_class!(
    // SAFETY: UIView's subclassing rules are kept: instances are created
    // through `initWithFrame:` (`TransparentCursorView::new`), the class does
    // not implement `Drop`, and it overrides no UIView method, so nothing is
    // drawn (`drawRect:` is not overridden).
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrTransparentCursorView"]
    #[ivars = Cell<bool>]
    pub(in crate::window) struct TransparentCursorView;

    // SAFETY: NSObject's inherited implementation fulfills this protocol.
    unsafe impl NSObjectProtocol for TransparentCursorView {}

    // SAFETY: UIView's inherited implementation fulfills this protocol.
    unsafe impl UICoordinateSpace for TransparentCursorView {}

    // SAFETY: the three required methods keep the protocol's signatures and
    // run on the main thread (the class is MainThreadOnly).
    unsafe impl UITextCursorView for TransparentCursorView {
        #[unsafe(method(isBlinking))]
        fn __is_blinking(&self) -> bool {
            let _native_entry = crate::native_operation::refused();
            self.ivars().get()
        }

        #[unsafe(method(setBlinking:))]
        fn __set_blinking(&self, blinking: bool) {
            let _native_entry = crate::native_operation::refused();
            self.ivars().set(blinking);
        }

        // Nothing blinks, so there is no animation to restart.
        #[unsafe(method(resetBlinkAnimation))]
        fn __reset_blink_animation(&self) {
            let _native_entry = crate::native_operation::refused();
        }
    }
);

impl TransparentCursorView {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Cell::new(false));
        // SAFETY: `initWithFrame:` is UIView's designated initializer, and the
        // ivars were set before it runs.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: CGRect::ZERO] };
        view.setBackgroundColor(Some(&UIColor::clearColor()));
        view.setOpaque(false);
        view
    }

    /// This view as the `UIView` the interaction's `setCursorView:` takes.
    pub(super) fn as_view(&self) -> &UIView {
        self
    }
}
