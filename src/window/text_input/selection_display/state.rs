//! When the selection display interaction is switched on or off and asked to
//! re-read the caret, without UIKit, so host tests import this file by path
//! (`tests/selection_display_state.rs`).
//!
//! The interaction only follows the text view: on while it is first
//! responder, off after it resigned. A caret that did not move asks UIKit for
//! nothing, and nothing here re-shows the system's input mode indicator:
//! UIKit shows it on its own when the view becomes first responder or the
//! input source changes (`docs/input-mode-indicator.md`).

/// What the state machine asks of UIKit's selection display interaction.
pub(crate) trait Sink {
    /// `setActivated:`.
    fn set_activated(&self, on: bool);
    /// `setNeedsSelectionUpdate`, which re-reads `selectedTextRange` and
    /// `caretRectForPosition:` and tells the input delegate nothing.
    fn set_needs_update(&self);
}

/// The caret rectangle as the bit patterns of its x, y, width and height,
/// so equal rectangles compare equal without floating-point comparison.
pub(crate) type CaretKey = [u32; 4];

/// The caret key of a rectangle given as x, y, width and height.
pub(crate) fn caret_key(x: f32, y: f32, width: f32, height: f32) -> CaretKey {
    [x.to_bits(), y.to_bits(), width.to_bits(), height.to_bits()]
}

#[derive(Debug, Default)]
pub(crate) struct DisplayState {
    activated: bool,
    caret: Option<CaretKey>,
}

impl DisplayState {
    /// Whether the interaction was last switched on.
    #[allow(
        dead_code,
        reason = "the host tests read it; the SDK's QA reads UIKit's isActivated"
    )]
    pub(crate) fn activated(&self) -> bool {
        self.activated
    }

    /// The text view became first responder, or was asked for text while it
    /// already was. Switching on an active interaction leaves it on; it never
    /// goes through off, so a focused view is not re-shown (issue #139,
    /// decision 2).
    pub(crate) fn acquired(&mut self, sink: &impl Sink) {
        self.activated = true;
        sink.set_activated(true);
        sink.set_needs_update();
    }

    /// The text view resigned first responder.
    pub(crate) fn resigned(&mut self, sink: &impl Sink) {
        if self.activated {
            self.activated = false;
            sink.set_activated(false);
        }
    }

    /// GPUI reported the caret. Only a moved caret asks UIKit to re-read it.
    /// An inactive interaction records the move too; switching it on
    /// re-reads the caret anyway.
    pub(crate) fn caret(&mut self, sink: &impl Sink, key: CaretKey) {
        if Some(key) != self.caret {
            sink.set_needs_update();
        }
        self.caret = Some(key);
    }
}
