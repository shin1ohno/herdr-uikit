//! Applying the window's requests to UIKit and telling UIKit about text GPUI
//! changed on its own.
//!
//! The window calls `TextInputView` while GPUI is drawing or dispatching,
//! when the app and the window's state are borrowed. Anything that reaches
//! UIKit from there can come straight back: `becomeFirstResponder`,
//! `reloadInputViews`, and `textDidChange:` all ask the text input for its
//! selection, which needs the input handler. So those requests only record
//! what is wanted and queue one flush on the run loop, where the handler is
//! free; edits UIKit starts itself refresh the cache synchronously instead,
//! so the flush does not echo them back as outside changes. GPUI hands the
//! window an input handler on every drawn frame, so the flush runs about
//! once per frame and takes first responder only when it was asked to.

use gpui::{Bounds, Pixels};
use objc2::{DefinedClass, sel};
use objc2_ui_kit::UITextInputDelegate;

use super::super::keyboard::{ResignContext, ResignReason};
use super::state::{Acquire, Change, Mode, TextState};
use super::traits::Traits;
use super::view_class::GPUITextInputView;

impl GPUITextInputView {
    pub(super) fn request_mode(&self, mode: Mode) {
        self.ivars().mode.set(Some(mode));
        self.request_first_responder();
    }

    /// Lets the next flush take first responder, even back from a responder
    /// UIKit gave it to: the window asked for a mode, or the view moved into
    /// a window.
    pub(super) fn request_first_responder(&self) {
        self.ivars()
            .acquire
            .set(Acquire::for_mode(self.ivars().mode.get()));
        self.schedule_flush();
    }

    /// UIKit took first responder away: the keyboard's dismiss key, another
    /// responder becoming first (an alert's or a picker's text field), or the
    /// view leaving its window. Flushes leave it there until the next request.
    pub(super) fn did_resign_first_responder(&self, context: ResignContext) {
        tracing::debug!("text input view resigned first responder");
        self.ivars().applied_mode.set(None);
        self.ivars().acquire.set(Acquire::Settled);
        self.ivars().target.text_input_did_resign(context);
    }

    pub(super) fn request_traits(&self, traits: Traits) {
        self.ivars().traits.set(traits);
        self.schedule_flush();
    }

    pub(super) fn set_caret(&self, bounds: Bounds<Pixels>) {
        self.ivars().caret.set(Some(bounds));
        // GPUI reports the caret when its selection moved, which UIKit must
        // hear about for the IME and the keyboard's suggestions.
        self.schedule_flush();
    }

    /// The handler's current state, or the default when GPUI has none.
    pub(super) fn read_snapshot(&self) -> TextState {
        self.with_handler(TextState::sample).unwrap_or_default()
    }

    /// Records the state after an edit UIKit made, which it already knows.
    pub(super) fn refresh_snapshot(&self) {
        let snapshot = self.read_snapshot();
        *self.ivars().snapshot.borrow_mut() = snapshot;
    }

    pub(super) fn refresh_edit_snapshot(
        &self,
        operation: &crate::native_operation::NativeOperation,
    ) {
        let snapshot = self
            .ivars()
            .target
            .with_edit_handler(
                operation,
                crate::native_operation::Phase::ObserveSnapshot,
                TextState::sample,
            )
            .unwrap_or_default();
        *self.ivars().snapshot.borrow_mut() = snapshot;
    }

    /// The queued flush: the mode, then the traits, then any text change.
    pub(super) fn flush(&self) {
        self.ivars().flush_scheduled.set(false);
        if !self.ivars().target.client.live() {
            return;
        }
        self.apply_mode();
        self.apply_traits();
        self.sync_text_state();
    }

    /// Takes first responder only for a request (see [`Acquire`]); a flush
    /// for a new handler or caret, which comes with every drawn frame, only
    /// reloads the input views of a view that already has it.
    fn apply_mode(&self) {
        let ivars = self.ivars();
        let Some(mode) = ivars.mode.get() else {
            return;
        };
        if mode == Mode::KeyOnly {
            return;
        }
        // `didMoveToWindow` requests again once there is a window to become
        // first responder in.
        if self.window().is_none() {
            return;
        }
        if !ivars.target.text_input_can_acquire() {
            return;
        }
        if self.isFirstResponder() {
            let acquire = ivars.acquire.get();
            let requested = acquire == Acquire::Requested;
            if acquire.needs_responder_restart(ivars.target.keyboard_needs_responder_restart()) {
                // reloadInputViews does not expand an iPad keyboard minimized
                // with its own hide key while this view remains responder.
                // Only an explicit request restarts that native acquisition.
                tracing::debug!("restarting text responder for a minimized keyboard");
                ivars.target.expect_keyboard_hide(ResignReason::Restart);
                self.resignFirstResponder();
                ivars.acquire.set(Acquire::Requested);
            } else {
                ivars.acquire.set(Acquire::Settled);
                if requested {
                    ivars.target.text_input_did_acquire();
                }
                let mode_changed = ivars.applied_mode.replace(Some(mode)) != Some(mode);
                if mode_changed {
                    self.reloadInputViews();
                }
                if mode_changed || requested {
                    ivars.target.reconcile_keyboard_layout();
                }
                if mode_changed {
                    self.accessibility_text_entry_acquired();
                }
                return;
            }
        }
        let acquire = ivars.acquire.get();
        if !acquire.wants_first_responder() {
            return;
        }
        // Becoming first responder reads `inputView` afresh.
        let became = self.becomeFirstResponder();
        if became {
            ivars.target.text_input_did_acquire();
            ivars.applied_mode.set(Some(mode));
            ivars.target.reconcile_keyboard_layout();
            self.accessibility_text_entry_acquired();
        } else if acquire.reports_failure() {
            tracing::debug!(
                ?mode,
                "text input view could not become first responder; retrying on later flushes"
            );
        }
        ivars.acquire.set(acquire.attempted(became));
    }

    fn apply_traits(&self) {
        let traits = self.ivars().traits.get();
        let changed = self.ivars().applied_traits.replace(traits) != traits;
        if changed && self.isFirstResponder() && self.ivars().mode.get() == Some(Mode::Text) {
            self.reloadInputViews();
            self.ivars().target.reconcile_keyboard_layout();
        }
    }

    fn sync_text_state(&self) {
        let snapshot = self.read_snapshot();
        let change = {
            let mut known = self.ivars().snapshot.borrow_mut();
            let change = snapshot.change_since(&known);
            *known = snapshot;
            change
        };
        self.notify(change);
    }

    /// Tells UIKit's input delegate about a change it did not make. The
    /// change has already happened, so "will" and "did" go out together; the
    /// delegate re-reads the text input on "did".
    pub(super) fn notify(&self, change: Change) {
        if change == Change::None {
            return;
        }
        let delegate = self.ivars().input_delegate.borrow().load();
        let Some(delegate) = delegate else {
            return;
        };
        let this = Some(self.as_text_input());
        match change {
            Change::Text => {
                delegate.textWillChange(this);
                delegate.textDidChange(this);
            }
            Change::Selection => {
                delegate.selectionWillChange(this);
                delegate.selectionDidChange(this);
            }
            Change::None => {}
        }
    }

    /// Stops pending flushes and key repeats, for a view whose window is
    /// going away.
    pub(super) fn cancel_pending(&self) {
        self.cancel_performs(None);
        self.ivars().flush_scheduled.set(false);
        self.ivars().keys.borrow_mut().stop_repeat();
    }

    /// Whether the key repeat timer should keep firing.
    pub(super) fn can_receive_keys(&self) -> bool {
        self.window().is_some() && self.ivars().target.has_key_responder()
    }

    pub(super) fn cancel_repeat_timer(&self) {
        self.cancel_performs(Some(sel!(herdrRepeatKey:)));
    }
}
