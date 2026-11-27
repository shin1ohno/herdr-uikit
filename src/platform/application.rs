//! The application class `UIApplicationMain` creates, a `UIApplication`
//! subclass that records which event UIKit is delivering, so a menu command
//! can tell a key press from a pick with the pointer or by touch
//! (`crate::menu`, "Keyboard precedence"). It changes nothing else: every
//! event goes on to UIKit's own `sendEvent:`.
//!
//! This assumes UIKit performs a key command synchronously while it delivers
//! the key event, inside `sendEvent:`; that has to be verified on a device.
//! Were it to perform it later, the source is unknown and the shortcut's
//! keystroke goes to GPUI, which keeps the keymap in charge.

#![allow(
    unsafe_code,
    reason = "defines an Objective-C class with objc2's define_class!"
)]

use std::cell::Cell;

use objc2::{ClassType, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::NSObject};
use objc2_foundation::NSString;
use objc2_ui_kit::{UIApplication, UIEvent, UIEventType, UIPressesEvent, UIResponder};

use crate::menu::EventSource;

thread_local! {
    /// The event `sendEvent:` is delivering on the main thread.
    static DELIVERING: Cell<EventSource> = const { Cell::new(EventSource::Unknown) };
    /// Key events delivered so far, numbering them.
    static KEY_EVENTS: Cell<u64> = const { Cell::new(0) };
    /// The key event being delivered carries a key the native modal gate
    /// fences (`crate::window::native_modal`).
    static FENCED: Cell<bool> = const { Cell::new(false) };
}

define_class!(
    // SAFETY: UIApplication may be subclassed when `UIApplicationMain` is
    // given the subclass's name; this one keeps `init`, adds no instance
    // variables, does not implement Drop, and its one override keeps the
    // superclass signature.
    #[unsafe(super(UIApplication, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    // A fixed name, like the delegates', so it does not change with the crate
    // version.
    #[name = "HerdrApplication"]
    pub(super) struct Application;

    impl Application {
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &UIEvent) {
            crate::native_operation::event(event.r#type() == UIEventType::Presses, || {
            let source = source(event);
            // Restored afterwards, so an event UIKit delivers from inside
            // another one leaves the outer one's source in place.
            let outer = DELIVERING.replace(source);
            let presses = event.downcast_ref::<UIPressesEvent>();
            let fenced = presses.is_some_and(|presses| {
                crate::window::native_modal::presses_will_deliver(presses, self.mtm())
            });
            let outer_fenced = FENCED.replace(fenced);
            // SAFETY: the argument is the event UIKit passed for this
            // selector; UIApplication's implementation returns nothing.
            let () = unsafe { msg_send![super(self), sendEvent: event] };
            FENCED.set(outer_fenced);
            if let Some(presses) = presses {
                crate::window::native_modal::presses_delivered(presses, self.mtm());
            }
            DELIVERING.set(outer);
            });
        }
    }
);

/// What `event` is, numbering it when it is a key event.
fn source(event: &UIEvent) -> EventSource {
    if event.r#type() != UIEventType::Presses {
        return EventSource::Other;
    }
    let serial = KEY_EVENTS.get().wrapping_add(1);
    KEY_EVENTS.set(serial);
    EventSource::Keys {
        flags: event.modifierFlags(),
        serial,
    }
}

/// Whether the key event being delivered carries a fenced key.
pub(crate) fn delivering_fenced() -> bool {
    FENCED.get()
}

/// The event UIKit is delivering now. Only the main thread's is recorded;
/// elsewhere it is always [`EventSource::Unknown`].
pub(crate) fn event_source() -> EventSource {
    DELIVERING.get()
}

/// The class name `UIApplicationMain` instantiates as the application.
pub(super) fn class_name() -> Retained<NSString> {
    NSString::from_class(Application::class())
}

#[cfg(test)]
mod tests;
