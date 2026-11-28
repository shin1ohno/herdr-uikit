//! Keyboard input through a dedicated, invisible `UIView` subclass,
//! `GPUITextInputView`, that adopts `UIKeyInput` and `UITextInput`. It is
//! UIKit's text input client for one window, so the software keyboard,
//! hardware keyboards, dictation, and IME composition (marked text, such as
//! Japanese kana-kanji conversion) all reach the window's
//! `PlatformInputHandler`. Hardware key presses (`pressesBegan:withEvent:`
//! and friends) arrive here too and become `KeyDown`, `KeyUp`, and
//! `ModifiersChanged`, so shortcuts work with and without a text field.
//!
//! In text mode the view is first responder and its `inputView` is nil, so
//! UIKit shows the software keyboard when no hardware keyboard is attached.
//! In key-only mode the drawing view owns first responder and forwards
//! hardware presses here; resigning this text client also removes UIKit's
//! input-assistant bar. The text view takes first responder only when the
//! window asks for text mode or it moves into a window with text mode still
//! requested. When UIKit gives first responder to someone else (the keyboard's
//! dismiss key, an alert's or a picker's text field), it leaves it there until
//! the window asks again.
//!
//! The window owns one `TextInputView` as a subview of its `GPUIView` and
//! calls the methods below, often while GPUI is drawing. They only record
//! what the window wants and queue one flush on the run loop, because UIKit
//! answers first-responder and delegate calls by asking the text input for
//! its state, which needs the input handler GPUI is holding at that moment.

mod accessibility;
pub(super) mod actions;
mod control_keys;
mod editing;
mod geometry;
mod keymap;
mod keys;
mod positions;
#[cfg(feature = "test-support")]
pub(super) mod probe;
mod query;
mod retirement;
mod run_loop;
mod selection_display;
mod state;
mod sync;
mod traits;
mod utf16;
mod view_class;

#[cfg(test)]
mod tests;

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui::{Bounds, Pixels, TextInputConfiguration};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, rc::Retained, runtime::Sel};
use objc2_ui_kit::UIView;

pub(crate) use self::keymap::{named_key_char, us_shifted};
use super::keyboard_accessory::HerdrKeyboardRow;
use super::{input_target::InputTarget, keyboard::ResignReason};
use crate::menu::Shortcut;
#[cfg(target_abi = "sim")]
pub use selection_display::qa::{InputModeIndicatorState, input_mode_indicator_state};
use state::Mode;
use traits::Traits;
use view_class::GPUITextInputView;
pub(super) use view_class::GPUITextInputView as NativeEndpoint;

/// The window's handle to its text input view.
pub(crate) struct TextInputView {
    view: RefCell<Retained<GPUITextInputView>>,
    pub(super) replacing: Cell<bool>,
    /// The consumer's shortcuts bar, put on every view this one replaces too;
    /// `None` until the consumer sets one, so a first empty bar is applied
    /// and removes the system's items as well.
    keyboard_bar: RefCell<Option<crate::keyboard_bar::KeyboardBar>>,
    /// The consumer's accessory row, and the view that shows it (one for the
    /// window's life, made with the first non-empty row); both go on every
    /// view this one replaces too.
    keyboard_accessory: RefCell<crate::keyboard_bar::KeyboardAccessory>,
    accessory_row: RefCell<Option<Retained<HerdrKeyboardRow>>>,
    keyboard_bar_presses: Rc<RefCell<crate::keyboard_bar::PressQueue>>,
}

impl TextInputView {
    pub(super) fn captured_native(&self) -> Retained<NativeEndpoint> {
        self.current()
    }
    pub(super) fn is_captured_current(&self, old: &NativeEndpoint) -> bool {
        std::ptr::eq(&*self.current(), old)
    }
    pub(super) fn replace_reserved_native(
        &self,
        old: Retained<NativeEndpoint>,
    ) -> crate::NativeRetirementRequestStatus {
        use crate::{NativeRetirementRequestStatus as Status, TextInputRetirementStatus as Legacy};
        let Some(next) = old.ivars().target.successor() else {
            return Status::InvalidatedAfterFence;
        };
        let successor = next.origin();
        let receipt = crate::TextInputRetirement {
            sessions: crate::RetiredTextInputSession {
                retired: old.ivars().target.client.identity,
                replacement: successor.client,
            },
            status: Rc::new(Cell::new(Legacy::Pending)),
        };
        self.finish_retirement(old, next, &receipt);
        // The new request's Window reservation remains held through terminal
        // completion, even though legacy finish releases its own replacing flag.
        match receipt.status() {
            Legacy::Replaced {
                first_responder,
                responder_requested,
            } => Status::Replaced {
                successor,
                first_responder,
                responder_requested,
            },
            _ => Status::InvalidatedAfterFence,
        }
    }

    fn current(&self) -> Retained<GPUITextInputView> {
        self.view.borrow().clone()
    }

    pub(crate) fn new(mtm: MainThreadMarker, target: InputTarget) -> Self {
        Self {
            view: RefCell::new(GPUITextInputView::new(mtm, target)),
            replacing: Cell::new(false),
            keyboard_bar: RefCell::new(None),
            keyboard_accessory: RefCell::default(),
            accessory_row: RefCell::new(None),
            keyboard_bar_presses: Rc::default(),
        }
    }

    /// Shows `bar` on the shortcuts bar unless it is already there. A bar
    /// that only draws other keys selected updates the buttons in place.
    pub(crate) fn set_keyboard_bar(&self, bar: &crate::keyboard_bar::KeyboardBar) {
        let in_place = {
            let applied = self.keyboard_bar.borrow();
            match applied.as_ref() {
                Some(applied) if *applied == *bar => return,
                Some(applied) => super::keyboard_bar::same_shape(applied, bar),
                None => false,
            }
        };
        *self.keyboard_bar.borrow_mut() = Some(bar.clone());
        let view = self.current();
        if in_place {
            super::keyboard_bar::update_selected(&view, bar);
        } else {
            self.apply_keyboard_bar(&view);
        }
    }

    /// Shows `row` in the accessory row unless it is already there. The
    /// first non-empty row installs the row view, which reloads the input
    /// views on the next flush that finds no text being composed; after
    /// that the view stays installed, and an empty row only collapses it to
    /// zero height (`HerdrKeyboardRow::set_collapsed`), with no reload.
    pub(crate) fn set_keyboard_accessory(&self, row: &crate::keyboard_bar::KeyboardAccessory) {
        if *self.keyboard_accessory.borrow() == *row {
            return;
        }
        *self.keyboard_accessory.borrow_mut() = row.clone();
        let view = self.current();
        if !row.is_empty() {
            let row_view = self
                .accessory_row
                .borrow_mut()
                .get_or_insert_with(|| HerdrKeyboardRow::new(view.mtm()))
                .clone();
            row_view.update(row, &self.keyboard_bar_presses);
            row_view.set_collapsed(false);
        } else if let Some(row_view) = self.accessory_row.borrow().as_ref() {
            row_view.update(row, &self.keyboard_bar_presses);
            row_view.set_collapsed(true);
        }
        if self.apply_keyboard_accessory(&view) {
            view.accessory_changed();
        }
    }

    /// Puts the row view, once made, on `view`; whether that installed it.
    fn apply_keyboard_accessory(&self, view: &GPUITextInputView) -> bool {
        let row = self
            .accessory_row
            .borrow()
            .as_ref()
            .map(|row| Retained::into_super(Retained::into_super(row.clone())));
        let mut slot = view.ivars().accessory.borrow_mut();
        let changed = slot.is_some() != row.is_some();
        *slot = row;
        changed
    }

    /// The accessory row's height in points, 0 without a row or collapsed.
    pub(super) fn accessory_height(&self) -> f64 {
        if self.keyboard_accessory.borrow().is_empty() {
            0.0
        } else {
            super::keyboard_accessory::ROW_HEIGHT
        }
    }

    /// The accessory row's frame in screen points while UIKit shows it.
    pub(super) fn accessory_screen_frame(&self) -> Option<(f64, f64, f64, f64)> {
        if self.keyboard_accessory.borrow().is_empty() {
            return None;
        }
        let row = self.accessory_row.borrow();
        super::keyboard_accessory::screen_frame(row.as_ref()?)
    }

    /// Simulator QA: the primary language of the current view's input mode.
    #[cfg(target_abi = "sim")]
    pub(super) fn input_language(&self) -> Option<String> {
        self.current().input_language()
    }

    /// Simulator QA: the accessory row's keys and frame, `collapsed`, or
    /// `none` before the first row.
    #[cfg(target_abi = "sim")]
    pub(super) fn accessory_sample(&self) -> String {
        match self.accessory_row.borrow().as_ref() {
            Some(row) if row.is_collapsed() => {
                format!("collapsed {}", super::keyboard_accessory::qa_sample(row))
            }
            Some(row) => super::keyboard_accessory::qa_sample(row),
            None => "none".into(),
        }
    }

    /// Puts the consumer's bar on `view`; a view gets the system's items
    /// until the consumer sets a bar.
    fn apply_keyboard_bar(&self, view: &GPUITextInputView) {
        let Some(bar) = self.keyboard_bar.borrow().clone() else {
            return;
        };
        super::keyboard_bar::apply(view, &bar, &self.keyboard_bar_presses, view.mtm());
    }

    pub(crate) fn keyboard_bar_presses(&self) -> Rc<RefCell<crate::keyboard_bar::PressQueue>> {
        Rc::clone(&self.keyboard_bar_presses)
    }

    /// The window closed: the press stream ends.
    pub(crate) fn close_keyboard_bar(&self) {
        if let Ok(mut presses) = self.keyboard_bar_presses.try_borrow_mut() {
            presses.close();
        }
    }

    /// The view to add as a subview of the window's `GPUIView`. It is
    /// at the drawing view's origin with matching bounds. Its hit-test
    /// override takes no touches, and its coordinates are the `GPUIView`'s,
    /// which are GPUI's window coordinates.
    pub(crate) fn view(&self) -> Retained<UIView> {
        Retained::into_super(self.current())
    }

    /// Puts the current view's selection display, which shows the system's
    /// input mode indicator at the caret, on `host`, the window's drawing
    /// view. The window calls it once, after adding `view()` to `host`.
    pub(crate) fn attach_selection_display(&self, host: &UIView) {
        self.current().attach_selection_display(host);
    }

    /// Simulator QA: the current view's selection display on `drawing_view`.
    #[cfg(target_abi = "sim")]
    pub(crate) fn input_mode_indicator_state(
        &self,
        drawing_view: &UIView,
    ) -> InputModeIndicatorState {
        self.current().input_mode_indicator_state(drawing_view)
    }

    /// Simulator QA: the view's requested and applied mode and its pending
    /// acquisition.
    #[cfg(all(target_abi = "sim", feature = "test-support"))]
    pub(crate) fn qa_acquisition(&self) -> String {
        let view = self.current();
        let ivars = view.ivars();
        format!(
            "mode={:?} applied={:?} acquire={:?} live={}",
            ivars.mode.get(),
            ivars.applied_mode.get(),
            ivars.acquire.get(),
            ivars.target.client.live()
        )
    }

    pub(super) fn editing_enabled(&self, selector: Sel) -> bool {
        self.current().editing_enabled(selector)
    }

    pub(super) fn perform_edit_action(
        &self,
        selector: Sel,
        sender: Option<&objc2::runtime::AnyObject>,
        operation: &crate::native_operation::NativeOperation,
    ) {
        let captured = self.current();
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        let _origin = crate::native_edit_observer::DrawingScope::enter();
        captured.perform_edit_action(selector, sender, operation);
    }

    /// Runs `f` with the current view's observation registration. A retired
    /// view takes its registration with it, so the successor starts unregistered.
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    pub(crate) fn with_observation_registration<R>(
        &self,
        f: impl FnOnce(&crate::native_edit_observer::Registration) -> R,
    ) -> R {
        let current = self.current();
        f(&current.ivars().observation)
    }

    pub(super) fn validate_edit_command(&self, command: &objc2_ui_kit::UICommand) {
        self.current().validate_edit_command(command);
    }

    /// Text mode: the view becomes first responder with no `inputView`, so
    /// the software keyboard shows when no hardware keyboard is attached. The
    /// window calls this when the focused element starts taking text and the
    /// window is active, or when GPUI asks for the software keyboard. It
    /// takes first responder back from wherever UIKit gave it, so the window
    /// also calls it when the user taps a text element after dismissing the
    /// keyboard.
    pub(crate) fn activate(&self) {
        self.current().request_mode(Mode::Text);
    }

    /// Key-only mode: the window gives first responder to the drawing view,
    /// which forwards hardware presses for shortcuts. This text client does
    /// not acquire first responder, so UIKit removes its keyboard and input
    /// assistant. The window calls this when text focus ends or GPUI dismisses
    /// the software keyboard.
    pub(crate) fn deactivate(&self) {
        self.current().request_mode(Mode::KeyOnly);
    }

    /// Marks an intentional native resignation before UIKit can synchronously
    /// deliver its hide notifications or responder callback.
    pub(super) fn resign(
        &self,
        reason: ResignReason,
        switch: &super::edit_menu::responder_switch::ResponderSwitch<
            impl super::edit_menu::responder_switch::SwitchOwner,
        >,
    ) {
        let current = self.current();
        if current.isFirstResponder() {
            current.ivars().target.expect_keyboard_hide(reason);
            switch.resign(&**current);
        }
    }

    /// Before the native modal gate shuts: GPUI gets the release of each key
    /// it saw pressed and the repeat stops. Returns every held key's usage,
    /// to fence until UIKit reports its release.
    pub(super) fn release_held_for_modal(&self) -> Vec<isize> {
        self.current().release_held_for_modal()
    }

    /// A menu command with `shortcut` was performed; see
    /// `GPUITextInputView::menu_shortcut`. Returns whether the key press
    /// was GPUI's to handle, so the command's action must not run.
    pub(crate) fn menu_shortcut(&self, shortcut: &Shortcut) -> bool {
        self.current().menu_shortcut(shortcut)
    }

    /// GPUI replaced or changed the input handler. The flush re-reads the
    /// selection, marked text, and length, and tells UIKit's input delegate
    /// (`textWillChange:`/`textDidChange:`, or the selection pair) only when
    /// they differ from what UIKit last saw, so a composition in progress is
    /// not reset by redundant notifications. GPUI sets a handler on every
    /// drawn frame, so this never takes first responder; only `activate`
    /// and `deactivate` do.
    pub(crate) fn input_handler_changed(&self) {
        self.current().schedule_flush();
    }

    /// Where the caret is, in window coordinates. Kept for
    /// `caretRectForPosition:` and `firstRectForRange:` when the handler
    /// cannot place a range, so the IME candidate window and the software
    /// keyboard's avoidance follow the caret; the selection it implies is
    /// re-read and reported to UIKit.
    pub(crate) fn caret_bounds_changed(&self, bounds: Bounds<Pixels>) {
        self.current().set_caret(bounds);
    }

    /// The focused element's text assistance: autocorrection, capitalization,
    /// suggestions, and the return key's label. Smart punctuation stays off.
    /// The window forwards `PlatformWindow::set_text_input_configuration`.
    pub(crate) fn set_text_input_configuration(&self, configuration: &TextInputConfiguration) {
        self.current().request_traits(Traits::from(configuration));
    }
}

impl NativeEndpoint {
    pub(in crate::window) fn native_target(&self) -> &InputTarget {
        &self.ivars().target
    }
    pub(in crate::window) fn fence_reserved(&self) {
        self.ivars().target.client.retire();
    }
    pub(in crate::window) fn cancel_reserved_work(&self) {
        self.cancel_pending();
    }
}

impl Drop for TextInputView {
    /// Queued flushes and key repeats retain the view; cancel them so a
    /// closed window's view is not kept alive or driven afterwards. The
    /// selection display is switched off and taken off the drawing view.
    fn drop(&mut self) {
        let current = self.current();
        current.cancel_pending();
        current.detach_selection_display();
    }
}
