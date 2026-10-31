//! Hardware key presses: `pressesBegan:withEvent:` and friends become GPUI
//! `KeyDown`, `KeyUp`, and `ModifiersChanged`.
//!
//! A key GPUI handles stops here. A key it declines goes on to `super`, where
//! UIKit's text system types it (calling `insertText:` or the IME) and
//! repeats it while held. UIKit sends one `pressesBegan:` per press and never
//! repeats keys GPUI kept, so those repeat on a timer here. While an IME is
//! composing, keys go to UIKit first, as on macOS, so return and the arrows
//! commit and navigate the composition instead of reaching key bindings.

#![allow(
    unsafe_code,
    reason = "forwarding unhandled presses to UIResponder's implementation"
)]

use gpui::{
    Capslock, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ModifiersChangedEvent, PlatformInput,
};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadOnly, msg_send, sel};
use objc2_foundation::NSSet;
use objc2_ui_kit::{UIKey, UIPress, UIPressesEvent};
use smallvec::SmallVec;

use super::keymap::{self, HeldModifiers, KeyEvent};
use super::view_class::GPUITextInputView;

/// Seconds before a held key starts repeating, and between repeats, close to
/// iPadOS's defaults for hardware keyboards (which apps cannot read).
const REPEAT_DELAY: f64 = 0.5;
const REPEAT_INTERVAL: f64 = 0.033;

/// Who received a key's press, so its release goes to the same place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// GPUI handled the press; UIKit never saw it.
    Gpui,
    /// The press went on to UIKit's text system.
    Forwarded,
}

#[derive(Debug)]
struct Press {
    usage: isize,
    route: Route,
    key: String,
}

#[derive(Debug)]
struct Repeat {
    usage: isize,
    keystroke: Keystroke,
}

/// Keys in flight between press and release.
#[derive(Debug, Default)]
pub(super) struct KeyState {
    held: HeldModifiers,
    capslock: bool,
    /// The modifiers and caps lock GPUI was last told about.
    reported: (Modifiers, bool),
    presses: SmallVec<[Press; 4]>,
    repeat: Option<Repeat>,
}

impl KeyState {
    /// Whether a held key named `key` went to UIKit, so the text UIKit types
    /// for it is not dispatched to GPUI a second time.
    pub(super) fn is_forwarded(&self, key: &str) -> bool {
        self.presses
            .iter()
            .any(|press| press.route == Route::Forwarded && press.key == key)
    }

    pub(super) fn stop_repeat(&mut self) {
        self.repeat = None;
    }

    fn began(&mut self, usage: isize, route: Route, key: String) {
        self.presses.retain(|press| press.usage != usage);
        self.presses.push(Press { usage, route, key });
    }

    fn ended(&mut self, usage: isize) -> Option<Route> {
        let index = self.presses.iter().position(|press| press.usage == usage)?;
        Some(self.presses.remove(index).route)
    }

    /// The modifier state to report, if it differs from the last report.
    fn modifiers_change(&mut self) -> Option<ModifiersChangedEvent> {
        let current = (self.held.modifiers(), self.capslock);
        if current == self.reported {
            return None;
        }
        self.reported = current;
        Some(ModifiersChangedEvent {
            modifiers: current.0,
            capslock: Capslock { on: current.1 },
        })
    }
}

/// A press's key as owned strings, since `UIKey` hands out fresh `NSString`s.
struct KeyFacts {
    usage: isize,
    keystroke: Option<Keystroke>,
    modifier_bit: Option<u8>,
    capslock_key: bool,
    flags: objc2_ui_kit::UIKeyModifierFlags,
}

impl KeyFacts {
    fn new(key: &UIKey) -> Self {
        let usage = key.keyCode();
        let characters = key.characters().to_string();
        let unmodified = key.charactersIgnoringModifiers().to_string();
        let flags = key.modifierFlags();
        Self {
            usage: usage.0,
            keystroke: keymap::keystroke(&KeyEvent {
                usage,
                characters: &characters,
                unmodified: &unmodified,
                flags,
            }),
            modifier_bit: keymap::modifier_bit(usage),
            capslock_key: keymap::is_capslock(usage),
            flags,
        }
    }

    fn is_modifier(&self) -> bool {
        self.modifier_bit.is_some() || self.capslock_key
    }
}

impl GPUITextInputView {
    pub(super) fn presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
        let mtm = self.mtm();
        for press in presses.iter() {
            let Some(key) = press.key(mtm) else {
                self.forward(&press, event, Phase::Began);
                continue;
            };
            let facts = KeyFacts::new(&key);
            if facts.is_modifier() {
                self.modifier_changed(&facts, true);
                // UIKit tracks modifiers for its own shortcuts and the
                // command-key overlay.
                self.forward(&press, event, Phase::Began);
                continue;
            }
            self.sync_modifiers(&facts);
            let Some(keystroke) = facts.keystroke else {
                self.forward(&press, event, Phase::Began);
                continue;
            };
            let route = if self.uikit_first(&keystroke) {
                Route::Forwarded
            } else if self.key_down(keystroke.clone(), false) {
                Route::Gpui
            } else {
                Route::Forwarded
            };
            self.ivars()
                .keys
                .borrow_mut()
                .began(facts.usage, route, keystroke.key.clone());
            match route {
                Route::Gpui => self.start_repeat(facts.usage, keystroke),
                Route::Forwarded => {
                    self.stop_repeat();
                    self.forward(&press, event, Phase::Began);
                }
            }
        }
    }

    /// `pressesEnded:` and `pressesCancelled:`: GPUI hears every release,
    /// and UIKit the releases of presses it saw begin.
    pub(super) fn presses_ended(
        &self,
        presses: &NSSet<UIPress>,
        event: Option<&UIPressesEvent>,
        cancelled: bool,
    ) {
        let phase = if cancelled {
            Phase::Cancelled
        } else {
            Phase::Ended
        };
        let mtm = self.mtm();
        for press in presses.iter() {
            let Some(key) = press.key(mtm) else {
                self.forward(&press, event, phase);
                continue;
            };
            let facts = KeyFacts::new(&key);
            if facts.is_modifier() {
                self.modifier_changed(&facts, false);
                self.forward(&press, event, phase);
                continue;
            }
            let route = self.ivars().keys.borrow_mut().ended(facts.usage);
            let repeating = self
                .ivars()
                .keys
                .borrow()
                .repeat
                .as_ref()
                .is_some_and(|repeat| repeat.usage == facts.usage);
            if repeating || cancelled {
                self.stop_repeat();
            }
            if let Some(keystroke) = facts.keystroke {
                self.ivars()
                    .target
                    .dispatch(PlatformInput::KeyUp(KeyUpEvent { keystroke }));
            }
            // A press that began before this view was first responder has no
            // route; UIKit is the one that may have seen it.
            if route != Some(Route::Gpui) {
                self.forward(&press, event, phase);
            }
        }
    }

    /// While an IME composes, or when the focused element wants printable
    /// keys to reach a composing input mode first, UIKit gets the key first.
    /// Command and control shortcuts still go to GPUI.
    fn uikit_first(&self, keystroke: &Keystroke) -> bool {
        if keystroke.modifiers.platform || keystroke.modifiers.control {
            return false;
        }
        let composing = self
            .with_handler(|handler| handler.marked_text_range())
            .flatten()
            .is_some();
        if composing {
            return true;
        }
        let printable = keystroke
            .key_char
            .as_deref()
            .is_some_and(|text| !text.chars().any(char::is_control));
        printable
            && self
                .textInputMode()
                .and_then(|mode| mode.primaryLanguage())
                .is_some_and(|language| keymap::is_composition_language(&language.to_string()))
            && self
                .with_handler(|handler| handler.query_prefers_ime_for_printable_keys())
                .unwrap_or(false)
    }

    fn key_down(&self, keystroke: Keystroke, is_held: bool) -> bool {
        self.ivars()
            .target
            .dispatch(PlatformInput::KeyDown(KeyDownEvent {
                keystroke,
                is_held,
                prefer_character_input: false,
            }))
    }

    fn modifier_changed(&self, facts: &KeyFacts, pressed: bool) {
        let change = {
            let mut keys = self.ivars().keys.borrow_mut();
            if let Some(bit) = facts.modifier_bit {
                keys.held.set(bit, pressed);
            }
            keys.capslock = keymap::capslock(facts.flags);
            keys.modifiers_change()
        };
        if let Some(change) = change {
            self.ivars()
                .target
                .dispatch(PlatformInput::ModifiersChanged(change));
        }
    }

    /// Adopts the modifiers a key reports when they disagree with the presses
    /// seen, such as a modifier held down before this view took focus.
    fn sync_modifiers(&self, facts: &KeyFacts) {
        let change = {
            let mut keys = self.ivars().keys.borrow_mut();
            keys.held.sync(keymap::modifiers(facts.flags));
            keys.capslock = keymap::capslock(facts.flags);
            keys.modifiers_change()
        };
        if let Some(change) = change {
            self.ivars()
                .target
                .dispatch(PlatformInput::ModifiersChanged(change));
        }
    }

    fn start_repeat(&self, usage: isize, keystroke: Keystroke) {
        self.cancel_repeat_timer();
        self.ivars().keys.borrow_mut().repeat = Some(Repeat { usage, keystroke });
        self.perform_after(sel!(herdrRepeatKey:), REPEAT_DELAY);
    }

    fn stop_repeat(&self) {
        self.ivars().keys.borrow_mut().stop_repeat();
        self.cancel_repeat_timer();
    }

    /// The repeat timer: one held `KeyDown`, then the next tick.
    pub(super) fn repeat_key(&self) {
        let keystroke = self
            .ivars()
            .keys
            .borrow()
            .repeat
            .as_ref()
            .map(|repeat| repeat.keystroke.clone());
        let Some(keystroke) = keystroke else {
            return;
        };
        // A release can be lost when focus leaves mid-press; stop rather
        // than repeat into a view that no longer has the keyboard.
        if !self.can_receive_keys() {
            self.ivars().keys.borrow_mut().stop_repeat();
            return;
        }
        self.key_down(keystroke, true);
        self.perform_after(sel!(herdrRepeatKey:), REPEAT_INTERVAL);
    }

    fn forward(&self, press: &Retained<UIPress>, event: Option<&UIPressesEvent>, phase: Phase) {
        let presses = NSSet::from_retained_slice(std::slice::from_ref(press));
        // SAFETY: each selector is UIResponder's press handler for its phase,
        // taking a set of presses and the optional event UIKit passed.
        let () = unsafe {
            match phase {
                Phase::Began => msg_send![super(self), pressesBegan: &*presses, withEvent: event],
                Phase::Ended => msg_send![super(self), pressesEnded: &*presses, withEvent: event],
                Phase::Cancelled => {
                    msg_send![super(self), pressesCancelled: &*presses, withEvent: event]
                }
            }
        };
    }
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Began,
    Ended,
    Cancelled,
}
