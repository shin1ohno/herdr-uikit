//! Source-stage native fixtures: actual selectors/raw closures, never OS proof.
#![allow(
    unsafe_code,
    reason = "typed UIKit selectors for an isolated receiving fixture"
)]
use std::{cell::Cell, ptr::NonNull, rc::Weak};

use gpui::AnyWindowHandle;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject},
};
use objc2_foundation::{NSError, NSSet, NSString};
use objc2_ui_kit::{
    UIApplication, UIKey, UIKeyModifierFlags, UIKeyboardHIDUsage, UIPasteboard, UIPress,
    UIPressPhase, UIPressType, UIView,
};

use crate::NativeEditAction;

/// A non-key callback-slot sentinel; it does not invent physical key history.
pub fn callback_sentinel(handle: AnyWindowHandle) -> bool {
    let Some(window) = super::window(handle) else {
        return false;
    };
    window.dispatch(gpui::PlatformInput::ModifiersChanged(
        gpui::ModifiersChangedEvent {
            modifiers: Default::default(),
            capslock: Default::default(),
        },
    ));
    true
}

/// Only fixed fixture text, on an isolated leased simulator's pasteboard.
pub fn set_fixture_pasteboard(text: &str) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    // SAFETY: explicit main-thread fixture setup; never read the user's clipboard.
    unsafe {
        UIPasteboard::generalPasteboard().setString(Some(&NSString::from_str(text)));
    }
    true
}

/// One native Edit root on the text object or its actual drawing parent.
pub fn edit(handle: AnyWindowHandle, action: NativeEditAction, drawing: bool) -> bool {
    let Some(window) = super::window(handle) else {
        return false;
    };
    let text: Retained<UIView> = window.text_input.view();
    let view = if drawing {
        text.superview()
    } else {
        Some(text)
    };
    let Some(view) = view else {
        return false;
    };
    // SAFETY: the standard Edit selectors take one nullable sender and return
    // void. Do not use performSelector's object return ABI for these methods.
    unsafe {
        let () = match action {
            NativeEditAction::Copy => msg_send![&*view, copy: None::<&AnyObject>],
            NativeEditAction::Cut => msg_send![&*view, cut: None::<&AnyObject>],
            NativeEditAction::Paste => msg_send![&*view, paste: None::<&AnyObject>],
            NativeEditAction::SelectAll => msg_send![&*view, selectAll: None::<&AnyObject>],
        };
    }
    true
}

/// An empty press set through the actual selector: hardware history, but no
/// press for the ledger. This is injected, not a physical keyboard.
pub fn empty_hardware_entry(handle: AnyWindowHandle) -> bool {
    let Some(window) = super::window(handle) else {
        return false;
    };
    let view = window.text_input.view();
    let presses = NSSet::<UIPress>::new();
    // SAFETY: standard UIResponder argument types, including a nil event.
    let () = unsafe { msg_send![&*view, pressesBegan: &*presses, withEvent: None::<&AnyObject>] };
    true
}

pub struct ProbePressIvars {
    key: Option<Retained<UIKey>>,
    phase: Cell<UIPressPhase>,
}

define_class!(
    // SAFETY: a test-support UIPress whose getters UIKit and the SDK read; it
    // adds no designated initializer and does not implement Drop.
    #[unsafe(super(UIPress, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrProbePress"]
    #[ivars = ProbePressIvars]
    pub struct ProbePress;

    impl ProbePress {
        #[unsafe(method_id(key))]
        fn __key(&self) -> Option<Retained<UIKey>> {
            self.ivars().key.clone()
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

define_class!(
    // SAFETY: a test-support UIKey that types nothing: no characters, no
    // modifiers and HID usage 0, which maps to no GPUI keystroke.
    #[unsafe(super(UIKey, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrProbeKey"]
    struct ProbeKey;

    impl ProbeKey {
        #[unsafe(method(keyCode))]
        fn __key_code(&self) -> UIKeyboardHIDUsage {
            UIKeyboardHIDUsage(0)
        }

        #[unsafe(method_id(characters))]
        fn __characters(&self) -> Retained<NSString> {
            NSString::new()
        }

        #[unsafe(method_id(charactersIgnoringModifiers))]
        fn __characters_ignoring_modifiers(&self) -> Retained<NSString> {
            NSString::new()
        }

        #[unsafe(method(modifierFlags))]
        fn __modifier_flags(&self) -> UIKeyModifierFlags {
            UIKeyModifierFlags::empty()
        }
    }
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbePhase {
    Began,
    Ended,
}

/// One injected press object, kept so its Ended is the same object as its Began.
pub struct ProbePressHandle(Retained<ProbePress>);

/// A new injected press, with a key (that types nothing) or without one.
pub fn probe_press(keyed: bool) -> Option<ProbePressHandle> {
    let mtm = MainThreadMarker::new()?;
    let key = keyed.then(|| {
        // SAFETY: NSObject's `init` on a subclass without ivars.
        let key: Retained<ProbeKey> =
            unsafe { msg_send![super(ProbeKey::alloc(mtm).set_ivars(())), init] };
        Retained::into_super(key)
    });
    let this = ProbePress::alloc(mtm).set_ivars(ProbePressIvars {
        key,
        phase: Cell::new(UIPressPhase::Began),
    });
    // SAFETY: NSObject's `init`; the ivars were set before it runs.
    let press: Retained<ProbePress> = unsafe { msg_send![super(this), init] };
    Some(ProbePressHandle(press))
}

/// Send `press` to the text view (or its drawing parent) through the actual
/// press selector for `phase`. This is injected, not a physical keyboard.
pub fn inject_press(
    handle: AnyWindowHandle,
    press: &ProbePressHandle,
    phase: ProbePhase,
    drawing: bool,
) -> bool {
    let Some(window) = super::window(handle) else {
        return false;
    };
    let text: Retained<UIView> = window.text_input.view();
    let view = if drawing {
        text.superview()
    } else {
        Some(text)
    };
    let Some(view) = view else {
        return false;
    };
    press.0.ivars().phase.set(match phase {
        ProbePhase::Began => UIPressPhase::Began,
        ProbePhase::Ended => UIPressPhase::Ended,
    });
    let presses = NSSet::from_retained_slice(&[Retained::into_super(press.0.clone())]);
    // SAFETY: standard UIResponder argument types, including a nil event.
    let () = unsafe {
        match phase {
            ProbePhase::Began => {
                msg_send![&*view, pressesBegan: &*presses, withEvent: None::<&AnyObject>]
            }
            ProbePhase::Ended => {
                msg_send![&*view, pressesEnded: &*presses, withEvent: None::<&AnyObject>]
            }
        }
    };
    true
}

pub fn on_raw_ingress(label: &'static str, callback: impl FnOnce() + 'static) -> bool {
    crate::native_operation::probe::register(label, callback)
}

/// Exercise the actual application delegate -> shared::later queue path.
pub fn raw_later() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = UIApplication::sharedApplication(mtm);
    // SAFETY: UIApplicationMain owns the SDK delegate for the lifetime of
    // this isolated main-thread fixture, including this read and callback.
    let Some(delegate) = (unsafe { app.delegate() }) else {
        return false;
    };
    // SAFETY: the SDK delegate implements this standard UIApplication callback.
    let () = unsafe { msg_send![&*delegate, applicationDidReceiveMemoryWarning: &*app] };
    true
}

/// Exercise actual raw teardown for a disposable fixture window/scene only.
pub fn raw_scene_destroy(handle: AnyWindowHandle) -> bool {
    let Some(window) = super::window(handle) else {
        return false;
    };
    let scene = window.state.borrow().scene.clone();
    let Some(scene) = scene else {
        return false;
    };
    super::super::scene::destroy_unwanted(&scene, window.mtm);
    true
}

/// Invoke the actual error-handler queue with a deliberately stale request ID.
/// It must take the no-window branch without changing any real scene request.
pub fn raw_scene_refusal() -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    let mut requests = super::super::requests::Requests::<Weak<super::super::Window>>::default();
    let id = requests.push(Weak::new());
    // No real scene requests are installed by this isolated test helper. The
    // caller must run after all fixture windows have connected (stage contract).
    let callback = super::super::scene::new_scene_refused(id);
    let error = NSError::new(-1, &NSString::from_str("SDK fixture"));
    callback.call((NonNull::from(&*error),));
    true
}
