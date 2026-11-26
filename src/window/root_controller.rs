//! The window's root view controller: a `UIViewController` that hides the OS
//! status bar while the window fills its screen (`screen_fill`). UIKit asks
//! the root view controller of the window for `prefersStatusBarHidden`; the
//! layout pass sets the answer and asks UIKit to read it again.

#![allow(
    unsafe_code,
    reason = "defines a UIViewController subclass with objc2's define_class!"
)]

use std::cell::Cell;

use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_foundation::NSObject;
use objc2_ui_kit::{UIResponder, UIViewController};

pub(crate) struct RootIvars {
    hides_status_bar: Cell<bool>,
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
            self.ivars().hides_status_bar.get()
        }
    }
);

impl RootController {
    pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RootIvars {
            hides_status_bar: Cell::new(false),
        });
        // SAFETY: `init` is NSObject's initializer, which UIViewController
        // routes to `initWithNibName:bundle:` with no nib.
        unsafe { msg_send![super(this), init] }
    }

    /// Hides the status bar (`hidden`) or shows it again; returns whether
    /// that changed anything. UIKit reads the new answer on the next status
    /// bar update, which this asks for.
    pub(super) fn set_hides_status_bar(&self, hidden: bool) -> bool {
        if self.ivars().hides_status_bar.replace(hidden) == hidden {
            return false;
        }
        self.setNeedsStatusBarAppearanceUpdate();
        true
    }

    pub(super) fn hides_status_bar(&self) -> bool {
        self.ivars().hides_status_bar.get()
    }
}
