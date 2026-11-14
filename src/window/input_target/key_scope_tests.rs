//! Source regression for the generic ingress prologue; not a native callback receipt.
use super::*;
use crate::native_edit_observer as observer;
use gpui::{KeyDownEvent, Keystroke, Modifiers};
use std::cell::Cell;

fn synthetic_return() -> PlatformInput {
    PlatformInput::KeyDown(KeyDownEvent {
        keystroke: Keystroke {
            modifiers: Modifiers::default(),
            key: "enter".to_owned(),
            key_char: None,
        },
        is_held: false,
        prefer_character_input: false,
    })
}

#[test]
fn synthetic_key_inside_a_native_edit_key_scope_does_not_inherit_the_native_key() {
    observer::with_source_test_observer(|registration, _| {
        // Deliberately a source fixture entry; never a native proof.
        let inv = observer::InvocationGuard::enter(registration, observer::Action::Copy);
        let native_key = inv.key();
        assert!(
            native_key.is_some(),
            "the fixture must hand out a native key"
        );
        let target = InputTarget::new(Weak::new());
        let delivered = Cell::new(false);
        {
            // The software keyboard's Return can arrive while a native Edit
            // KeyDown is on the stack (`dispatch_native_edit` holds this scope).
            let _native = observer::KeyScope::enter(native_key);
            assert_eq!(observer::current_native_observation_key(), native_key);
            let handled = target.dispatch_via(synthetic_return(), |_| {
                delivered.set(true);
                assert_eq!(
                    observer::current_native_observation_key(),
                    None,
                    "generic ingress must not inherit the outer native key"
                );
                true
            });
            assert!(handled && delivered.get());
            assert_eq!(
                observer::current_native_observation_key(),
                native_key,
                "the outer native key scope is restored after the synthetic dispatch"
            );
        }
        assert_eq!(observer::current_native_observation_key(), None);
        inv.complete(
            observer::Decision::AllowedExplicitPolicy,
            observer::Outcome::Dispatched {
                key_down_handled: true,
                fallback: observer::Fallback::NotEligible,
            },
        );
    });
}
