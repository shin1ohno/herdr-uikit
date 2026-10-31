//! The focused element's text state, as far as UIKit's text system sees it.
//! GPUI registers the input handler again on every frame and around every
//! event, so a registration says nothing about whether the text changed.
//! Comparing this state frame to frame does, and the text input view hears
//! only of frames where it may have: telling UIKit of a change on a cursor
//! blink or a frame of terminal output can reset an IME composition.

use std::ops::Range;

use gpui::PlatformInputHandler;

/// How much text before the selection is compared, in UTF-16 units: enough
/// to notice an edit there, or another field taking focus with the same
/// selection, without copying a document every frame.
const CONTEXT: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TextState {
    /// The text just before the selection.
    pub(super) context: Option<String>,
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
        let context = selection.as_ref().and_then(|(range, _)| {
            let start = range.start.min(range.end);
            handler.text_for_range(start.saturating_sub(CONTEXT)..start, &mut None)
        });
        Self {
            context,
            marked,
            selection,
        }
    }
}

/// Whether the text input view must re-read the input handler after a frame:
/// an element took text input, or its state differs from the last frame's.
/// A content change that leaves the selection, the marked range, and the
/// text before the selection as they were goes unreported.
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
