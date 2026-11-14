//! Native endpoint and scene registry TEST DOUBLES. These do not test UIKit.
use crate::{
    AnyWindowHandle, MainThreadMarker, NativeInputOrigin, NativeRetirementRequestStatus,
    RetiredTextInputSession, TextInputRetirement, TextInputRetirementError,
    TextInputRetirementStatus, TextInputSessionState,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

mod client_lifetime;
mod native_retirement_request;
mod retirement;
pub(crate) use native_retirement_request::*;
pub(crate) use retirement::*;

pub(crate) struct Window {
    pub(crate) handle: AnyWindowHandle,
    pub(crate) state: RefCell<State>,
    pub(crate) text_input: TextInputView,
}
#[derive(Default)]
pub(crate) struct State {
    pub(crate) closed: bool,
    pub(crate) native_request_serial: u64,
    pub(crate) native_request_pending: Option<u64>,
    pub(crate) native_hardware_observed: bool,
}
pub(crate) struct Target {
    window: Weak<Window>,
    pub(crate) client: Rc<client_lifetime::ClientLifetime>,
}
impl Target {
    pub(crate) fn origin(&self) -> NativeInputOrigin {
        NativeInputOrigin {
            window: self.window.clone(),
            client: self.client.identity,
        }
    }
    fn successor(&self) -> Option<Self> {
        Some(Self {
            window: self.window.clone(),
            client: self.client.successor()?,
        })
    }
}
pub(crate) struct NativeEndpoint {
    target: Target,
}
impl NativeEndpoint {
    pub(crate) fn native_target(&self) -> &Target {
        &self.target
    }
    pub(crate) fn fence_reserved(&self) {
        self.target.client.retire();
    }
    pub(crate) fn cancel_reserved_work(&self) {}
}
pub(crate) mod text_input {
    pub(crate) use super::NativeEndpoint;
}
pub(crate) struct TextInputView {
    view: RefCell<Rc<NativeEndpoint>>,
    pub(crate) replacing: Cell<bool>,
    pub(crate) constructed: Cell<u32>,
    pub(crate) acquire: RefCell<Option<Box<dyn FnOnce()>>>,
    pub(crate) ready: Cell<bool>,
}
impl TextInputView {
    pub(crate) fn captured_native(&self) -> Rc<NativeEndpoint> {
        self.view.borrow().clone()
    }
    pub(crate) fn is_captured_current(&self, old: &NativeEndpoint) -> bool {
        std::ptr::eq(&*self.captured_native(), old)
    }
    fn install(&self, next: Target) -> NativeInputOrigin {
        let origin = next.origin();
        self.constructed.set(self.constructed.get() + 1);
        *self.view.borrow_mut() = Rc::new(NativeEndpoint { target: next });
        let hook = self.acquire.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        self.replacing.set(false);
        origin
    }
    pub(crate) fn replace_reserved_native(
        &self,
        old: Rc<NativeEndpoint>,
    ) -> NativeRetirementRequestStatus {
        let next = old.target.successor().expect("checked successor");
        let successor = self.install(next);
        NativeRetirementRequestStatus::Replaced {
            successor,
            first_responder: self.ready.get(),
            responder_requested: true,
        }
    }
    pub(crate) fn begin_retirement(
        &self,
    ) -> Result<(Rc<NativeEndpoint>, Target, TextInputRetirement), TextInputRetirementError> {
        if self.replacing.get() {
            return Err(TextInputRetirementError::InProgress);
        }
        let old = self.captured_native();
        if old.target.client.busy() {
            return Err(TextInputRetirementError::NativeCallbackInFlight);
        }
        let next = old
            .target
            .successor()
            .ok_or(TextInputRetirementError::IdentityExhausted)?;
        let receipt = TextInputRetirement {
            sessions: RetiredTextInputSession {
                retired: old.target.client.identity,
                replacement: next.client.identity,
            },
            status: Rc::new(Cell::new(TextInputRetirementStatus::Pending)),
        };
        self.replacing.set(true);
        old.target.client.retire();
        Ok((old, next, receipt))
    }
    pub(crate) fn finish_retirement(
        &self,
        _old: Rc<NativeEndpoint>,
        next: Target,
        receipt: &TextInputRetirement,
    ) {
        self.install(next);
        receipt.status.set(TextInputRetirementStatus::Replaced {
            first_responder: self.ready.get(),
            responder_requested: true,
        });
    }
    pub(crate) fn session_state(&self) -> TextInputSessionState {
        let old = self.captured_native();
        TextInputSessionState {
            client: old.target.client.identity,
            retired: !old.target.client.live(),
            replacement_pending: self.replacing.get(),
            first_responder: self.ready.get(),
            attached: true,
        }
    }
}

pub(crate) mod scene {
    use super::*;
    thread_local! { pub(super) static WINDOWS: RefCell<Vec<Weak<Window>>> = const { RefCell::new(Vec::new()) }; }
    pub(crate) fn live_windows(_: MainThreadMarker) -> Vec<Rc<Window>> {
        WINDOWS.with_borrow(|ws| ws.iter().filter_map(Weak::upgrade).collect())
    }
}
pub(crate) fn fixture(handle: u64, identity: u64) -> Rc<Window> {
    let window = Rc::new_cyclic(|weak| Window {
        handle,
        state: RefCell::new(State::default()),
        text_input: TextInputView {
            view: RefCell::new(Rc::new(NativeEndpoint {
                target: Target {
                    window: weak.clone(),
                    client: client_lifetime::fixture(identity),
                },
            })),
            replacing: Cell::new(false),
            constructed: Cell::new(0),
            acquire: RefCell::new(None),
            ready: Cell::new(true),
        },
    });
    scene::WINDOWS.with_borrow_mut(|ws| ws.push(Rc::downgrade(&window)));
    window
}
