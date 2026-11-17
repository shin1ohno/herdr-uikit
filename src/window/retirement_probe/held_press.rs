//! Test-support only, iOS Simulator only: a keyed press held across real
//! time, sent through the text view's actual press selectors
//! (`pressesBegan/Changed/Ended/Cancelled:withEvent:`), so the press ledger,
//! the key state and the SDK's own repeat timer all run as for a hardware
//! key. The same press object is reused from `Began` to its release.
//!
//! This is injected, not a physical keyboard: UIKit's press tracking never
//! sees it, so the order of phases and views is the caller's script, not
//! evidence of what iPadOS delivers. See `docs/held-press-injection.md`.
#![allow(
    unsafe_code,
    reason = "test-support UIPress/UIKey subclasses and typed UIKit press selectors"
)]

mod script;

use std::cell::{Cell, RefCell};

use gpui::AnyWindowHandle;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, MessageReceiver, NSObject, Sel},
};
use objc2_foundation::{NSSet, NSString};
use objc2_ui_kit::{
    UIKey, UIKeyModifierFlags, UIKeyboardHIDUsage, UIPress, UIPressPhase, UIPressType, UIView,
};

pub use script::{
    HeldProbeDestination, HeldProbeKeySpec, HeldProbePhase, HeldProbeScript, HeldProbeScriptError,
};

use super::NativeClient;

pub struct HeldProbeKeyIvars {
    usage: isize,
    characters: Retained<NSString>,
}

define_class!(
    // SAFETY: a test-support UIKey that reports one fixed key with no
    // modifiers; it adds no designated initializer and does not implement Drop.
    #[unsafe(super(UIKey, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrHeldProbeKey"]
    #[ivars = HeldProbeKeyIvars]
    pub struct HeldProbeKey;

    impl HeldProbeKey {
        #[unsafe(method(keyCode))]
        fn __key_code(&self) -> UIKeyboardHIDUsage {
            UIKeyboardHIDUsage(self.ivars().usage)
        }

        #[unsafe(method_id(characters))]
        fn __characters(&self) -> Retained<NSString> {
            self.ivars().characters.clone()
        }

        #[unsafe(method_id(charactersIgnoringModifiers))]
        fn __characters_ignoring_modifiers(&self) -> Retained<NSString> {
            self.ivars().characters.clone()
        }

        #[unsafe(method(modifierFlags))]
        fn __modifier_flags(&self) -> UIKeyModifierFlags {
            UIKeyModifierFlags::empty()
        }
    }
);

pub struct HeldProbePressIvars {
    key: Retained<UIKey>,
    phase: Cell<UIPressPhase>,
}

define_class!(
    // SAFETY: a test-support UIPress whose getters UIKit and the SDK read; it
    // adds no designated initializer and does not implement Drop.
    #[unsafe(super(UIPress, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrHeldProbePress"]
    #[ivars = HeldProbePressIvars]
    pub struct HeldProbePress;

    impl HeldProbePress {
        #[unsafe(method_id(key))]
        fn __key(&self) -> Option<Retained<UIKey>> {
            Some(self.ivars().key.clone())
        }

        #[unsafe(method(phase))]
        fn __phase(&self) -> UIPressPhase {
            self.ivars().phase.get()
        }

        /// No TV-remote or arrow type, so no focus or remote handling.
        #[unsafe(method(type))]
        fn __type(&self) -> UIPressType {
            UIPressType(-1)
        }
    }
);

/// Where [`inject_held_press`] sends a phase.
#[derive(Clone, Copy)]
pub enum HeldProbeTarget<'a> {
    /// The window's text view at the time of the call.
    Current,
    /// A text view captured with `retirement_probe::capture` that has since
    /// retired (it is no longer the window's text view and is fenced).
    Retired(&'a NativeClient),
}

/// Why nothing was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldProbeError {
    /// Off the main thread.
    NotMainThread,
    /// The window is closed or not in a live scene.
    WindowUnavailable,
    /// A `Retired` target is still the window's text view, or still live.
    TargetNotRetired,
    /// The phase script refused the step.
    Script(HeldProbeScriptError),
}

/// One injected press object and its phase script.
pub struct HeldProbeHandle {
    press: Retained<HeldProbePress>,
    script: RefCell<HeldProbeScript>,
}

impl HeldProbeHandle {
    /// The phases this press has sent so far.
    pub fn script(&self) -> HeldProbeScript {
        self.script.borrow().clone()
    }
}

/// A new press object carrying `key`, not yet sent anywhere.
pub fn held_press(key: HeldProbeKeySpec) -> Option<HeldProbeHandle> {
    let mtm = MainThreadMarker::new()?;
    let key = HeldProbeKey::alloc(mtm).set_ivars(HeldProbeKeyIvars {
        usage: key.usage,
        characters: NSString::from_str(key.characters),
    });
    // SAFETY: NSObject's `init`; the ivars were set before it runs.
    let key: Retained<HeldProbeKey> = unsafe { msg_send![super(key), init] };
    let press = HeldProbePress::alloc(mtm).set_ivars(HeldProbePressIvars {
        key: Retained::into_super(key),
        phase: Cell::new(UIPressPhase::Began),
    });
    // SAFETY: NSObject's `init`; the ivars were set before it runs.
    let press: Retained<HeldProbePress> = unsafe { msg_send![super(press), init] };
    Some(HeldProbeHandle {
        press,
        script: RefCell::new(HeldProbeScript::default()),
    })
}

/// Send `phase` of `press` to `target` through the actual press selector,
/// with a nil event, after the phase script admits it. The call returns
/// after the view's selector body (including any SDK `KeyDown`/`KeyUp`) ran.
pub fn inject_held_press(
    handle: AnyWindowHandle,
    press: &HeldProbeHandle,
    phase: HeldProbePhase,
    target: HeldProbeTarget<'_>,
) -> Result<(), HeldProbeError> {
    if MainThreadMarker::new().is_none() {
        return Err(HeldProbeError::NotMainThread);
    }
    let window = super::window(handle).ok_or(HeldProbeError::WindowUnavailable)?;
    let current = NativeClient::new(&window.text_input);
    let (client, destination) = match target {
        HeldProbeTarget::Current => (current, HeldProbeDestination::Current),
        HeldProbeTarget::Retired(old) => {
            if old.identity() == current.identity() || old.live() {
                return Err(HeldProbeError::TargetNotRetired);
            }
            (old.clone(), HeldProbeDestination::Retired)
        }
    };
    press
        .script
        .borrow_mut()
        .step(phase, destination, client.identity())
        .map_err(HeldProbeError::Script)?;
    press
        .press
        .ivars()
        .phase
        .set(UIPressPhase(phase.uikit_phase()));
    let presses = NSSet::from_retained_slice(&[Retained::into_super(press.press.clone())]);
    let view: Retained<UIView> = client.view();
    let selector = Sel::register(phase.selector());
    // SAFETY: every selector `HeldProbePhase::selector` names is a standard
    // UIResponder press handler taking an NSSet of UIPress and a nullable
    // UIPressesEvent and returning void; the view is a main-thread UIView.
    let () = unsafe { (&*view).send_message(selector, (&*presses, None::<&AnyObject>)) };
    Ok(())
}
