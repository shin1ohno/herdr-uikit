//! The system's selection display for one text input view, which shows the
//! input mode indicator ("A", "あ", the "A かな あ" switcher) at the terminal
//! cursor as a standard text field does (issue #139).
//!
//! One `UITextSelectionDisplayInteraction` per `GPUITextInputView`: its
//! `textInput` is fixed when it is created, so a view that replaces a
//! retired one gets its own. It sits on the drawing view, not on the text
//! view: the text view takes no touches (`pointInside:withEvent:` is
//! false), and the indicator is tapped to switch the input source. Its
//! cursor view is transparent, so GPUI's cursor stays the only one drawn.
//!
//! The interaction only follows first responder: on when the text view
//! acquires it, off when it resigns. Nothing here re-shows the indicator; a
//! caret move only asks UIKit to re-read the caret, which tells the input
//! delegate nothing. See `docs/input-mode-indicator.md`.
#![allow(
    unsafe_code,
    reason = "a typed Objective-C delegate and UIKit's selection display setters"
)]

mod cursor;
#[cfg(target_abi = "sim")]
pub(super) mod qa;
mod state;

#[cfg(target_abi = "sim")]
use std::cell::Cell;
use std::cell::RefCell;

use gpui::{Bounds, Pixels};
use objc2::{
    MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_ui_kit::{
    UIInteraction, UITextSelectionDisplayInteraction, UITextSelectionDisplayInteractionDelegate,
    UIView,
};

use self::cursor::TransparentCursorView;
use self::state::{DisplayState, Sink};
use super::view_class::GPUITextInputView;

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no ivars, no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrSelectionDisplayDelegate"]
    struct SelectionDisplayDelegate;

    // SAFETY: NSObject's inherited implementation fulfills this protocol.
    unsafe impl NSObjectProtocol for SelectionDisplayDelegate {}

    // SAFETY: every method of the protocol is optional and none is
    // implemented, so the interaction installs its views on its own view.
    unsafe impl UITextSelectionDisplayInteractionDelegate for SelectionDisplayDelegate {}
);

impl SelectionDisplayDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) struct SelectionDisplay {
    interaction: Retained<UITextSelectionDisplayInteraction>,
    // The initializer requires a delegate and UIKit keeps delegates weakly,
    // so the display keeps it alive.
    _delegate: Retained<SelectionDisplayDelegate>,
    _cursor: Retained<TransparentCursorView>,
    state: RefCell<DisplayState>,
    /// Simulator QA: how often UIKit was asked to re-read the selection.
    #[cfg(target_abi = "sim")]
    needs_update_calls: Cell<u64>,
}

impl SelectionDisplay {
    /// A switched-off display for `text_input`, not yet on any view.
    pub(super) fn new(mtm: MainThreadMarker, text_input: &GPUITextInputView) -> Self {
        let delegate = SelectionDisplayDelegate::new(mtm);
        let interaction = UITextSelectionDisplayInteraction::initWithTextInput_delegate(
            UITextSelectionDisplayInteraction::alloc(mtm),
            text_input.as_text_input(),
            ProtocolObject::from_ref(&*delegate),
        );
        let cursor = TransparentCursorView::new(mtm);
        // SAFETY: `TransparentCursorView` implements `UITextCursorView`.
        unsafe { interaction.setCursorView(cursor.as_view()) };
        interaction.setActivated(false);
        Self {
            interaction,
            _delegate: delegate,
            _cursor: cursor,
            state: RefCell::new(DisplayState::default()),
            #[cfg(target_abi = "sim")]
            needs_update_calls: Cell::new(0),
        }
    }

    /// Puts the interaction on `host`, the window's drawing view.
    pub(super) fn attach(&self, host: &UIView) {
        host.addInteraction(ProtocolObject::from_ref(&*self.interaction));
    }

    /// Switches the interaction off and takes it off its view. Idempotent.
    pub(super) fn detach(&self) {
        self.deactivate();
        if let Some(host) = self.interaction.view() {
            host.removeInteraction(ProtocolObject::from_ref(&*self.interaction));
        }
    }

    /// The text view acquired first responder.
    pub(super) fn activate(&self) {
        self.state.borrow_mut().acquired(self);
    }

    /// The text view resigned first responder.
    pub(super) fn deactivate(&self) {
        self.state.borrow_mut().resigned(self);
    }

    /// GPUI reported the caret; UIKit re-reads it only when it moved.
    pub(super) fn caret_moved(&self, bounds: Bounds<Pixels>) {
        let key = state::caret_key(
            bounds.origin.x.as_f32(),
            bounds.origin.y.as_f32(),
            bounds.size.width.as_f32(),
            bounds.size.height.as_f32(),
        );
        self.state.borrow_mut().caret(self, key);
    }
}

impl Sink for SelectionDisplay {
    fn set_activated(&self, on: bool) {
        self.interaction.setActivated(on);
    }

    fn set_needs_update(&self) {
        #[cfg(target_abi = "sim")]
        self.needs_update_calls
            .set(self.needs_update_calls.get().saturating_add(1));
        self.interaction.setNeedsSelectionUpdate();
    }
}
