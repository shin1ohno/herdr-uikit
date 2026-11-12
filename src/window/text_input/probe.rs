//! Test-only retained native selector endpoint for the app probe.
#![allow(
    unsafe_code,
    reason = "typed standard UIKit selectors on main-thread views"
)]

use gpui::PlatformInput;
use objc2::{DefinedClass, MainThreadOnly, msg_send, rc::Retained, runtime::AnyObject, sel};
use objc2_foundation::{NSRange, NSString};
use objc2_ui_kit::{UICommand, UIKeyInput, UIMenuElementAttributes, UITextInput, UIView};

use super::{TextInputView, view_class::GPUITextInputView};

/// A captured native origin, not a lookup of the window's current client.
#[derive(Clone)]
pub struct NativeClient(Retained<GPUITextInputView>);

impl NativeClient {
    pub(crate) fn new(input: &TextInputView) -> Self {
        Self(input.current())
    }
    pub(crate) fn dispatch(&self, input: PlatformInput) -> bool {
        self.0.ivars().target.dispatch(input)
    }
    pub fn identity(&self) -> u64 {
        self.0.ivars().target.client.identity
    }
    pub fn insert(&self, text: &str) {
        self.0.insertText(&NSString::from_str(text));
    }
    pub fn mark(&self, text: &str) {
        self.0.setMarkedText_selectedRange(
            Some(&NSString::from_str(text)),
            NSRange::new(text.encode_utf16().count(), 0),
        );
    }
    pub fn unmark(&self) {
        self.0.unmarkText();
    }
    pub fn delete(&self) {
        self.0.deleteBackward();
    }
    pub fn first_responder(&self) -> bool {
        self.0.isFirstResponder()
    }
    /// Observed preconditions: this native object is first responder, its
    /// window is active with a key responder, and GPUI has text focus.
    /// Reading these values never requests focus or retries acquisition.
    pub fn text_readiness(&self) -> (bool, bool, bool) {
        let target = &self.0.ivars().target;
        (
            self.0.isFirstResponder(),
            target.edit_window_active(),
            target.text_input_focused(),
        )
    }

    pub fn attached(&self) -> bool {
        self.0.superview().is_some()
    }

    /// Actual Objective-C canPerformAction and validateCommand on the captured
    /// text view (or its drawing parent). Order: Copy, Cut, Paste, SelectAll.
    pub fn availability(&self, drawing: bool) -> Option<([bool; 4], [bool; 4])> {
        let text: Retained<UIView> = Retained::into_super(self.0.clone());
        let view = if drawing { text.superview()? } else { text };
        let selectors = [sel!(copy:), sel!(cut:), sel!(paste:), sel!(selectAll:)];
        let mut can = [false; 4];
        let mut validated = [false; 4];
        for (i, selector) in selectors.into_iter().enumerate() {
            // SAFETY: UIView implements these standard UIResponder selectors;
            // the command is main-thread-only and carries the matching action.
            unsafe {
                can[i] =
                    msg_send![&*view, canPerformAction: selector, withSender: None::<&AnyObject>];
                let command = UICommand::commandWithTitle_image_action_propertyList(
                    &NSString::from_str("probe"),
                    None,
                    selector,
                    None,
                    self.0.mtm(),
                );
                let () = msg_send![&*view, validateCommand: &*command];
                validated[i] = !command
                    .attributes()
                    .contains(UIMenuElementAttributes::Disabled);
            }
        }
        Some((can, validated))
    }

    /// Execute all four standard selectors. The probe handles paste's key-down
    /// so this does not touch the real pasteboard.
    pub fn edit_actions(&self) {
        // SAFETY: the receivers implement the standard sender-only selectors.
        unsafe {
            let () = msg_send![&*self.0, copy: None::<&AnyObject>];
            let () = msg_send![&*self.0, cut: None::<&AnyObject>];
            let () = msg_send![&*self.0, paste: None::<&AnyObject>];
            let () = msg_send![&*self.0, selectAll: None::<&AnyObject>];
        }
    }
}
