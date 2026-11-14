//! Irreversible per-window menu ownership, independent of UIKit.
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

pub const EDIT_MENU_PROPERTY_KEY: &str = "dev.herdr.uikit.edit-menu.token";
pub const EDIT_MENU_IDENTIFIER_PREFIX: &str = "dev.herdr.uikit.edit-menu.config:";

/// Process-unique identity of one presentation. Revoked identities never return.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NativeEditMenuToken(u64);
impl NativeEditMenuToken {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevokeReason {
    Replaced,
    Dismissed,
    Unavailable,
    Closed,
    Inactive,
    ResignedOutside,
    SwitchLeftNoResponder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeEditMenuStatus {
    Current,
    Revoked(RevokeReason),
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuDecision {
    Items(NativeEditMenuToken),
    Empty,
    Passthrough,
}

/// A present namespace with a malformed value is still owned, never untagged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sender {
    Tagged(Option<u64>),
    Command,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    Token(NativeEditMenuToken),
    Reject,
    Command,
    Other,
}

#[derive(Default)]
pub(crate) struct EditMenuState {
    current: Option<NativeEditMenuToken>,
    history: BTreeMap<NativeEditMenuToken, RevokeReason>,
    available: bool,
    switch_depth: usize,
    acquire_failed: bool,
}

impl EditMenuState {
    pub(crate) fn present(&mut self) -> Option<NativeEditMenuToken> {
        if !self.available {
            return None;
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        // Exhaustion fails closed rather than wrapping and reusing a token.
        let value = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .ok()?;
        self.revoke(RevokeReason::Replaced);
        let token = NativeEditMenuToken(value);
        self.current = Some(token);
        Some(token)
    }
    pub(crate) fn revoke(&mut self, reason: RevokeReason) -> bool {
        if let Some(token) = self.current.take() {
            self.history.insert(token, reason);
            true
        } else {
            false
        }
    }
    pub(crate) fn availability(&mut self, available: bool) -> bool {
        self.available = available;
        !available && self.revoke(RevokeReason::Unavailable)
    }
    pub(crate) fn status(&self, token: NativeEditMenuToken) -> NativeEditMenuStatus {
        if self.current == Some(token) {
            NativeEditMenuStatus::Current
        } else {
            self.history
                .get(&token)
                .copied()
                .map(NativeEditMenuStatus::Revoked)
                .unwrap_or(NativeEditMenuStatus::Unknown)
        }
    }
    pub(crate) fn dismiss(&mut self, token: NativeEditMenuToken) -> bool {
        self.current == Some(token) && self.revoke(RevokeReason::Dismissed)
    }
    pub(crate) fn menu_for(&self, identifier: Option<&str>) -> MenuDecision {
        let Some(value) = identifier.and_then(|s| s.strip_prefix(EDIT_MENU_IDENTIFIER_PREFIX))
        else {
            return MenuDecision::Passthrough;
        };
        match (
            value.parse::<u64>().ok().map(NativeEditMenuToken),
            self.current,
        ) {
            (Some(token), Some(current)) if token == current && self.available => {
                MenuDecision::Items(token)
            }
            _ => MenuDecision::Empty,
        }
    }
    pub(crate) fn delivery(&self, sender: Sender) -> Delivery {
        match sender {
            Sender::Tagged(Some(value))
                if self.current == Some(NativeEditMenuToken(value)) && self.available =>
            {
                Delivery::Token(NativeEditMenuToken(value))
            }
            Sender::Tagged(_) => Delivery::Reject,
            Sender::Command => Delivery::Command,
            Sender::Other => Delivery::Other,
        }
    }
    pub(crate) fn resigned(&mut self) -> bool {
        resign_revokes(self.switch_depth != 0) && self.revoke(RevokeReason::ResignedOutside)
    }
    #[cfg(target_abi = "sim")]
    pub(crate) fn switching(&self) -> bool {
        self.switch_depth != 0
    }
    pub(crate) fn begin_switch(&mut self) {
        self.switch_depth += 1;
    }
    pub(crate) fn acquired(&mut self, result: bool) {
        self.acquire_failed |= !result;
    }
    pub(crate) fn end_switch(&mut self, has_responder: bool) -> bool {
        self.switch_depth -= 1;
        if self.switch_depth != 0 {
            return false;
        }
        let failed = std::mem::take(&mut self.acquire_failed) || !has_responder;
        failed && self.revoke(RevokeReason::SwitchLeftNoResponder)
    }
}

pub(crate) fn resign_revokes(internal_switch: bool) -> bool {
    !internal_switch
}

pub(crate) fn can_present(available: bool, closed: bool, active: bool, responder: bool) -> bool {
    available && !closed && active && responder
}
