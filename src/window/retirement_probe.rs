//! Test-support only: injected events for the UIApplicationMain example.
//! These helpers use the real window and native objects. They do not emulate
//! UIKit's choice of destination for queued keyboard/IME input.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use gpui::{AnyWindowHandle, KeyDownEvent, Keystroke, PlatformInput};
use objc2::MainThreadMarker;

pub use super::text_input::probe::{NativeClient, NativeQuery, NativeQueryError};
use super::{scene, state::Window};

fn window(handle: AnyWindowHandle) -> Option<Rc<Window>> {
    scene::live_windows(MainThreadMarker::new()?)
        .into_iter()
        .find(|window| window.handle == handle && !window.state.borrow().closed)
}

/// Retain the actual current native object, including after its retirement.
pub fn capture(handle: AnyWindowHandle) -> Option<NativeClient> {
    Some(NativeClient::new(&window(handle)?.text_input))
}

/// Inject through the real platform window callback slot, like a drawing-view
/// event. The example's GPUI key listener calls the public retirement API.
pub fn key(handle: AnyWindowHandle, key: &str, native_origin: bool) -> bool {
    let Some(window) = window(handle) else {
        return false;
    };
    let event = PlatformInput::KeyDown(KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: None,
            modifiers: Default::default(),
        },
        is_held: false,
        prefer_character_input: false,
    });
    if native_origin {
        NativeClient::new(&window.text_input).dispatch(event)
    } else {
        window.dispatch(event)
    }
}

thread_local! {
    static NEXT: RefCell<VecDeque<Box<dyn FnOnce()>>> = RefCell::new(VecDeque::new());
}

/// Schedule probe work outside a GPUI borrow; no test guard masks production
/// entry points. This queue is only for the probe, never input recovery.
pub fn next_native_turn(f: impl FnOnce() + 'static) {
    assert!(MainThreadMarker::new().is_some());
    NEXT.with_borrow_mut(|queue| queue.push_back(Box::new(f)));
    dispatch2::DispatchQueue::main().exec_async(|| {
        let next = NEXT.with_borrow_mut(|queue| queue.pop_front());
        if let Some(next) = next {
            next();
        }
    });
}
