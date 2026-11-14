//! Opt-in replacement of the native object, after the current GPUI update.

use std::{cell::Cell, rc::Rc};

use objc2::{DefinedClass, MainThreadOnly, rc::Retained};
use objc2_ui_kit::UIViewAutoresizing;

use super::{TextInputView, state::Acquire, view_class::GPUITextInputView};
use crate::{
    RetiredTextInputSession, TextInputRetirement, TextInputRetirementError,
    TextInputRetirementStatus,
    window::{input_target::InputTarget, keyboard::ResignReason},
};

impl TextInputView {
    pub(in crate::window) fn begin_retirement(
        &self,
    ) -> Result<
        (
            Retained<GPUITextInputView>,
            InputTarget,
            TextInputRetirement,
        ),
        TextInputRetirementError,
    > {
        if self.replacing.get() {
            return Err(TextInputRetirementError::InProgress);
        }
        let old = self.current();
        let target = &old.ivars().target;
        if target.client.busy() {
            return Err(TextInputRetirementError::NativeCallbackInFlight);
        }
        let next = target
            .successor()
            .ok_or(TextInputRetirementError::IdentityExhausted)?;
        let receipt = TextInputRetirement {
            sessions: RetiredTextInputSession {
                retired: target.client.identity,
                replacement: next.client.identity,
            },
            status: Rc::new(Cell::new(TextInputRetirementStatus::Pending)),
        };
        self.replacing.set(true);
        // No UIKit call before this line: every callback on `old` retains
        // this same permanently closed token, even after a new handler exists.
        target.client.retire();
        Ok((old, next, receipt))
    }

    pub(in crate::window) fn finish_retirement(
        &self,
        old: Retained<GPUITextInputView>,
        next: InputTarget,
        receipt: &TextInputRetirement,
    ) {
        old.cancel_pending();
        let old_was_responder = old.isFirstResponder();
        let requested = old_was_responder || old.ivars().acquire.get().wants_first_responder();
        let view = GPUITextInputView::new(old.mtm(), next);
        view.ivars().mode.set(old.ivars().mode.get());
        view.ivars().traits.set(old.ivars().traits.get());
        view.ivars().caret.set(old.ivars().caret.get());
        // Do not copy delegate, tokenizer, marked snapshot, pending selectors,
        // press/repeat state or any UIKit session object to the replacement.
        view.setFrame(old.frame());
        view.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        *self.view.borrow_mut() = view.clone();
        if old_was_responder {
            // Old observer callbacks have a dead target. Record the planned
            // transfer on the live target before UIKit can post WillHide.
            view.ivars()
                .target
                .expect_keyboard_hide(ResignReason::Restart);
        }
        if let Some(parent) = old.superview() {
            parent.addSubview(&view);
        }
        // didMoveToWindow may schedule a normal flush. Consume it here once,
        // after copying the original acquisition intent, without a timer gap.
        view.cancel_pending();
        view.ivars().acquire.set(if requested {
            Acquire::for_mode(view.ivars().mode.get())
        } else {
            Acquire::Settled
        });
        view.flush();
        old.removeFromSuperview();
        let first_responder = view.isFirstResponder();
        receipt.status.set(TextInputRetirementStatus::Replaced {
            first_responder,
            responder_requested: requested,
        });
        self.replacing.set(false);
        tracing::debug!(
            retired_client = receipt.sessions.retired,
            replacement_client = receipt.sessions.replacement,
            first_responder,
            responder_requested = requested,
            "native text client replaced; OS delivery guarantee remains unverified"
        );
    }
}

impl TextInputView {
    pub(crate) fn session_state(&self) -> crate::TextInputSessionState {
        let view = self.current();
        crate::TextInputSessionState {
            client: view.ivars().target.client.identity,
            retired: !view.ivars().target.client.live(),
            replacement_pending: self.replacing.get(),
            first_responder: view.isFirstResponder(),
            attached: view.window().is_some(),
        }
    }
}
