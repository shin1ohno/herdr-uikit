//! The window's root view controller: a `UIViewController` that remembers
//! whether the window fills its screen and answers `prefersStatusBarHidden`
//! from it (`screen_fill::hides_status_bar`, which keeps the status bar since
//! yebisu#242). UIKit asks the root view controller of the window for
//! `prefersStatusBarHidden`; the layout pass sets the fill and asks UIKit to
//! read the answer again when it changed.

#![allow(
    unsafe_code,
    reason = "defines a UIViewController subclass with objc2's define_class!"
)]

use std::cell::Cell;

use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_foundation::NSObject;
use objc2_ui_kit::{UIResponder, UIViewController};

pub(crate) struct RootIvars {
    fills_screen: Cell<bool>,
}

define_class!(
    // SAFETY: UIViewController may be subclassed. RootController overrides
    // only `prefersStatusBarHidden`, a getter UIKit calls on the main thread,
    // and does not implement Drop.
    #[unsafe(super(UIViewController, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrRootViewController"]
    #[ivars = RootIvars]
    pub(crate) struct RootController;

    impl RootController {
        // SAFETY: the selector is UIViewController's BOOL getter.
        #[unsafe(method(prefersStatusBarHidden))]
        fn prefers_status_bar_hidden(&self) -> bool {
            super::screen_fill::hides_status_bar(self.ivars().fills_screen.get())
        }
    }
);

impl RootController {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RootIvars {
            fills_screen: Cell::new(false),
        });
        // SAFETY: `init` is NSObject's initializer, which UIViewController
        // routes to `initWithNibName:bundle:` with no nib.
        unsafe { msg_send![super(this), init] }
    }

    /// Records whether the window fills its screen; returns whether that
    /// changed. When the status bar answer changes with it, this asks UIKit
    /// for a status bar update, on which UIKit reads the new answer.
    pub(super) fn set_fills_screen(&self, fills: bool) -> bool {
        let was = self.ivars().fills_screen.replace(fills);
        if was == fills {
            return false;
        }
        if super::screen_fill::hides_status_bar(was) != super::screen_fill::hides_status_bar(fills)
        {
            self.setNeedsStatusBarAppearanceUpdate();
        }
        true
    }

    pub(super) fn fills_screen(&self) -> bool {
        self.ivars().fills_screen.get()
    }
}
