//! One window's native modal gate: the state of the native search session it
//! presents, and the effects each transition asks of the window.
//!
//! The gate is shut (`blocks`) from the moment a presentation is accepted
//! until the GPUI responder is actually back, so every reader that would hand
//! first responder, keys, text or menu commands to GPUI checks one value.
//! Every session ends with exactly one `Dismissed` or `Failed` and then
//! exactly one `Restored`; transitions for another session are ignored.
//!
//! std only: the host tests (`tests/native_modal.rs`) include this file.

use std::fmt;

/// Identifies one presentation; never reused within a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Why a presented search panel closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DismissReason {
    // keep-sorted start
    /// The Cancel button.
    Cancel,
    /// Escape while the field was not composing.
    Escape,
    /// A tap outside the popover.
    OutsideTap,
    /// The consumer called `dismiss` or dropped its handle.
    Requested,
    /// Return or the software keyboard's search key, not composing.
    Return,
    /// The consumer closed it after a result was selected.
    Selected(u64),
    /// The sheet was swiped down to completion.
    SwipedDown,
    /// UIKit removed the panel without telling its delegate.
    System,
    /// The window closed or its scene disconnected.
    WindowClosed,
    // keep-sorted end
}

/// Why a presentation never became usable. Reported apart from dismissals,
/// so a consumer (and a test) can tell "could not open" from any path that
/// only fails after it opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentFailure {
    // keep-sorted start
    /// The field did not become first responder while the panel was up.
    FieldRefused,
    /// The root controller was already presenting something else.
    PresenterBusy,
    /// The window lost its UIWindow or scene before the deferred present.
    WindowDetached,
    // keep-sorted end
}

/// The GPUI view UIKit made first responder when the gate opened again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Responder {
    /// The drawing view (key-only mode).
    Drawing,
    /// The text input view (text mode).
    Text,
    /// The window closed first; nothing was restored.
    WindowClosed,
}

/// A notification for the session's consumer, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    Dismissed(DismissReason),
    Failed(PresentFailure),
    Opened,
    Restored(Responder),
}

/// What the window must do after a transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Ask UIKit to dismiss the panel (animated unless the window closes).
    Dismiss { animated: bool },
    /// Send the consumer a notification.
    Emit(Note),
    /// Give first responder back to GPUI's views and report what took it.
    Restore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Accepted; UIKit has not been asked yet (the call is deferred).
    Presenting { released: bool },
    /// On screen. `focused` once the field has become first responder.
    Presented { focused: bool },
    /// Dismissal asked of UIKit, completion pending. `released` when the
    /// consumer already dropped its handle.
    Dismissing {
        reason: DismissReason,
        released: bool,
    },
    /// Gone from the screen; the consumer still holds its handle.
    AwaitingConsumer,
    /// Handle dropped; waiting for a GPUI view to become first responder.
    Restoring,
}

/// Whether an app menu command runs: not when the key that chose it is
/// fenced, not when its origin window's gate was shut as UIKit performed it,
/// and not when that gate is shut as its deferred delivery runs.
pub fn command_runs(
    fenced_key: bool,
    shut_when_performed: bool,
    shut_when_delivered: bool,
) -> bool {
    !fenced_key && !shut_when_performed && !shut_when_delivered
}

/// Whether an app menu command shows enabled: never while the key window's
/// gate is shut (the panel's field keeps its own Edit commands).
pub fn command_enabled(key_window_shut: bool) -> bool {
    !key_window_shut
}

/// Refusal of a new presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Busy;

#[derive(Debug, Default)]
pub struct Gate {
    current: Option<(SessionId, Phase)>,
    last: u64,
}

type Effects = Vec<Effect>;

impl Gate {
    /// Whether any reader must keep GPUI out: from acceptance to restore.
    pub fn blocks(&self) -> bool {
        self.current.is_some()
    }

    pub fn phase(&self) -> Option<(SessionId, Phase)> {
        self.current
    }

    pub fn session(&self) -> Option<SessionId> {
        self.current.map(|(id, _)| id)
    }

    fn phase_of(&self, id: SessionId) -> Option<Phase> {
        self.current
            .and_then(|(current, phase)| (current == id).then_some(phase))
    }

    fn set(&mut self, id: SessionId, phase: Phase) {
        self.current = Some((id, phase));
    }

    /// Accepts a presentation, shutting the gate at once.
    pub fn request(&mut self) -> Result<SessionId, Busy> {
        if self.current.is_some() {
            return Err(Busy);
        }
        self.last += 1;
        let id = SessionId(self.last);
        self.set(id, Phase::Presenting { released: false });
        Ok(id)
    }

    /// The deferred present ran and UIKit finished presenting; `focused` is
    /// what the field's `becomeFirstResponder` returned.
    pub fn presented(&mut self, id: SessionId, focused: bool) -> Effects {
        match self.phase_of(id) {
            Some(Phase::Presenting { released: false }) => {
                self.set(id, Phase::Presented { focused });
                if focused {
                    vec![Effect::Emit(Note::Opened)]
                } else {
                    Vec::new()
                }
            }
            Some(Phase::Presenting { released: true }) => {
                self.set(
                    id,
                    Phase::Dismissing {
                        reason: DismissReason::Requested,
                        released: true,
                    },
                );
                vec![Effect::Dismiss { animated: false }]
            }
            // Dismissal began while UIKit was still presenting.
            Some(Phase::Dismissing { .. }) => vec![Effect::Dismiss { animated: false }],
            _ => Vec::new(),
        }
    }

    /// The field became first responder after a later attempt.
    pub fn focused(&mut self, id: SessionId) -> Effects {
        match self.phase_of(id) {
            Some(Phase::Presented { focused: false }) => {
                self.set(id, Phase::Presented { focused: true });
                vec![Effect::Emit(Note::Opened)]
            }
            _ => Vec::new(),
        }
    }

    /// The presentation could not be made usable. Nothing is on screen (or
    /// the window removes it), so the gate goes straight to restoring.
    pub fn failed(&mut self, id: SessionId, failure: PresentFailure) -> Effects {
        match self.phase_of(id) {
            Some(Phase::Presenting { .. }) => {
                self.set(id, Phase::Restoring);
                vec![Effect::Emit(Note::Failed(failure)), Effect::Restore]
            }
            Some(Phase::Presented { focused: false }) => {
                self.set(id, Phase::Restoring);
                vec![
                    Effect::Dismiss { animated: false },
                    Effect::Emit(Note::Failed(failure)),
                    Effect::Restore,
                ]
            }
            _ => Vec::new(),
        }
    }

    /// The app (Return, Escape, Cancel, a selection, the consumer) starts a
    /// dismissal. The first reason wins.
    pub fn dismiss(&mut self, id: SessionId, reason: DismissReason) -> Effects {
        match self.phase_of(id) {
            Some(Phase::Presenting { released }) => {
                // UIKit is asked once `presented` reports the panel up.
                self.set(id, Phase::Dismissing { reason, released });
                Vec::new()
            }
            Some(Phase::Presented { .. }) => {
                self.set(
                    id,
                    Phase::Dismissing {
                        reason,
                        released: false,
                    },
                );
                vec![Effect::Dismiss { animated: true }]
            }
            _ => Vec::new(),
        }
    }

    /// The panel left the screen: our dismissal's completion, or UIKit's own
    /// (outside tap, completed swipe, presenter gone) with `reason`.
    pub fn dismissed(&mut self, id: SessionId, reason: DismissReason) -> Effects {
        let (reason, released) = match self.phase_of(id) {
            Some(Phase::Dismissing { reason, released }) => (reason, released),
            Some(Phase::Presented { .. }) => (reason, false),
            _ => return Vec::new(),
        };
        let mut effects = vec![Effect::Emit(Note::Dismissed(reason))];
        if released {
            self.set(id, Phase::Restoring);
            effects.push(Effect::Restore);
        } else {
            self.set(id, Phase::AwaitingConsumer);
        }
        effects
    }

    /// The consumer dropped its handle (or called `finish`).
    pub fn release(&mut self, id: SessionId) -> Effects {
        match self.phase_of(id) {
            Some(Phase::Presenting { .. }) => {
                self.set(id, Phase::Presenting { released: true });
                Vec::new()
            }
            Some(Phase::Presented { .. }) => {
                self.set(
                    id,
                    Phase::Dismissing {
                        reason: DismissReason::Requested,
                        released: true,
                    },
                );
                vec![Effect::Dismiss { animated: true }]
            }
            Some(Phase::Dismissing { reason, .. }) => {
                self.set(
                    id,
                    Phase::Dismissing {
                        reason,
                        released: true,
                    },
                );
                Vec::new()
            }
            Some(Phase::AwaitingConsumer) => {
                self.set(id, Phase::Restoring);
                vec![Effect::Restore]
            }
            _ => Vec::new(),
        }
    }

    /// A GPUI view is first responder again: the gate opens.
    pub fn restored(&mut self, id: SessionId, responder: Responder) -> Effects {
        match self.phase_of(id) {
            Some(Phase::Restoring) => {
                self.current = None;
                vec![Effect::Emit(Note::Restored(responder))]
            }
            _ => Vec::new(),
        }
    }

    /// The deferred present found the session already released or dismissed:
    /// nothing reached the screen, so it is dismissed here, once.
    pub fn not_presented(&mut self, id: SessionId) -> Effects {
        match self.phase_of(id) {
            Some(Phase::Presenting { released }) => {
                self.set(
                    id,
                    Phase::Dismissing {
                        reason: DismissReason::Requested,
                        released,
                    },
                );
                self.dismissed(id, DismissReason::Requested)
            }
            Some(Phase::Dismissing { reason, .. }) => self.dismissed(id, reason),
            _ => Vec::new(),
        }
    }

    /// The window is closing: whatever the phase, the session ends now with
    /// one dismissal (if none was reported) and a `WindowClosed` restore.
    pub fn window_closed(&mut self) -> Effects {
        let Some((_, phase)) = self.current.take() else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        match phase {
            Phase::Presenting { .. } => {
                effects.push(Effect::Emit(Note::Dismissed(DismissReason::WindowClosed)));
            }
            Phase::Presented { .. } | Phase::Dismissing { .. } => {
                effects.push(Effect::Dismiss { animated: false });
                effects.push(Effect::Emit(Note::Dismissed(DismissReason::WindowClosed)));
            }
            Phase::AwaitingConsumer | Phase::Restoring => {}
        }
        effects.push(Effect::Emit(Note::Restored(Responder::WindowClosed)));
        effects
    }
}
