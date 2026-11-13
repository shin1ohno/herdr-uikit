//! Native menu interaction and responder lifetime, owned by one window.
mod delegate;
pub(crate) mod state;
pub(super) use delegate::classify;
pub(super) mod responder_switch;
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod tests;

use self::{delegate::EditMenuDelegate, responder_switch::SwitchOwner, state::*};
use super::{scene, state::Window};
use gpui::{AnyWindowHandle, Pixels, Point};
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation::CGPoint;
use objc2_foundation::NSString;
use objc2_ui_kit::{UIEditMenuConfiguration, UIEditMenuInteraction, UIView};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub(super) struct EditMenu {
    _delegate: Retained<EditMenuDelegate>,
    interaction: Retained<UIEditMenuInteraction>,
    pub(super) state: RefCell<EditMenuState>,
    #[cfg(target_abi = "sim")]
    pub(super) observation: RefCell<SwitchTrace>,
}

impl EditMenu {
    pub(super) fn new(view: &UIView, owner: Weak<Window>, mtm: MainThreadMarker) -> Self {
        let delegate = EditMenuDelegate::new(owner, mtm);
        let interaction = UIEditMenuInteraction::initWithDelegate(
            UIEditMenuInteraction::alloc(mtm),
            Some(ProtocolObject::from_ref(&*delegate)),
        );
        view.addInteraction(ProtocolObject::from_ref(&*interaction));
        Self {
            _delegate: delegate,
            interaction,
            state: RefCell::default(),
            #[cfg(target_abi = "sim")]
            observation: RefCell::default(),
        }
    }
    pub(super) fn revoke(&self, reason: RevokeReason) {
        let revoked = self.state.borrow_mut().revoke(reason);
        if revoked {
            self.interaction.dismissMenu();
        }
    }
    pub(super) fn dismiss(&self, token: NativeEditMenuToken) -> bool {
        let revoked = self.state.borrow_mut().dismiss(token);
        if revoked {
            self.interaction.dismissMenu();
        }
        revoked
    }
    pub(super) fn availability(&self, available: bool) {
        let revoked = self.state.borrow_mut().availability(available);
        if revoked {
            self.interaction.dismissMenu();
        }
    }
    pub(super) fn resigned(&self) {
        let revoked = self.state.borrow_mut().resigned();
        if revoked {
            self.interaction.dismissMenu();
        }
    }
}

impl SwitchOwner for &Window {
    fn begin(&self) {
        #[cfg(target_abi = "sim")]
        if !self.edit_menu.state.borrow().switching() {
            let mut trace = self.edit_menu.observation.borrow_mut();
            trace.restart = false;
            trace.acquires.clear();
        }
        self.edit_menu.state.borrow_mut().begin_switch();
    }
    fn acquired(&self, result: bool) {
        self.edit_menu.state.borrow_mut().acquired(result);
        #[cfg(target_abi = "sim")]
        self.edit_menu
            .observation
            .borrow_mut()
            .acquires
            .push(result);
    }
    fn finish(&self) {
        let drawing_owned = self.view.isFirstResponder();
        let text_owned = self.text_input.view().isFirstResponder();
        let owned = drawing_owned || text_owned;
        let revoked = self.edit_menu.state.borrow_mut().end_switch(owned);
        #[cfg(target_abi = "sim")]
        if !self.edit_menu.state.borrow().switching() {
            self.edit_menu
                .observation
                .borrow_mut()
                .finish(drawing_owned, text_owned);
        }
        if revoked {
            self.edit_menu.interaction.dismissMenu();
        }
    }
}

pub(crate) fn present(handle: AnyWindowHandle, at: Point<Pixels>) -> Option<NativeEditMenuToken> {
    let window = scene::menu_window(handle)?;
    let valid = {
        let s = window.state.borrow();
        state::can_present(
            s.native_edit_availability.is_some(),
            s.closed,
            s.is_active,
            window.view.isFirstResponder() || window.text_input.view().isFirstResponder(),
        )
    };
    if !valid {
        return None;
    }
    let token = window.edit_menu.state.borrow_mut().present()?;
    // UIKit can query text handlers synchronously. Present after GPUI's update,
    // and revalidate because an intervening dismissal must cancel the request.
    window.defer(move |window| {
        if window.edit_menu.state.borrow().status(token) != NativeEditMenuStatus::Current {
            return;
        }
        let id = NSString::from_str(&format!("{EDIT_MENU_IDENTIFIER_PREFIX}{}", token.value()));
        #[allow(
            unsafe_code,
            reason = "NSString conforms to NSCopying and UIKit copies it"
        )]
        let configuration = unsafe {
            UIEditMenuConfiguration::configurationWithIdentifier_sourcePoint(
                Some(ProtocolObject::from_ref(&*id)),
                CGPoint::new(f64::from(at.x), f64::from(at.y)),
                window.mtm,
            )
        };
        window
            .edit_menu
            .interaction
            .presentEditMenuWithConfiguration(&configuration);
    });
    Some(token)
}

pub(crate) fn dismiss(handle: AnyWindowHandle, token: NativeEditMenuToken) -> bool {
    scene::menu_window(handle).is_some_and(|w| w.edit_menu.dismiss(token))
}
pub(crate) fn status(handle: AnyWindowHandle, token: NativeEditMenuToken) -> NativeEditMenuStatus {
    scene::menu_window(handle).map_or(NativeEditMenuStatus::Unknown, |w| {
        w.edit_menu.state.borrow().status(token)
    })
}

impl SwitchOwner for Rc<Window> {
    fn begin(&self) {
        (&**self).begin();
    }
    fn acquired(&self, result: bool) {
        (&**self).acquired(result);
    }
    fn finish(&self) {
        (&**self).finish();
    }
}

#[cfg(target_abi = "sim")]
#[derive(Default)]
pub(super) struct SwitchTrace {
    pub(super) restart: bool,
    acquires: Vec<bool>,
    events: std::collections::VecDeque<crate::NativeEditMenuSwitchObservation>,
    dropped: usize,
}
#[cfg(target_abi = "sim")]
impl SwitchTrace {
    fn finish(&mut self, drawing_owned: bool, text_owned: bool) {
        if !self.restart && self.acquires.is_empty() {
            return;
        }
        if self.events.len() == 256 {
            self.events.pop_front();
            self.dropped += 1;
        }
        self.events
            .push_back(crate::NativeEditMenuSwitchObservation {
                restart_reached: self.restart,
                acquire_returns: std::mem::take(&mut self.acquires),
                drawing_owned,
                text_owned,
            });
    }
}
#[cfg(target_abi = "sim")]
pub(crate) fn take_observations(
    handle: AnyWindowHandle,
) -> (usize, Vec<crate::NativeEditMenuSwitchObservation>) {
    let Some(window) = scene::menu_window(handle) else {
        return (0, Vec::new());
    };
    let mut trace = window.edit_menu.observation.borrow_mut();
    (
        std::mem::take(&mut trace.dropped),
        trace.events.drain(..).collect(),
    )
}
