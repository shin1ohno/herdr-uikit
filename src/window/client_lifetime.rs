//! Identity belongs to the native object, never to a callback's arrival time.

use std::{cell::Cell, rc::Rc};

#[derive(Default)]
pub(crate) struct ClientLifetime {
    pub(crate) identity: u64,
    retired: Cell<bool>,
    callbacks: Cell<usize>,
}

impl ClientLifetime {
    pub(crate) fn successor(&self) -> Option<Rc<Self>> {
        Some(Rc::new(Self {
            identity: self.identity.checked_add(1)?,
            ..Self::default()
        }))
    }
    pub(crate) fn live(&self) -> bool {
        !self.retired.get()
    }
    pub(crate) fn busy(&self) -> bool {
        self.callbacks.get() != 0
    }
    pub(crate) fn retire(&self) {
        self.retired.set(true);
    }
    pub(crate) fn enter(&self) -> Option<Callback<'_>> {
        if !self.live() {
            return None;
        }
        self.callbacks.set(self.callbacks.get() + 1);
        Some(Callback(self))
    }
}

pub(crate) struct Callback<'a>(&'a ClientLifetime);
impl Drop for Callback<'_> {
    fn drop(&mut self) {
        self.0.callbacks.set(self.0.callbacks.get() - 1);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    #[test]
    fn retired_origin_cannot_be_relabelled_by_a_new_client() {
        let old = Rc::new(ClientLifetime::default());
        let callback_origin = old.clone();
        let new = old.successor().expect("identity available");
        old.retire();
        assert!(
            callback_origin.enter().is_none(),
            "old origin must stay closed"
        );
        assert!(
            new.enter().is_some(),
            "new identical text must be admissible"
        );
        assert_eq!((old.identity, new.identity), (0, 1));
    }
    #[test]
    fn callback_depth_survives_nested_calls() {
        let client = ClientLifetime::default();
        let outer = client.enter().expect("live");
        {
            let _inner = client.enter();
            assert!(client.busy());
        }
        assert!(client.busy());
        drop(outer);
        assert!(!client.busy());
    }
    #[test]
    fn identities_do_not_wrap() {
        let client = ClientLifetime {
            identity: u64::MAX,
            ..ClientLifetime::default()
        };
        assert!(client.successor().is_none());
    }
}
