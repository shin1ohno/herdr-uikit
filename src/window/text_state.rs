//! The focused element's text state, as far as UIKit's text system sees it.
//! GPUI registers the input handler again on every frame and around every
//! event, so a registration says nothing about whether the text changed.
//! Comparing this state frame to frame does, and the text input view hears
//! only of frames where it may have: telling UIKit of a change on a cursor
//! blink or a frame of terminal output can reset an IME composition.

use std::ops::Range;

use gpui::PlatformInputHandler;

/// How much text on either side of the selection's start is compared, in
/// UTF-16 units: enough to notice another field taking focus even with its
/// caret at zero, without copying the whole document every frame.
const CONTEXT: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TextState {
    /// Bounded text around the selection's start.
    pub(super) context: Option<String>,
    /// Whether GPUI has a text input handler for this state.
    pub(super) handler: bool,
    /// The document length, when the handler knows it.
    pub(super) len: Option<usize>,
    /// The marked (composing) range, in UTF-16 units.
    pub(super) marked: Option<Range<usize>>,
    /// The selected range in UTF-16 units, and whether it is reversed.
    pub(super) selection: Option<(Range<usize>, bool)>,
}

impl TextState {
    /// Reads the state from the focused element's input handler.
    pub(super) fn sample(handler: &mut PlatformInputHandler) -> Self {
        let selection = handler
            .selected_text_range(false)
            .map(|selection| (selection.range, selection.reversed));
        let marked = handler.marked_text_range();
        let len = handler.text_length_utf16();
        let context = selection
            .as_ref()
            .and_then(|(range, _)| handler.text_for_range(context_range(range, len), &mut None));
        Self {
            context,
            handler: true,
            len,
            marked,
            selection,
        }
    }
}

/// Whether the text input view must re-read the input handler after a frame:
/// an element took text input, or its state differs from the last frame's.
/// A content change that leaves the selection, marked range, nearby text,
/// and document length as they were goes unreported.
///
/// Nothing is reported while text is marked. Only the IME marks text, so it
/// made every change since, and telling UIKit of its own edit can reset the
/// composition. The frame that ends the composition reports.
pub(super) fn changed(previous: Option<&TextState>, current: Option<&TextState>) -> bool {
    let Some(current) = current else {
        return false;
    };
    current.marked.is_none() && previous != Some(current)
}

/// An unknown document length only permits reading before the caret, as a
/// terminal may expose just its current composition. Known documents also
/// include following text, bounded on both sides and at the document's end.
fn context_range(selection: &Range<usize>, len: Option<usize>) -> Range<usize> {
    let caret = selection.start.min(selection.end);
    let caret = len.map_or(caret, |len| caret.min(len));
    let end = len.map_or(caret, |len| caret.saturating_add(CONTEXT).min(len));
    caret.saturating_sub(CONTEXT)..end
}

#[cfg(test)]
mod tests;
