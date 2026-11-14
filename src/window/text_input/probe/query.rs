//! Native observations on one retained endpoint; no model or focus repair.

use std::ops::Range;

use objc2::{msg_send, rc::Retained};
use objc2_foundation::NSInteger;
use objc2_ui_kit::{UITextInput, UITextPosition, UITextRange};

use super::NativeClient;

/// The captured native identity and the value actually returned by UIKit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeQuery<T> {
    pub client: u64,
    pub value: T,
}

/// Numeric failures are distinct from an observed native nil.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeQueryError {
    IntegerOutOfRange,
    NegativeOffset,
    ReversedRange,
}

fn signed(value: usize) -> Result<NSInteger, NativeQueryError> {
    NSInteger::try_from(value).map_err(|_| NativeQueryError::IntegerOutOfRange)
}

fn unsigned(value: NSInteger) -> Result<usize, NativeQueryError> {
    if value < 0 {
        return Err(NativeQueryError::NegativeOffset);
    }
    usize::try_from(value).map_err(|_| NativeQueryError::IntegerOutOfRange)
}

impl NativeClient {
    fn observe<T>(
        &self,
        read: impl FnOnce() -> Result<T, NativeQueryError>,
    ) -> Result<NativeQuery<T>, NativeQueryError> {
        let client = self.identity();
        Ok(NativeQuery {
            client,
            value: read()?,
        })
    }

    fn beginning(&self) -> Option<Retained<UITextPosition>> {
        // SAFETY: the standard selector returns a retained-compatible object.
        // Accept nil even for UIKit's declared nonnull getter, without inventing 0.
        unsafe { msg_send![&*self.0, beginningOfDocument] }
    }

    fn offset(
        &self,
        start: &UITextPosition,
        end: &UITextPosition,
    ) -> Result<usize, NativeQueryError> {
        unsigned(self.0.offsetFromPosition_toPosition(start, end))
    }

    /// Read actual selection endpoints without setting selection, even on an
    /// old retired object. Nil positions remain nil; negative/reversed offsets fail.
    pub fn selected_range_utf16(
        &self,
    ) -> Result<NativeQuery<Option<Range<usize>>>, NativeQueryError> {
        self.observe(|| {
            let Some(range) = self.0.selectedTextRange() else {
                return Ok(None);
            };
            let Some(begin) = self.beginning() else {
                return Ok(None);
            };
            // SAFETY: UITextRange's actual start/end selectors return positions.
            // Optional retained results preserve an unexpected native nil.
            let (start, end): (
                Option<Retained<UITextPosition>>,
                Option<Retained<UITextPosition>>,
            ) = unsafe { (msg_send![&*range, start], msg_send![&*range, end]) };
            let (Some(start), Some(end)) = (start, end) else {
                return Ok(None);
            };
            let start = self.offset(&begin, &start)?;
            let end = self.offset(&begin, &end)?;
            if start > end {
                return Err(NativeQueryError::ReversedRange);
            }
            Ok(Some(start..end))
        })
    }

    /// Measure beginning to end through this object's native offset selector.
    /// A real zero is Some(0), including when the captured client is retired.
    pub fn document_length_utf16(&self) -> Result<NativeQuery<Option<usize>>, NativeQueryError> {
        self.observe(|| {
            let Some(begin) = self.beginning() else {
                return Ok(None);
            };
            // SAFETY: standard position getter on the same main-thread receiver.
            let end: Option<Retained<UITextPosition>> =
                unsafe { msg_send![&*self.0, endOfDocument] };
            end.map(|end| self.offset(&begin, &end)).transpose()
        })
    }

    /// Ask UIKit twice for positions; do not synthesize from + delta or clamp.
    pub fn position_offset_utf16(
        &self,
        from: usize,
        delta: isize,
    ) -> Result<NativeQuery<Option<usize>>, NativeQueryError> {
        self.observe(|| {
            let from = signed(from)?;
            // NSInteger is the native pointer-width isize alias. This typed
            // assignment is lossless on every target, including signed extremes.
            let delta: NSInteger = delta;
            let Some(begin) = self.beginning() else {
                return Ok(None);
            };
            let Some(from) = self.0.positionFromPosition_offset(&begin, from) else {
                return Ok(None);
            };
            self.0
                .positionFromPosition_offset(&from, delta)
                .map(|position| self.offset(&begin, &position))
                .transpose()
        })
    }

    /// SETUP ONLY: set once, then independently observe equality. Never use
    /// this to verify or repair the selection after an operation under test.
    pub fn select_utf16(&self, range: Range<usize>) -> Result<NativeQuery<bool>, NativeQueryError> {
        self.observe(|| {
            if range.start > range.end {
                return Err(NativeQueryError::ReversedRange);
            }
            let (start, end) = (signed(range.start)?, signed(range.end)?);
            let Some(begin) = self.beginning() else {
                return Ok(false);
            };
            let start = self.0.positionFromPosition_offset(&begin, start);
            let end = self.0.positionFromPosition_offset(&begin, end);
            let (Some(start), Some(end)) = (start, end) else {
                return Ok(false);
            };
            let native: Option<Retained<UITextRange>> =
                self.0.textRangeFromPosition_toPosition(&start, &end);
            let Some(native) = native else {
                return Ok(false);
            };
            self.0.setSelectedTextRange(Some(&native));
            Ok(self.selected_range_utf16()?.value == Some(range))
        })
    }
}
