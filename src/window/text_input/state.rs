//! What the text input view keeps between UIKit's calls: the window it
//! serves, the mode the window asked for, the last text state UIKit was told
//! about, and the hardware keys in flight.

use std::cell::{Cell, RefCell};
use std::ops::Range;

use gpui::{Bounds, Pixels, PlatformInputHandler};
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_ui_kit::{UITextInputDelegate, UIView};

use super::super::input_target::InputTarget;
use super::keys::KeyState;
use super::traits::Traits;

/// Whether the software keyboard may show. The view stays first responder in
/// both modes so hardware key presses keep reaching GPUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// The focused element takes text: no `inputView`, so UIKit shows the
    /// software keyboard when no hardware keyboard is attached.
    Text,
    /// Nothing takes text: an empty `inputView` keeps the software keyboard
    /// hidden while hardware keys still arrive for shortcuts.
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
    /// Whether a flush calls `becomeFirstResponder` on a view that is not
    /// first responder.
    pub(super) fn wants_first_responder(self) -> bool {
        self != Self::Settled
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
    pub(super) snapshot: RefCell<Snapshot>,
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
            snapshot: RefCell::new(Snapshot::default()),
            caret: Cell::new(None),
            input_delegate: RefCell::new(Weak::default()),
            marked_text_style: RefCell::new(None),
            flush_scheduled: Cell::new(false),
            keys: RefCell::new(KeyState::default()),
        }
    }
}

/// The focused element's text state in UTF-16 offsets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    /// GPUI has an input handler for the focused element.
    pub(super) handler: bool,
    pub(super) selection: Option<Range<usize>>,
    pub(super) marked: Option<Range<usize>>,
    /// The document length, when the handler knows it.
    pub(super) len: Option<usize>,
}

/// What UIKit must be told after the text state changed underneath it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Change {
    None,
    Selection,
    Text,
}

impl Snapshot {
    /// Reads the state through the handler.
    pub(super) fn read(handler: &mut PlatformInputHandler) -> Self {
        Self {
            handler: true,
            selection: handler
                .selected_text_range(false)
                .map(|selection| selection.range),
            marked: handler.marked_text_range(),
            len: handler.text_length_utf16(),
        }
    }

    /// Text changes (including a different handler) subsume selection
    /// changes: UIKit re-reads the selection after `textDidChange:`.
    pub(super) fn change_since(&self, old: &Self) -> Change {
        if self.handler != old.handler || self.marked != old.marked || self.len != old.len {
            Change::Text
        } else if self.selection != old.selection {
            Change::Selection
        } else {
            Change::None
        }
    }

    /// The end of the document. Handlers that do not report a length (a
    /// terminal, whose document is only its composition) end at the furthest
    /// offset they did report.
    pub(super) fn document_len(&self) -> usize {
        self.len.unwrap_or_else(|| {
            let selection = self.selection.as_ref().map_or(0, |range| range.end);
            let marked = self.marked.as_ref().map_or(0, |range| range.end);
            selection.max(marked)
        })
    }
}
