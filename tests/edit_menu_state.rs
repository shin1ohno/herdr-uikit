#[path = "../src/window/edit_menu/responder_switch.rs"]
mod responder_switch;
#[path = "../src/window/edit_menu/state.rs"]
mod state;

use responder_switch::*;
use state::*;
use std::cell::{Cell, RefCell};

fn ready() -> EditMenuState {
    let mut s = EditMenuState::default();
    s.availability(true);
    s
}
fn show(s: &mut EditMenuState) -> NativeEditMenuToken {
    s.present().unwrap_or_else(|| panic!("available menu"))
}
fn identifier(t: NativeEditMenuToken) -> String {
    format!("{EDIT_MENU_IDENTIFIER_PREFIX}{}", t.value())
}

#[test]
fn owned_configurations_never_fall_back_to_untagged_suggestions() {
    let mut a = ready();
    let mut b = ready();
    let t1 = show(&mut a);
    let foreign = show(&mut b);
    assert_ne!(t1, foreign);
    assert_eq!(a.menu_for(Some(&identifier(t1))), MenuDecision::Items(t1));
    let t2 = show(&mut a);
    for id in [
        identifier(t1),
        identifier(foreign),
        format!("{EDIT_MENU_IDENTIFIER_PREFIX}bad"),
        format!("{EDIT_MENU_IDENTIFIER_PREFIX}18446744073709551616"),
    ] {
        assert_eq!(a.menu_for(Some(&id)), MenuDecision::Empty);
    }
    assert_eq!(a.menu_for(Some("other menu")), MenuDecision::Passthrough);
    assert_eq!(a.menu_for(None), MenuDecision::Passthrough);
    assert!(a.availability(false));
    assert_eq!(a.menu_for(Some(&identifier(t2))), MenuDecision::Empty);
    assert!(a.present().is_none());
    a.availability(true);
    assert_eq!(a.menu_for(Some(&identifier(t2))), MenuDecision::Empty);
}

#[test]
fn seven_revocations_are_irreversible_and_window_scoped() {
    for reason in [
        RevokeReason::Replaced,
        RevokeReason::Dismissed,
        RevokeReason::Unavailable,
        RevokeReason::Closed,
        RevokeReason::Inactive,
        RevokeReason::ResignedOutside,
        RevokeReason::SwitchLeftNoResponder,
    ] {
        let mut s = ready();
        let token = show(&mut s);
        assert_eq!(s.status(token), NativeEditMenuStatus::Current);
        assert!(s.revoke(reason));
        assert!(!s.revoke(RevokeReason::Closed));
        assert_eq!(s.status(token), NativeEditMenuStatus::Revoked(reason));
        let replacement = show(&mut s);
        assert_ne!(replacement, token);
        assert_eq!(
            s.delivery(Sender::Tagged(Some(token.value()))),
            Delivery::Reject
        );
        assert_eq!(s.status(token), NativeEditMenuStatus::Revoked(reason));
        assert_eq!(ready().status(token), NativeEditMenuStatus::Unknown);
    }
}

#[test]
fn sender_namespace_controls_delivery_not_the_command_class_alone() {
    assert_eq!(EDIT_MENU_PROPERTY_KEY, "dev.herdr.uikit.edit-menu.token");
    let mut s = ready();
    let t1 = show(&mut s);
    let t2 = show(&mut s);
    assert_eq!(
        s.status(t1),
        NativeEditMenuStatus::Revoked(RevokeReason::Replaced)
    );
    assert_eq!(
        s.delivery(Sender::Tagged(Some(t2.value()))),
        Delivery::Token(t2)
    );
    assert_eq!(
        s.delivery(Sender::Tagged(Some(t1.value()))),
        Delivery::Reject
    );
    assert_eq!(s.delivery(Sender::Tagged(None)), Delivery::Reject);
    assert_eq!(s.delivery(Sender::Command), Delivery::Command);
    assert_eq!(s.delivery(Sender::Other), Delivery::Other);
    assert!(!s.dismiss(t1));
    assert_eq!(s.status(t2), NativeEditMenuStatus::Current);
    assert!(s.dismiss(t2));
    assert_eq!(
        s.status(t2),
        NativeEditMenuStatus::Revoked(RevokeReason::Dismissed)
    );
}

struct Owner {
    state: RefCell<EditMenuState>,
    owned: Cell<bool>,
}
impl SwitchOwner for &Owner {
    fn begin(&self) {
        self.state.borrow_mut().begin_switch();
    }
    fn acquired(&self, result: bool) {
        self.state.borrow_mut().acquired(result);
    }
    fn finish(&self) {
        self.state.borrow_mut().end_switch(self.owned.get());
    }
}
struct Endpoint<'a> {
    owner: &'a Owner,
    acquire: bool,
}
impl Responder for Endpoint<'_> {
    fn acquire(&self) -> bool {
        self.owner.owned.set(self.acquire);
        self.acquire
    }
    fn resign(&self) -> bool {
        self.owner.owned.set(false);
        self.owner.state.borrow_mut().resigned();
        true
    }
}

#[test]
fn production_guard_preserves_successful_same_flush_restart_but_revokes_failure() {
    for (acquire, attempt) in [(true, true), (false, true), (false, false)] {
        let owner = Owner {
            state: RefCell::new(ready()),
            owned: Cell::new(true),
        };
        let token = show(&mut owner.state.borrow_mut());
        let endpoint = Endpoint {
            owner: &owner,
            acquire,
        };
        {
            let guard = ResponderSwitch::new(&owner);
            #[cfg(target_abi = "sim")]
            assert!(owner.state.borrow().switching());
            guard.resign(&endpoint);
            assert_eq!(
                owner.state.borrow().status(token),
                NativeEditMenuStatus::Current
            );
            if attempt {
                assert_eq!(guard.acquire(&endpoint), acquire);
            }
        }
        let expected = if acquire && attempt {
            NativeEditMenuStatus::Current
        } else {
            NativeEditMenuStatus::Revoked(RevokeReason::SwitchLeftNoResponder)
        };
        assert_eq!(owner.state.borrow().status(token), expected);
        owner.owned.set(true);
        assert_eq!(owner.state.borrow().status(token), expected);
        assert_eq!(
            owner
                .state
                .borrow()
                .delivery(Sender::Tagged(Some(token.value()))),
            if acquire && attempt {
                Delivery::Token(token)
            } else {
                Delivery::Reject
            }
        );
    }
}

#[test]
fn failed_acquire_revokes_even_if_another_responder_remains() {
    let owner = Owner {
        state: RefCell::new(ready()),
        owned: Cell::new(true),
    };
    let token = show(&mut owner.state.borrow_mut());
    let guard = ResponderSwitch::new(&owner);
    assert!(!guard.acquire(&Endpoint {
        owner: &owner,
        acquire: false
    }));
    owner.owned.set(true);
    drop(guard);
    assert_eq!(
        owner.state.borrow().status(token),
        NativeEditMenuStatus::Revoked(RevokeReason::SwitchLeftNoResponder)
    );
}

#[test]
fn external_resign_stays_revoked_after_return_and_nested_guards_share_failures() {
    let owner = Owner {
        state: RefCell::new(ready()),
        owned: Cell::new(true),
    };
    let token = show(&mut owner.state.borrow_mut());
    Endpoint {
        owner: &owner,
        acquire: true,
    }
    .resign();
    owner.owned.set(true);
    assert_eq!(
        owner.state.borrow().status(token),
        NativeEditMenuStatus::Revoked(RevokeReason::ResignedOutside)
    );
    let token = show(&mut owner.state.borrow_mut());
    let outer = ResponderSwitch::new(&owner);
    {
        let inner = ResponderSwitch::new(&owner);
        inner.acquire(&Endpoint {
            owner: &owner,
            acquire: false,
        });
    }
    owner.owned.set(true);
    drop(outer);
    assert_eq!(
        owner.state.borrow().status(token),
        NativeEditMenuStatus::Revoked(RevokeReason::SwitchLeftNoResponder)
    );
}

#[test]
fn presentation_requires_availability_live_active_window_and_responder() {
    for bits in 0..16 {
        let values = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
        assert_eq!(
            can_present(values.0, values.1, values.2, values.3),
            bits == 13
        );
    }
}
