//! UIKit's standard editing commands use the app's key bindings, so terminal
//! paste retains the app's clipboard handling and bracketed-paste policy.

#![allow(
    unsafe_code,
    reason = "UIPasteboard's non-atomic properties are used only on the main thread"
)]

use gpui::{
    Capslock, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ModifiersChangedEvent, PlatformInput,
};
use objc2::{DefinedClass, runtime::Sel, sel};
use objc2_ui_kit::{UICommand, UIMenuElementAttributes, UIPasteboard};

use super::state::Mode;
use super::view_class::GPUITextInputView;

pub(super) use crate::NativeEditAction as EditAction;
use crate::native_edit_action::NativeEditActionGuard;
#[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
use crate::native_edit_observer as observer;

impl EditAction {
    pub(super) fn from_selector(selector: Sel) -> Option<Self> {
        if selector == sel!(copy:) {
            Some(Self::Copy)
        } else if selector == sel!(cut:) {
            Some(Self::Cut)
        } else if selector == sel!(paste:) {
            Some(Self::Paste)
        } else if selector == sel!(selectAll:) {
            Some(Self::SelectAll)
        } else {
            None
        }
    }

    fn keystroke(self) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                platform: true,
                ..Modifiers::default()
            },
            key: match self {
                Self::Copy => "c",
                Self::Cut => "x",
                Self::Paste => "v",
                Self::SelectAll => "a",
            }
            .into(),
            key_char: None,
        }
    }
}

impl GPUITextInputView {
    fn edit_decision(&self, selector: Sel) -> EditDecision {
        let Some(action) = EditAction::from_selector(selector) else {
            return EditDecision::RefusedUnknownSelector;
        };
        let target = &self.ivars().target;
        editing_decision(
            action,
            target.native_edit_availability(),
            target.edit_window_active(),
            self.ivars().mode.get(),
            target.text_input_focused(),
        )
    }

    pub(super) fn editing_enabled(&self, selector: Sel) -> bool {
        self.edit_decision(selector).allowed()
    }

    pub(super) fn validate_edit_command(&self, command: &UICommand) {
        // SAFETY: UICommand always carries an action selector.
        let selector = unsafe { command.action() };
        if EditAction::from_selector(selector).is_some() {
            let mut attributes = command.attributes();
            attributes.set(
                UIMenuElementAttributes::Disabled,
                !self.editing_enabled(selector),
            );
            command.setAttributes(attributes);
        }
    }

    pub(super) fn perform_edit_action(
        &self,
        selector: Sel,
        sender: Option<&objc2::runtime::AnyObject>,
    ) {
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        let _outside_key_scope = observer::KeyScope::enter(None);
        // The observation entry precedes the menu-delivery gate, so a rejected
        // stale menu delivery is recorded as a terminal refusal, not skipped.
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        let invocation = observer::InvocationGuard::enter(
            &self.ivars().observation,
            match EditAction::from_selector(selector) {
                Some(EditAction::Copy) => observer::Action::Copy,
                Some(EditAction::Cut) => observer::Action::Cut,
                Some(EditAction::Paste) => observer::Action::Paste,
                Some(EditAction::SelectAll) => observer::Action::SelectAll,
                None => observer::Action::Unknown,
            },
        );
        let decision = self.edit_decision(selector);
        let checked = dispatch_checked_selector_inner(
            selector,
            self.ivars().target.menu_delivery(sender),
            decision.allowed(),
            self.ivars().target.modifiers(),
            |input| {
                #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
                {
                    self.ivars()
                        .target
                        .dispatch_native_edit(input, invocation.key())
                }
                #[cfg(not(all(target_os = "ios", target_abi = "sim", feature = "test-support")))]
                {
                    self.ivars().target.dispatch(input)
                }
            },
            || {
                let pasteboard = UIPasteboard::generalPasteboard();
                // SAFETY: this UIView is main-thread-only, so its pasteboard
                // access is serialized with the platform's clipboard access.
                if !unsafe { pasteboard.hasStrings() } {
                    return;
                }
                // SAFETY: as above; this is an explicit user paste action.
                if let Some(text) = unsafe { pasteboard.string() } {
                    self.with_handler(|handler| {
                        handler.replace_text_in_range(None, &text.to_string());
                    });
                }
            },
            #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
            invocation.key(),
        );
        self.refresh_snapshot();
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        match checked {
            CheckedDispatch::RejectedDelivery => {
                invocation.complete(
                    observer::Decision::RefusedMenuDelivery,
                    observer::Outcome::Refused,
                );
            }
            CheckedDispatch::Disabled => {
                invocation.complete(decision.observed(), observer::Outcome::Refused);
            }
            CheckedDispatch::Dispatched(summary) => invocation.complete(
                decision.observed(),
                observer::Outcome::Dispatched {
                    key_down_handled: summary.handled,
                    fallback: if summary.fallback_invoked {
                        observer::Fallback::Invoked
                    } else {
                        observer::Fallback::NotEligible
                    },
                },
            ),
        }
        #[cfg(not(all(target_os = "ios", target_abi = "sim", feature = "test-support")))]
        if let CheckedDispatch::Dispatched(summary) = checked {
            let _ = (summary.handled, summary.fallback_invoked);
        }
    }
}

// Legacy test entry points exercise the same production dispatcher. They do
// not manufacture an actual native observation key.
#[cfg(test)]
fn dispatch_edit_selector(
    selector: Sel,
    original: (Modifiers, Capslock),
    dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
) {
    if let Some(action) = EditAction::from_selector(selector) {
        dispatch_edit_action(action, original, dispatch, paste);
    }
}

#[cfg(test)]
fn dispatch_edit_action(
    action: EditAction,
    original: (Modifiers, Capslock),
    dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
) {
    let _ = dispatch_edit_action_inner(
        action,
        original,
        dispatch,
        paste,
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        None,
    );
}

#[cfg(test)]
fn dispatch_edit_action_inner(
    action: EditAction,
    original: (Modifiers, Capslock),
    dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))] key: Option<
        observer::ObservationKey,
    >,
) -> DispatchSummary {
    dispatch_menu_action(
        action,
        None,
        original,
        dispatch,
        paste,
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        key,
    )
}

struct DispatchSummary {
    handled: bool,
    fallback_invoked: bool,
}

/// What the menu-delivery and availability gates did with one native selector.
enum CheckedDispatch {
    RejectedDelivery,
    Disabled,
    Dispatched(DispatchSummary),
}

#[cfg(any(test, feature = "test-support"))]
pub(in crate::window) fn dispatch_checked_selector(
    selector: Sel,
    delivery: super::super::edit_menu::state::Delivery,
    enabled: bool,
    original: (Modifiers, Capslock),
    dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
) {
    let _ = dispatch_checked_selector_inner(
        selector,
        delivery,
        enabled,
        original,
        dispatch,
        paste,
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        None,
    );
}

fn dispatch_checked_selector_inner(
    selector: Sel,
    delivery: super::super::edit_menu::state::Delivery,
    enabled: bool,
    original: (Modifiers, Capslock),
    dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))] key: Option<
        observer::ObservationKey,
    >,
) -> CheckedDispatch {
    use super::super::edit_menu::state::Delivery;
    let token = match delivery {
        Delivery::Reject => return CheckedDispatch::RejectedDelivery,
        Delivery::Token(token) => Some(token),
        Delivery::Command | Delivery::Other => None,
    };
    if !enabled {
        return CheckedDispatch::Disabled;
    }
    let Some(action) = EditAction::from_selector(selector) else {
        return CheckedDispatch::Disabled;
    };
    CheckedDispatch::Dispatched(dispatch_menu_action(
        action,
        token,
        original,
        dispatch,
        paste,
        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
        key,
    ))
}

fn dispatch_menu_action(
    action: EditAction,
    token: Option<crate::NativeEditMenuToken>,
    original: (Modifiers, Capslock),
    mut dispatch: impl FnMut(PlatformInput) -> bool,
    paste: impl FnOnce(),
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))] key: Option<
        observer::ObservationKey,
    >,
) -> DispatchSummary {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    let _outside_key_scope = observer::KeyScope::enter(None);
    let keystroke = action.keystroke();
    let origin = NativeEditActionGuard::enter(Some(action));
    let menu_origin = crate::native_edit_menu::NativeEditMenuGuard::enter(token);
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    let key_scope = observer::KeyScope::enter(key);
    let handled = dispatch(PlatformInput::KeyDown(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    }));
    dispatch(PlatformInput::KeyUp(KeyUpEvent { keystroke }));
    drop(menu_origin);
    drop(origin);
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    drop(key_scope);
    dispatch(PlatformInput::ModifiersChanged(ModifiersChangedEvent {
        modifiers: original.0,
        capslock: original.1,
    }));
    let fallback_invoked = action == EditAction::Paste && !handled;
    if fallback_invoked {
        paste();
    }
    DispatchSummary {
        handled,
        fallback_invoked,
    }
}

#[cfg(test)]
mod tests;

// One decision for canPerformAction, validateCommand, and execution. Explicit
// consumers work in either keyboard mode; non-opted-in clients keep defaults.
#[cfg(test)]
fn editing_allowed(
    action: EditAction,
    override_state: Option<crate::NativeEditAvailability>,
    active: bool,
    mode: Option<Mode>,
    text_focused: bool,
) -> bool {
    editing_decision(action, override_state, active, mode, text_focused).allowed()
}

#[derive(Clone, Copy)]
enum EditDecision {
    RefusedUnknownSelector,
    RefusedInactiveOrNoResponder,
    RefusedPolicyDisallowed,
    RefusedNoTextMode,
    RefusedNoTextFocus,
    AllowedExplicitPolicy,
    AllowedDefaultText,
}
impl EditDecision {
    fn allowed(self) -> bool {
        matches!(self, Self::AllowedExplicitPolicy | Self::AllowedDefaultText)
    }
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    fn observed(self) -> observer::Decision {
        match self {
            Self::RefusedUnknownSelector => observer::Decision::RefusedUnknownSelector,
            Self::RefusedInactiveOrNoResponder => observer::Decision::RefusedInactiveOrNoResponder,
            Self::RefusedPolicyDisallowed => observer::Decision::RefusedPolicyDisallowed,
            Self::RefusedNoTextMode => observer::Decision::RefusedNoTextMode,
            Self::RefusedNoTextFocus => observer::Decision::RefusedNoTextFocus,
            Self::AllowedExplicitPolicy => observer::Decision::AllowedExplicitPolicy,
            Self::AllowedDefaultText => observer::Decision::AllowedDefaultText,
        }
    }
}
fn editing_decision(
    action: EditAction,
    override_state: Option<crate::NativeEditAvailability>,
    active: bool,
    mode: Option<Mode>,
    text_focused: bool,
) -> EditDecision {
    match override_state {
        Some(_) if !active => EditDecision::RefusedInactiveOrNoResponder,
        Some(state) => {
            if state.allows(action) {
                EditDecision::AllowedExplicitPolicy
            } else {
                EditDecision::RefusedPolicyDisallowed
            }
        }
        None if mode != Some(Mode::Text) => EditDecision::RefusedNoTextMode,
        None if !text_focused => EditDecision::RefusedNoTextFocus,
        None => EditDecision::AllowedDefaultText,
    }
}

#[cfg(all(test, target_os = "ios", target_abi = "sim", feature = "test-support"))]
mod observer_tests;
