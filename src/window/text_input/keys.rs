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
use crate::menu::{self, EventSource, Route, Shortcut, Trigger};
use crate::native_edit_action::NativeEditActionGuard;
use crate::platform::application;

/// Seconds before a held key starts repeating, and between repeats, close to
/// iPadOS's defaults for hardware keyboards (which apps cannot read).
const REPEAT_DELAY: f64 = 0.5;
const REPEAT_INTERVAL: f64 = 0.033;

#[derive(Debug)]
struct Press {
    usage: isize,
    route: Route,
    key: String,
    /// The serial of the key event that delivered the press, 0 when none
    /// was being delivered.
    event: u64,
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
    /// The keystroke a menu key command last handed to GPUI, until a key is
    /// released or the modifiers change, so UIKit repeating the command
    /// reaches GPUI as a held key.
    menu_press: Option<Keystroke>,
}

impl KeyState {
    /// Whether a held key named `key` went to UIKit, so the text UIKit types
    /// for it is not dispatched to GPUI a second time.
    pub(super) fn is_forwarded(&self, key: &str) -> bool {
        self.presses
            .iter()
            .any(|press| press.route == Route::Forwarded && press.key == key)
    }

    /// The route of a press of `key` that the key event being delivered
    /// brought, so a key command for that same press is not offered to GPUI
    /// again. A press from an earlier key event is a different press of the
    /// key, or one whose release was lost, so it does not count. With no
    /// event being delivered any press still held counts.
    fn prior(&self, key: &str, source: EventSource) -> Option<Route> {
        let current = match source {
            EventSource::Keys { serial, .. } => Some(serial),
            EventSource::Unknown => None,
            EventSource::Other => return None,
        };
        self.presses
            .iter()
            .find(|press| press.key == key && current.is_none_or(|serial| press.event == serial))
            .map(|press| press.route)
    }

    pub(super) fn stop_repeat(&mut self) {
        self.repeat = None;
    }

    fn began(&mut self, usage: isize, route: Route, key: String, event: u64) {
        self.presses.retain(|press| press.usage != usage);
        self.presses.push(Press {
            usage,
            route,
            key,
            event,
        });
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
            self.ivars().keys.borrow_mut().menu_press = None;
            self.sync_flags(facts.flags);
            let Some(keystroke) = facts.keystroke else {
                self.forward(&press, event, Phase::Began);
                continue;
            };
            if keystroke.key == "escape" {
                self.trace_escape("native-press");
            }
            let route = if self.uikit_first(&keystroke) {
                Route::Forwarded
            } else if self.key_down(keystroke.clone(), false) {
                Route::Gpui
            } else {
                Route::Forwarded
            };
            if keystroke.key == "escape" {
                self.trace_escape(match route {
                    Route::Gpui => "press-handled-by-gpui",
                    Route::Forwarded => "press-forwarded-to-uikit",
                });
            }
            let serial = match application::event_source() {
                EventSource::Keys { serial, .. } => serial,
                EventSource::Unknown | EventSource::Other => 0,
            };
            self.ivars()
                .keys
                .borrow_mut()
                .began(facts.usage, route, keystroke.key.clone(), serial);
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
            let route = {
                let mut keys = self.ivars().keys.borrow_mut();
                keys.menu_press = None;
                keys.ended(facts.usage)
            };
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
            if let Some(keystroke) = facts.keystroke
                && !crate::key_routing::is_input_source_switch(&keystroke)
            {
                dispatch_key_up(keystroke, |input| self.ivars().target.dispatch(input));
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
    /// Input-source switching is always UIKit's. Other command and control shortcuts go to GPUI.
    fn uikit_first(&self, keystroke: &Keystroke) -> bool {
        crate::key_routing::uikit_first(
            keystroke,
            || {
                self.with_handler(|handler| handler.marked_text_range())
                    .flatten()
                    .is_some()
            },
            || {
                self.textInputMode()
                    .and_then(|mode| mode.primaryLanguage())
                    .is_some_and(|language| keymap::is_composition_language(&language.to_string()))
                    && self
                        .with_handler(|handler| handler.query_prefers_ime_for_printable_keys())
                        .unwrap_or(false)
            },
        )
    }

    /// A menu command with `shortcut` was performed. When its key press
    /// triggered it, the keystroke goes to GPUI as the press would have
    /// (`crate::menu`, "Keyboard precedence"). Returns whether that press
    /// was GPUI's, now or earlier in the same key event; otherwise the caller
    /// dispatches the item's action.
    ///
    /// No repeat timer starts here, unlike for a press GPUI keeps: were
    /// UIKit to send no `pressesEnded:` for a press its key command took,
    /// the timer would never stop. A repeat UIKit performs reaches GPUI as a
    /// held key instead.
    pub(super) fn menu_shortcut(&self, shortcut: &Shortcut) -> bool {
        let source = application::event_source();
        let keystroke = &shortcut.keystroke;
        if keystroke.key == "escape" {
            self.trace_escape("menu-command");
        }
        let prior = self.ivars().keys.borrow().prior(&keystroke.key, source);
        match menu::trigger(shortcut.flags, source, prior) {
            Trigger::Handled => true,
            Trigger::Declined | Trigger::Pointer => false,
            Trigger::Keyboard => {
                // The press itself never reached this view, so its modifiers
                // are adopted from the event, as for a press.
                if let EventSource::Keys { flags, .. } = source {
                    self.sync_flags(flags);
                }
                let held = {
                    let mut keys = self.ivars().keys.borrow_mut();
                    let held = keys.menu_press.as_ref() == Some(keystroke);
                    keys.menu_press = Some(keystroke.clone());
                    held
                };
                self.key_down(keystroke.clone(), held)
            }
        }
    }

    fn key_down(&self, keystroke: Keystroke, is_held: bool) -> bool {
        if keystroke.key == "escape" {
            self.trace_escape("gpui-keydown-dispatch");
        }
        dispatch_key_down(keystroke, is_held, |input| {
            self.ivars().target.dispatch(input)
        })
    }

    /// Opt-in routing evidence for a single known control, never text or
    /// arbitrary keystrokes. Normal input does not enable this diagnostic.
    pub(super) fn trace_escape(&self, stage: &'static str) {
        if std::env::var_os("HERDR_IOS_ESCAPE_DIAGNOSTICS").is_none() {
            return;
        }
        let composing = self.with_handler(|handler| handler.marked_text_range().is_some());
        tracing::debug!(
            stage,
            handler_available = composing.is_some(),
            composing = composing == Some(true),
            first_responder = self.isFirstResponder(),
            "Escape routing diagnostic"
        );
    }

    fn modifier_changed(&self, facts: &KeyFacts, pressed: bool) {
        let change = {
            let mut keys = self.ivars().keys.borrow_mut();
            if let Some(bit) = facts.modifier_bit {
                keys.held.set(bit, pressed);
            }
            keys.menu_press = None;
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
    fn sync_flags(&self, flags: objc2_ui_kit::UIKeyModifierFlags) {
        let change = {
            let mut keys = self.ivars().keys.borrow_mut();
            keys.held.sync(keymap::modifiers(flags));
            keys.capslock = keymap::capslock(flags);
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

/// The physical-key delivery path, shared by hardware presses and repeats.
pub(super) fn dispatch_key_down(
    keystroke: Keystroke,
    is_held: bool,
    dispatch: impl FnOnce(PlatformInput) -> bool,
) -> bool {
    // A reentrant physical press must not inherit a native command's scope.
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    let _observation = crate::native_edit_observer::KeyScope::enter(None);
    let _origin = NativeEditActionGuard::enter(None);
    dispatch(PlatformInput::KeyDown(KeyDownEvent {
        keystroke,
        is_held,
        prefer_character_input: false,
    }))
}

/// The physical release path, including cancelled presses.
pub(super) fn dispatch_key_up(
    keystroke: Keystroke,
    dispatch: impl FnOnce(PlatformInput) -> bool,
) -> bool {
    #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]
    let _observation = crate::native_edit_observer::KeyScope::enter(None);
    let _origin = NativeEditActionGuard::enter(None);
    dispatch(PlatformInput::KeyUp(KeyUpEvent { keystroke }))
}
