//! What UIKit reads from the text input: text, caret movement, and geometry,
//! answered from GPUI's input handler.
//!
//! Directions here are logical: left steps towards the start of the text and
//! right towards its end. The view maps the visual directions UIKit passes
//! under the app's layout direction (`LayoutDirection::logical`) before
//! calling in, so in right to left text a visual left moves forward.

use std::ops::Range;

use gpui::{Bounds, Pixels};
use objc2::DefinedClass;
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::NSInteger;
use objc2_ui_kit::UITextLayoutDirection;

use super::geometry;
use super::utf16;
use super::view_class::GPUITextInputView;

impl GPUITextInputView {
    pub(super) fn text_in_range(&self, range: Range<usize>) -> Option<String> {
        self.with_handler(|handler| {
            let mut adjusted = None;
            handler.text_for_range(range, &mut adjusted)
        })
        .flatten()
    }

    /// `positionFromPosition:offset:`: `None` outside the document.
    pub(super) fn offset_from(&self, from: usize, offset: NSInteger) -> Option<usize> {
        utf16::offset_by(from, offset, self.read_snapshot().document_len())
    }

    /// The one-unit range beside `offset` in `direction`, empty at either
    /// end of the document.
    pub(super) fn extended_range(
        &self,
        offset: usize,
        direction: UITextLayoutDirection,
    ) -> Range<usize> {
        let step = match direction {
            UITextLayoutDirection::Left | UITextLayoutDirection::Up => -1,
            _ => 1,
        };
        let len = self.read_snapshot().document_len();
        let other = utf16::offset_by(offset, step, len).unwrap_or(offset);
        utf16::between(offset, other)
    }

    /// `positionFromPosition:inDirection:offset:`. Left and right step
    /// through the text; up and down go through the layout, which only an
    /// element that maps points to characters can answer.
    pub(super) fn position_in_direction(
        &self,
        from: usize,
        direction: UITextLayoutDirection,
        offset: NSInteger,
    ) -> Option<usize> {
        let len = self.read_snapshot().document_len();
        match direction {
            UITextLayoutDirection::Left => utf16::offset_by(from, offset.checked_neg()?, len),
            UITextLayoutDirection::Right => utf16::offset_by(from, offset, len),
            UITextLayoutDirection::Up => self.offset_on_line(from, offset.checked_neg()?),
            UITextLayoutDirection::Down => self.offset_on_line(from, offset),
            _ => None,
        }
    }

    fn offset_on_line(&self, from: usize, lines: isize) -> Option<usize> {
        self.with_handler(|handler| {
            let caret = handler.bounds_for_range(from..from)?;
            handler.character_index_for_point(geometry::line_point(caret, lines)?)
        })
        .flatten()
    }

    /// `firstRectForRange:`, where the IME places its candidate window.
    pub(super) fn first_rect(&self, range: Range<usize>) -> CGRect {
        let bounds = self
            .with_handler(|handler| handler.bounds_for_range(range))
            .flatten();
        self.rect_or_caret(bounds, geometry::rect)
    }

    /// `caretRectForPosition:`, which the keyboard and the caps lock and
    /// dictation indicators follow.
    pub(super) fn caret_rect(&self, offset: usize) -> CGRect {
        let bounds = self
            .with_handler(|handler| handler.bounds_for_range(offset..offset))
            .flatten();
        self.rect_or_caret(bounds, geometry::caret)
    }

    /// Falls back to the caret GPUI last reported when the handler cannot
    /// place the range, so the IME stays next to the text.
    fn rect_or_caret(
        &self,
        bounds: Option<Bounds<Pixels>>,
        to_rect: fn(Bounds<Pixels>, f32) -> CGRect,
    ) -> CGRect {
        let scale_factor = self.ivars().target.scale_factor();
        match bounds {
            Some(bounds) => to_rect(bounds, scale_factor),
            None => self
                .ivars()
                .caret
                .get()
                .map_or(CGRect::ZERO, |caret| geometry::caret(caret, scale_factor)),
        }
    }

    /// The character nearest a point in the `textInputView`.
    pub(super) fn closest_offset(&self, point: CGPoint) -> Option<usize> {
        let point = geometry::point(point);
        self.with_handler(|handler| handler.character_index_for_point(point))
            .flatten()
    }
}
