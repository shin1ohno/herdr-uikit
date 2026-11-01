//! `GpuiView`, the `UIView` subclass each window draws into. It hosts the
//! renderer's `CAMetalLayer` as a sublayer, forwards layout, trait, and
//! safe-area changes, touches, pointer recognizers, and keyboard and scene
//! notifications to its `Window`, and stands in as first responder while no
//! text field is focused, passing hardware key presses to the text input view.
//! It also performs GPUI's menu commands for its window (`crate::menu`).

#![allow(
    unsafe_code,
    reason = "defines an Objective-C UIView subclass, calls its superclass's \
              implementations, and registers it with NSNotificationCenter"
)]

use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::AnyObject, sel,
};
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSSet};
use objc2_ui_kit::{
    UICommand, UIEvent, UIHoverGestureRecognizer, UIKeyboardDidHideNotification,
    UIKeyboardDidShowNotification, UIKeyboardWillChangeFrameNotification,
    UIKeyboardWillHideNotification, UIKeyboardWillShowNotification, UIPanGestureRecognizer,
    UIPress, UIPressesEvent, UIResponder, UISceneDidEnterBackgroundNotification,
    UISceneWillEnterForegroundNotification, UITouch, UITraitCollection, UIView, UIWindowScene,
};

use super::{keyboard::Event as KeyboardEvent, state::Window, touch::TouchStage};
use crate::platform::menus::{self, Performed};

pub(super) struct ViewIvars {
    /// Set while a press is being handed to the text input view, so a press
    /// it passes up the responder chain goes on rather than back to it.
    forwarding_presses: Cell<bool>,
    window: Weak<Window>,
}

define_class!(
    // SAFETY:
    // - `UIView` asks subclasses only to call its designated initializer,
    //   `initWithFrame:`, which `GpuiView::new` does.
    // - `GpuiView` does not implement `Drop`; its ivars drop with it.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrUIKitGPUIView"]
    #[ivars = ViewIvars]
    pub(super) struct GpuiView;

    impl GpuiView {
        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            // SAFETY: `layoutSubviews` takes no arguments and returns nothing.
            let () = unsafe { msg_send![super(self), layoutSubviews] };
            if let Some(window) = self.owner() {
                window.layout();
            }
        }

        #[unsafe(method(safeAreaInsetsDidChange))]
        fn safe_area_insets_did_change(&self) {
            // SAFETY: as for `layoutSubviews`.
            let () = unsafe { msg_send![super(self), safeAreaInsetsDidChange] };
            if let Some(window) = self.owner() {
                window.safe_area_changed();
            }
        }

        // Deprecated in iOS 17 in favour of trait change registration, which
        // objc2-ui-kit does not bind; UIKit still sends it to overriders.
        // Layout also re-reads traits, which iPadOS 18 tracks on its own.
        #[unsafe(method(traitCollectionDidChange:))]
        fn trait_collection_did_change(&self, previous: Option<&UITraitCollection>) {
            // SAFETY: the argument is the one UIKit passed in.
            let () = unsafe { msg_send![super(self), traitCollectionDidChange: previous] };
            if let Some(window) = self.owner() {
                window.traits_changed();
            }
        }

        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.touches(touches, event, TouchStage::Began);
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.touches(touches, event, TouchStage::Moved);
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.touches(touches, event, TouchStage::Ended);
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            self.touches(touches, event, TouchStage::Cancelled);
        }

        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            match self.press_target() {
                Some(target) => {
                    let _forwarding = Forwarding::new(&self.ivars().forwarding_presses);
                    // SAFETY: the target is a `UIResponder`; the arguments are
                    // the ones UIKit passed in.
                    let () = unsafe { msg_send![&*target, pressesBegan: presses, withEvent: event] };
                }
                // SAFETY: as above, for this view's superclass.
                None => {
                    let () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
                }
            }
        }

        #[unsafe(method(pressesChanged:withEvent:))]
        fn presses_changed(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            match self.press_target() {
                Some(target) => {
                    let _forwarding = Forwarding::new(&self.ivars().forwarding_presses);
                    // SAFETY: as in `pressesBegan:withEvent:`.
                    let () = unsafe { msg_send![&*target, pressesChanged: presses, withEvent: event] };
                }
                // SAFETY: as in `pressesBegan:withEvent:`.
                None => {
                    let () = unsafe { msg_send![super(self), pressesChanged: presses, withEvent: event] };
                }
            }
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            match self.press_target() {
                Some(target) => {
                    let _forwarding = Forwarding::new(&self.ivars().forwarding_presses);
                    // SAFETY: as in `pressesBegan:withEvent:`.
                    let () = unsafe { msg_send![&*target, pressesEnded: presses, withEvent: event] };
                }
                // SAFETY: as in `pressesBegan:withEvent:`.
                None => {
                    let () = unsafe { msg_send![super(self), pressesEnded: presses, withEvent: event] };
                }
            }
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            match self.press_target() {
                Some(target) => {
                    let _forwarding = Forwarding::new(&self.ivars().forwarding_presses);
                    // SAFETY: as in `pressesBegan:withEvent:`.
                    let () = unsafe { msg_send![&*target, pressesCancelled: presses, withEvent: event] };
                }
                // SAFETY: as in `pressesBegan:withEvent:`.
                None => {
                    let () = unsafe { msg_send![super(self), pressesCancelled: presses, withEvent: event] };
                }
            }
        }

        // The focused window's command, whatever its first responder: the
        // text input view does not perform it, so it reaches this view.
        #[unsafe(method(herdrMenuAction:))]
        fn menu_action(&self, sender: Option<&AnyObject>) {
            let Some(performed) = Performed::new(sender) else {
                return;
            };
            let handled = performed.shortcut().is_some_and(|shortcut| {
                self.owner()
                    .is_some_and(|window| window.text_input.menu_shortcut(shortcut))
            });
            if !handled {
                performed.dispatch_action();
            }
        }

        #[unsafe(method(validateCommand:))]
        fn validate_command(&self, command: &UICommand) {
            // SAFETY: the argument is the command UIKit passed; UIResponder's
            // implementation returns nothing.
            let () = unsafe { msg_send![super(self), validateCommand: command] };
            menus::validate(command);
        }

        #[unsafe(method(handleHover:))]
        fn handle_hover(&self, recognizer: &UIHoverGestureRecognizer) {
            if let Some(window) = self.owner() {
                window.handle_hover(recognizer);
            }
        }

        #[unsafe(method(handleScroll:))]
        fn handle_scroll(&self, recognizer: &UIPanGestureRecognizer) {
            if let Some(window) = self.owner() {
                window.handle_scroll(recognizer);
            }
        }

        #[unsafe(method(keyboardWillChangeFrame:))]
        fn keyboard_will_change_frame(&self, notification: &NSNotification) {
            if let Some(window) = self.owner() {
                window.keyboard_will_change_frame(notification);
            }
        }

        #[unsafe(method(keyboardWillShow:))]
        fn keyboard_will_show(&self, notification: &NSNotification) {
            if let Some(window) = self.owner() {
                window.keyboard_visibility_changed(notification, KeyboardEvent::WillShow);
            }
        }

        #[unsafe(method(keyboardDidShow:))]
        fn keyboard_did_show(&self, notification: &NSNotification) {
            if let Some(window) = self.owner() {
                window.keyboard_visibility_changed(notification, KeyboardEvent::DidShow);
            }
        }

        #[unsafe(method(keyboardWillHide:))]
        fn keyboard_will_hide(&self, notification: &NSNotification) {
            if let Some(window) = self.owner() {
                window.keyboard_visibility_changed(notification, KeyboardEvent::WillHide);
            }
        }

        #[unsafe(method(keyboardDidHide:))]
        fn keyboard_did_hide(&self, notification: &NSNotification) {
            if let Some(window) = self.owner() {
                window.keyboard_visibility_changed(notification, KeyboardEvent::DidHide);
            }
        }

        #[unsafe(method(sceneWillEnterForeground:))]
        fn scene_will_enter_foreground(&self, _notification: &NSNotification) {
            if let Some(window) = self.owner() {
                window.scene_visibility_changed(true);
            }
        }

        #[unsafe(method(sceneDidEnterBackground:))]
        fn scene_did_enter_background(&self, _notification: &NSNotification) {
            if let Some(window) = self.owner() {
                window.scene_visibility_changed(false);
            }
        }
    }
);

impl GpuiView {
    pub(super) fn new(
        mtm: MainThreadMarker,
        window: Weak<Window>,
        frame: CGRect,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ViewIvars {
            forwarding_presses: Cell::new(false),
            window,
        });
        // SAFETY: `initWithFrame:` is UIView's designated initializer.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.setMultipleTouchEnabled(true);
        this.setOpaque(true);
        this
    }

    /// Follows the software keyboard, which every scene's windows share.
    pub(super) fn observe_keyboard(&self) {
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: this view implements `keyboardWillChangeFrame:` taking the
        // notification, and is removed as an observer when its window closes;
        // the name is an immutable UIKit constant.
        unsafe {
            center.addObserver_selector_name_object(
                self.as_observer(),
                sel!(keyboardWillChangeFrame:),
                Some(UIKeyboardWillChangeFrameNotification),
                None,
            );
            for (selector, name) in [
                (sel!(keyboardWillShow:), UIKeyboardWillShowNotification),
                (sel!(keyboardDidShow:), UIKeyboardDidShowNotification),
                (sel!(keyboardWillHide:), UIKeyboardWillHideNotification),
                (sel!(keyboardDidHide:), UIKeyboardDidHideNotification),
            ] {
                center.addObserver_selector_name_object(
                    self.as_observer(),
                    selector,
                    Some(name),
                    None,
                );
            }
        }
    }

    /// Follows `scene` moving between the foreground and the background.
    pub(super) fn observe_scene(&self, scene: &UIWindowScene) {
        let center = NSNotificationCenter::defaultCenter();
        let object: &AnyObject = scene;
        // SAFETY: as in `observe_keyboard`, for `sceneWillEnterForeground:`
        // and `sceneDidEnterBackground:`; posting is limited to `scene`.
        unsafe {
            center.addObserver_selector_name_object(
                self.as_observer(),
                sel!(sceneWillEnterForeground:),
                Some(UISceneWillEnterForegroundNotification),
                Some(object),
            );
            center.addObserver_selector_name_object(
                self.as_observer(),
                sel!(sceneDidEnterBackground:),
                Some(UISceneDidEnterBackgroundNotification),
                Some(object),
            );
        }
    }

    pub(super) fn stop_observing(&self) {
        // SAFETY: removing an observer has no requirement beyond it being an
        // object, which this view is.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(self.as_observer()) };
    }

    fn as_observer(&self) -> &AnyObject {
        self
    }

    /// The window this view draws, while it is open.
    fn owner(&self) -> Option<Rc<Window>> {
        self.ivars().window.upgrade()
    }

    fn touches(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>, stage: TouchStage) {
        if let Some(window) = self.owner() {
            window.handle_touches(touches, event, stage);
        }
    }

    /// Where a hardware key press this view received should go: the text
    /// input view, which turns presses into GPUI key events, unless it is
    /// first responder itself. Then the press reached this view because the
    /// text input view passed it up the chain, and it continues up.
    fn press_target(&self) -> Option<Retained<UIView>> {
        if self.ivars().forwarding_presses.get() {
            return None;
        }
        let target = self.owner()?.text_input.view();
        (!target.isFirstResponder()).then_some(target)
    }
}

/// Marks a press as being forwarded until dropped.
struct Forwarding<'a>(&'a Cell<bool>);

impl<'a> Forwarding<'a> {
    fn new(flag: &'a Cell<bool>) -> Self {
        flag.set(true);
        Self(flag)
    }
}

impl Drop for Forwarding<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
