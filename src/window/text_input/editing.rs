//! Edits UIKit asks for: typing, deletion, IME composition (marked text),
//! dictation, and selection, applied through GPUI's input handler.
//!
//! UIKit made each of these changes, so none is reported back to its input
//! delegate; the cached state is refreshed instead (see `sync`).

use std::ops::Range;

use gpui::{KeyDownEvent, KeyUpEvent, Keystroke, PlatformInput, PlatformInputHandler};
use objc2::DefinedClass;

use super::keymap;
use super::utf16;
use super::view_class::GPUITextInputView;

impl GPUITextInputView {
    /// `insertText:` from the software keyboard, a hardware key GPUI did not
    /// handle, or an IME committing its composition.
    pub(super) fn insert_text(&self, text: &str) {
        if keymap::is_lone_control(text) {
            self.insert_control(text);
            return;
        }
        self.with_handler(|handler| handler.replace_text_in_range(None, text));
        self.refresh_snapshot();
    }

    /// Dictation's text is always text, even a single line break.
    pub(super) fn insert_dictation(&self, text: &str) {
        self.with_handler(|handler| handler.replace_text_in_range(None, text));
        self.refresh_snapshot();
    }

    /// A lone control character is a key: the software keyboard's return
    /// key reaches a terminal as `enter`. A hardware key that UIKit typed
    /// after GPUI declined it is not dispatched a second time, and an
    /// unhandled key types nothing, as on macOS.
    fn insert_control(&self, text: &str) {
        let composing = self
            .with_handler(|handler| handler.marked_text_range())
            .flatten()
            .is_some();
        if composing {
            self.with_handler(PlatformInputHandler::unmark_text);
            self.refresh_snapshot();
        }
        let Some(keystroke) = keymap::control_keystroke(text) else {
            return;
        };
        if self.ivars().keys.borrow().is_forwarded(&keystroke.key) {
            return;
        }
        self.tap_key(keystroke);
    }

    /// `deleteBackward`. Marked text and a selection are the handler's to
    /// delete; otherwise backspace goes to GPUI as a key, so a terminal sends
    /// it to the program, and only an unhandled one deletes in the handler.
    pub(super) fn delete_backward(&self) {
        // A hardware backspace GPUI already declined.
        if self.ivars().keys.borrow().is_forwarded("backspace") {
            self.delete_in_handler();
            return;
        }
        let snapshot = self.read_snapshot();
        let selected = snapshot
            .selection
            .as_ref()
            .is_some_and(|(selection, _)| !selection.is_empty());
        if snapshot.marked.is_some() || selected || !self.tap_key(keymap::backspace()) {
            self.delete_in_handler();
        }
    }

    /// Deletes the selection, or the character before the caret. Inside
    /// marked text the composition continues with that character removed.
    fn delete_in_handler(&self) {
        self.with_handler(|handler| {
            let marked = handler.marked_text_range();
            let Some(selection) = handler.selected_text_range(false) else {
                return;
            };
            if !selection.range.is_empty() {
                handler.replace_text_in_range(Some(selection.range), "");
                return;
            }
            let caret = selection.range.end;
            let Some(deleted) = previous_char(handler, caret) else {
                return;
            };
            match marked {
                Some(marked) if marked.start <= deleted.start && deleted.end <= marked.end => {
                    delete_in_marked(handler, marked, deleted);
                }
                _ => handler.replace_text_in_range(Some(deleted), ""),
            }
        });
        self.refresh_snapshot();
    }

    /// `setMarkedText:selectedRange:`: the IME's composition replaces the
    /// marked text, or the selection when there is none. `selected` is
    /// relative to the new marked text, as GPUI expects.
    pub(super) fn set_marked_text(&self, text: &str, selected: Option<Range<usize>>) {
        let selected = selected.map(|range| utf16::clamp(range, utf16::len(text)));
        self.with_handler(|handler| {
            if !text.is_empty() {
                handler.replace_and_mark_text_in_range(None, text, selected);
            } else if let Some(marked) = handler.marked_text_range() {
                // An emptied composition removes the marked text only; with
                // no composition there is nothing to remove, and the
                // selection must survive.
                handler.replace_text_in_range(Some(marked), "");
            }
        });
        self.refresh_snapshot();
    }

    pub(super) fn unmark_text(&self) {
        self.with_handler(PlatformInputHandler::unmark_text);
        self.refresh_snapshot();
    }

    /// `replaceRange:withText:`, used by autocorrection, text replacement,
    /// and the keyboard's suggestions.
    pub(super) fn replace_range(&self, range: Range<usize>, text: &str) {
        self.with_handler(|handler| handler.replace_text_in_range(Some(range), text));
        self.refresh_snapshot();
    }

    /// `setSelectedTextRange:`, from UIKit's own caret movement.
    pub(super) fn select_range(&self, range: Range<usize>) {
        self.with_handler(|handler| handler.set_selected_text_range(range));
        self.refresh_snapshot();
    }

    /// Sends a key press and release with no physical key behind it, and
    /// returns whether GPUI handled the press.
    pub(super) fn tap_key(&self, keystroke: Keystroke) -> bool {
        let target = &self.ivars().target;
        let handled = target.dispatch(PlatformInput::KeyDown(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        }));
        target.dispatch(PlatformInput::KeyUp(KeyUpEvent { keystroke }));
        handled
    }
}

/// The range of the character before `caret`, one or two UTF-16 units.
fn previous_char(handler: &mut PlatformInputHandler, caret: usize) -> Option<Range<usize>> {
    if caret == 0 {
        return None;
    }
    let requested = caret.saturating_sub(2)..caret;
    let mut adjusted = None;
    let text = handler.text_for_range(requested.clone(), &mut adjusted)?;
    Some(utf16::char_before(
        caret,
        adjusted.unwrap_or(requested),
        &text,
    ))
}

/// Removes `deleted` from the marked text and keeps composing the rest, with
/// the caret where the character was.
fn delete_in_marked(
    handler: &mut PlatformInputHandler,
    marked: Range<usize>,
    deleted: Range<usize>,
) {
    let mut adjusted = None;
    let Some(text) = handler.text_for_range(marked.clone(), &mut adjusted) else {
        return;
    };
    let relative = deleted.start - marked.start..deleted.end - marked.start;
    let caret = relative.start;
    match utf16::remove(&text, relative) {
        Some(remaining) if !remaining.is_empty() => {
            handler.replace_and_mark_text_in_range(Some(marked), &remaining, Some(caret..caret));
        }
        Some(_) => handler.replace_text_in_range(Some(marked), ""),
        None => handler.replace_text_in_range(Some(deleted), ""),
    }
}
