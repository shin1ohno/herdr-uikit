//! What the text input view keeps between UIKit's calls: the window it
//! serves, the mode the window asked for, the last text state UIKit was told
//! about, and the hardware keys in flight.

use std::cell::{Cell, RefCell};

use gpui::{Bounds, Pixels};
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_ui_kit::{UITextInputDelegate, UIView};

use super::super::input_target::InputTarget;
pub(super) use super::super::text_state::TextState;
use super::keys::KeyState;
use super::traits::Traits;

/// Whether the software keyboard may show. The drawing view owns first
/// responder in key-only mode and forwards hardware presses here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// The focused element takes text: no `inputView`, so UIKit shows the
    /// software keyboard when no hardware keyboard is attached.
    Text,
    /// Nothing takes text: the drawing view receives hardware keys, avoiding
    /// UIKit's assistant bar for a text responder with an empty `inputView`.
    KeyOnly,
}

/// Whether a flush may make the view first responder. Flushes run about once
/// per drawn frame, so only a request may take first responder: when UIKit
/// gives it to another responder (the keyboard's dismiss key resigning it, an
/// alert's or a picker's text field), it stays there until the window asks
/// for a mode again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Acquire {
    /// Leave first responder where it is: the view has it, or UIKit took it
    /// away after the last request.
    #[default]
    Settled,
    /// The window asked for a mode, or the view moved into a window.
    Requested,
    /// Becoming first responder failed (the scene was not ready); later
    /// flushes retry without reporting it again.
    Retrying,
}

impl Acquire {
    /// Only a text client may acquire the text view. Key-only input belongs
    /// to the drawing view, even when an older text request is still queued.
    pub(super) fn for_mode(mode: Option<Mode>) -> Self {
        match mode {
            Some(Mode::Text) => Self::Requested,
            Some(Mode::KeyOnly) | None => Self::Settled,
        }
    }

    /// Whether a flush calls `becomeFirstResponder` on a view that is not
    /// first responder.
    pub(super) fn wants_first_responder(self) -> bool {
        self != Self::Settled
    }

    /// Expanding a minimized keyboard requires a fresh native acquisition;
    /// ordinary flushes and a never-shown keyboard must keep their responder.
    pub(super) fn needs_responder_restart(self, minimized: bool) -> bool {
        self == Self::Requested && minimized
    }

    /// Whether a failed attempt is the first for this request, and worth
    /// reporting.
    pub(super) fn reports_failure(self) -> bool {
        self == Self::Requested
    }

    /// The state after a flush called `becomeFirstResponder`.
    pub(super) fn attempted(self, became_first_responder: bool) -> Self {
        if became_first_responder {
            Self::Settled
        } else {
            Self::Retrying
        }
    }
}

pub(super) struct Ivars {
    pub(super) target: InputTarget,
    /// The mode the window last asked for; `None` until it asks.
    pub(super) mode: Cell<Option<Mode>>,
    /// The mode UIKit last saw, to reload input views only on a change;
    /// `None` while the view is not first responder.
    pub(super) applied_mode: Cell<Option<Mode>>,
    pub(super) acquire: Cell<Acquire>,
    /// The zero-height `inputView` of [`Mode::KeyOnly`].
    pub(super) key_only_input_view: Retained<UIView>,
    pub(super) traits: Cell<Traits>,
    pub(super) applied_traits: Cell<Traits>,
    /// The text state as UIKit last knew it, to tell it about changes GPUI
    /// made on its own (focus moves, edits by key bindings).
    pub(super) snapshot: RefCell<TextState>,
    /// The caret GPUI last reported, for geometry the handler cannot give.
    pub(super) caret: Cell<Option<Bounds<Pixels>>>,
    pub(super) input_delegate: RefCell<Weak<ProtocolObject<dyn UITextInputDelegate>>>,
    pub(super) marked_text_style: RefCell<Option<Retained<AnyObject>>>,
    /// A flush is queued on the run loop (see `TextInputView`).
    pub(super) flush_scheduled: Cell<bool>,
    pub(super) keys: RefCell<KeyState>,
}

impl Ivars {
    pub(super) fn new(target: InputTarget, key_only_input_view: Retained<UIView>) -> Self {
        Self {
            target,
            mode: Cell::new(None),
            applied_mode: Cell::new(None),
            acquire: Cell::new(Acquire::default()),
            key_only_input_view,
            traits: Cell::new(Traits::default()),
            applied_traits: Cell::new(Traits::default()),
            snapshot: RefCell::new(TextState::default()),
            caret: Cell::new(None),
            input_delegate: RefCell::new(Weak::default()),
            marked_text_style: RefCell::new(None),
            flush_scheduled: Cell::new(false),
            keys: RefCell::new(KeyState::default()),
        }
    }
}

/// What UIKit must be told after the text state changed underneath it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Change {
    None,
    Selection,
    Text,
}

impl TextState {
    /// Handler availability, composition, document size, or text at a fixed
    /// caret require text callbacks. A caret move uses selection callbacks.
    pub(super) fn change_since(&self, old: &Self) -> Change {
        if self.handler != old.handler || self.marked != old.marked || self.len != old.len {
            Change::Text
        } else if self.selection != old.selection {
            // Moving the caret also moves the sampled context window. That
            // alone is a selection change, not an outside text edit.
            Change::Selection
        } else if self.context != old.context {
            Change::Text
        } else {
            Change::None
        }
    }

    /// The end of the document. Handlers that do not report a length (a
    /// terminal, whose document is only its composition) end at the furthest
    /// offset they did report.
    pub(super) fn document_len(&self) -> usize {
        self.len.unwrap_or_else(|| {
            let selection = self.selection.as_ref().map_or(0, |(range, _)| range.end);
            let marked = self.marked.as_ref().map_or(0, |range| range.end);
            selection.max(marked)
        })
    }
}
