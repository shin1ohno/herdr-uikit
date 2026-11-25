//! The inline field's decisions that need no UIKit object: what Return
//! commits, how long the text may get, where the field goes over a label,
//! what part of it is visible, why a session closed, and the session's
//! bounded event queue.
//!
//! std only (the waker is `std::task::Waker`): `tests/native_inline_field_policy.rs`
//! includes this file, with `gate.rs` beside it as `super::gate`.

use std::collections::VecDeque;
use std::task::Waker;

use super::gate::{DismissReason as GateReason, PresentFailure, Responder};

/// What Return does with the field's committed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Send this name (trimmed and at most `max_chars` characters).
    Commit(String),
    /// The name did not change: close without sending anything.
    Unchanged,
    /// The name is empty and must not be: keep the field and show the hint.
    Hint,
}

/// Return's decision for `text`, given the field's initial text.
pub fn commit_decision(initial: &str, text: &str, allow_empty: bool, max_chars: usize) -> Decision {
    let trimmed: String = text.trim().chars().take(max_chars).collect();
    if trimmed == initial.trim() {
        return Decision::Unchanged;
    }
    if trimmed.is_empty() && !allow_empty {
        return Decision::Hint;
    }
    Decision::Commit(trimmed)
}

/// `text` cut to `max_chars` characters, and whether it was cut. A field
/// that is composing (`marked`) is never cut: the IME still owns its text.
pub fn clamp(text: &str, max_chars: usize, marked: bool) -> (String, bool) {
    if marked || text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    (text.chars().take(max_chars).collect(), true)
}

/// A rectangle in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn right(self) -> f32 {
        self.x + self.w
    }
    fn bottom(self) -> f32 {
        self.y + self.h
    }
}

/// The field's rectangle over `label`: `outset_h` wider on each side,
/// `outset_v` taller on each side, and at least `min_width` wide (it grows
/// to the right, so the text keeps the label's left edge).
pub fn field_rect(label: Rect, outset_h: f32, outset_v: f32, min_width: f32) -> Rect {
    Rect {
        x: label.x - outset_h,
        y: label.y - outset_v,
        w: (label.w + 2.0 * outset_h).max(min_width),
        h: label.h + 2.0 * outset_v,
    }
}

/// The part of `frame` inside `clip`, and whether nothing of it is
/// (`true`: the field hides but keeps editing).
pub fn visible(frame: Rect, clip: Option<Rect>) -> (Rect, bool) {
    let Some(clip) = clip else {
        return (frame, false);
    };
    let x = frame.x.max(clip.x);
    let y = frame.y.max(clip.y);
    let right = frame.right().min(clip.right());
    let bottom = frame.bottom().min(clip.bottom());
    if right <= x || bottom <= y {
        return (
            Rect {
                x,
                y,
                w: 0.0,
                h: 0.0,
            },
            true,
        );
    }
    (
        Rect {
            x,
            y,
            w: right - x,
            h: bottom - y,
        },
        false,
    )
}

/// Why an inline field session closed. Every session reports exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeInlineDismissReason {
    // keep-sorted start
    /// Return with a changed name; `Committed` came just before.
    Committed,
    /// Escape while the field was not composing.
    Escape,
    /// A tap outside the field.
    OutsideTap,
    /// The field never became usable; nothing was edited.
    PresentationFailed(PresentFailure),
    /// The consumer called `dismiss` or dropped its handle, or UIKit
    /// removed the field on its own.
    Requested,
    /// Return with the name unchanged; nothing to send.
    Unchanged,
    /// The window closed or its scene disconnected.
    WindowClosed,
    // keep-sorted end
}

/// The inline field's session events, in order: `Opened` (the field is
/// first responder); at most one `Committed`; one `Dismissed`; and, after
/// the handle is dropped and a GPUI view is first responder again, one
/// `InputRestored`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeInlineFieldEvent {
    Opened,
    /// Return committed this name: trimmed, changed, and not empty unless the
    /// request allows it.
    Committed(String),
    Dismissed(NativeInlineDismissReason),
    InputRestored(Responder),
}

/// The inline field's reason for the gate's dismissal reason. Return is
/// `Committed` or `Unchanged` by the decision Return made; reasons only the
/// search panel has, and UIKit's own removal (`System`), are `Requested`.
pub fn dismiss_reason(gate: GateReason, decision: Option<&Decision>) -> NativeInlineDismissReason {
    match gate {
        GateReason::Return => match decision {
            Some(Decision::Commit(_)) => NativeInlineDismissReason::Committed,
            _ => NativeInlineDismissReason::Unchanged,
        },
        GateReason::Escape => NativeInlineDismissReason::Escape,
        GateReason::OutsideTap => NativeInlineDismissReason::OutsideTap,
        GateReason::WindowClosed => NativeInlineDismissReason::WindowClosed,
        GateReason::Cancel
        | GateReason::Requested
        | GateReason::Selected(_)
        | GateReason::SwipedDown
        | GateReason::System => NativeInlineDismissReason::Requested,
    }
}

/// How many events a session's queue holds; a session sends at most four.
pub const EVENT_CAPACITY: usize = 8;

/// The queue was full; the event was not added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

/// A FIFO of at most `cap` events that wakes its reader.
#[derive(Debug)]
pub struct Bounded<E> {
    events: VecDeque<E>,
    cap: usize,
    waker: Option<Waker>,
    closed: bool,
}

impl<E> Default for Bounded<E> {
    fn default() -> Self {
        Self::new(EVENT_CAPACITY)
    }
}

impl<E> Bounded<E> {
    pub fn new(cap: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(cap),
            cap,
            waker: None,
            closed: false,
        }
    }

    /// Appends `event`, or refuses it when `cap` events are waiting. Nothing
    /// is added after `close`.
    pub fn push(&mut self, event: E) -> Result<(), Full> {
        if self.closed {
            return Ok(());
        }
        if self.events.len() >= self.cap {
            return Err(Full);
        }
        self.events.push_back(event);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
        Ok(())
    }

    pub fn pop(&mut self) -> Option<E> {
        self.events.pop_front()
    }

    /// No more events will come; a waiting reader is woken to see the end.
    pub fn close(&mut self) {
        self.closed = true;
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn register(&mut self, waker: &Waker) {
        self.waker = Some(waker.clone());
    }
}
