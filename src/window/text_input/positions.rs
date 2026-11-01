//! `UITextPosition` and `UITextRange` subclasses carrying UTF-16 offsets into
//! the focused element's text, the units GPUI's input handler counts in.
//!
//! UIKit treats both as opaque: it only hands them back to the text input
//! that made them, which compares and measures them through `UITextInput`.
//! Ranges are value objects, so they copy and compare by offsets.

#![allow(
    unsafe_code,
    reason = "Objective-C subclasses of UIKit's text position and range classes"
)]

use std::ops::Range;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, NSZone};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::NSCopying;
use objc2_ui_kit::{UITextLayoutDirection, UITextPosition, UITextRange};

define_class!(
    // SAFETY: UITextPosition adds nothing to NSObject's subclassing rules:
    // `init` is its designated initializer, the class does not implement
    // `Drop`, and the overrides keep NSObject's signatures.
    #[unsafe(super(UITextPosition, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = usize]
    pub(super) struct TextPosition;

    unsafe impl NSObjectProtocol for TextPosition {
        #[unsafe(method(isEqual:))]
        fn __is_equal(&self, other: Option<&AnyObject>) -> bool {
            other
                .and_then(|other| other.downcast_ref::<Self>())
                .is_some_and(|other| other.offset() == self.offset())
        }

        #[unsafe(method(hash))]
        fn __hash(&self) -> usize {
            self.offset()
        }
    }
);

impl TextPosition {
    pub(super) fn new(mtm: MainThreadMarker, offset: usize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(offset);
        // SAFETY: `init` is the designated initializer of UITextPosition, and
        // the ivars were set before it runs.
        unsafe { msg_send![super(this), init] }
    }

    /// `new` as the superclass UIKit's methods return.
    pub(super) fn position(mtm: MainThreadMarker, offset: usize) -> Retained<UITextPosition> {
        Retained::into_super(Self::new(mtm, offset))
    }

    pub(super) fn offset(&self) -> usize {
        *self.ivars()
    }
}

/// The offset of a position this view made, or `None` for any other.
pub(super) fn offset(position: &UITextPosition) -> Option<usize> {
    position
        .downcast_ref::<TextPosition>()
        .map(TextPosition::offset)
}

#[derive(Clone, Debug)]
pub(super) struct RangeIvars {
    start: usize,
    end: usize,
}

define_class!(
    // SAFETY: UITextRange is abstract with `start`, `end`, and `isEmpty` for
    // subclasses to override, which this class does with the superclass
    // signatures; `init` is its designated initializer, and the class does
    // not implement `Drop`.
    #[unsafe(super(UITextRange, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RangeIvars]
    pub(super) struct TextRange;

    impl TextRange {
        #[unsafe(method_id(start))]
        fn __start(&self) -> Retained<UITextPosition> {
            TextPosition::position(self.mtm(), self.ivars().start)
        }

        #[unsafe(method_id(end))]
        fn __end(&self) -> Retained<UITextPosition> {
            TextPosition::position(self.mtm(), self.ivars().end)
        }

        #[unsafe(method(isEmpty))]
        fn __is_empty(&self) -> bool {
            self.ivars().start == self.ivars().end
        }
    }

    unsafe impl NSObjectProtocol for TextRange {
        #[unsafe(method(isEqual:))]
        fn __is_equal(&self, other: Option<&AnyObject>) -> bool {
            other
                .and_then(|other| other.downcast_ref::<Self>())
                .is_some_and(|other| other.range() == self.range())
        }

        #[unsafe(method(hash))]
        fn __hash(&self) -> usize {
            self.ivars().start.rotate_left(16) ^ self.ivars().end
        }
    }

    // UIKit copies the ranges it keeps, such as a selection it is about to
    // set, so a range must answer `copy`.
    unsafe impl NSCopying for TextRange {
        #[unsafe(method_id(copyWithZone:))]
        fn __copy_with_zone(&self, _zone: *mut NSZone) -> Retained<Self> {
            Self::new(self.mtm(), self.range())
        }
    }
);

impl TextRange {
    /// A range over `range`, ordered so `start <= end` as UIKit requires.
    pub(super) fn new(mtm: MainThreadMarker, range: Range<usize>) -> Retained<Self> {
        let (start, end) = (range.start.min(range.end), range.start.max(range.end));
        let this = Self::alloc(mtm).set_ivars(RangeIvars { start, end });
        // SAFETY: `init` is the designated initializer of UITextRange, and the
        // ivars were set before it runs.
        unsafe { msg_send![super(this), init] }
    }

    /// `new` as the superclass UIKit's methods return.
    pub(super) fn text_range(mtm: MainThreadMarker, range: Range<usize>) -> Retained<UITextRange> {
        Retained::into_super(Self::new(mtm, range))
    }

    pub(super) fn range(&self) -> Range<usize> {
        self.ivars().start..self.ivars().end
    }
}

/// The end of `range` furthest in `direction`: the start leftward or upward,
/// the end otherwise. `direction` is logical, left meaning towards the start;
/// the view maps UIKit's visual directions first
/// (`LayoutDirection::logical`), so right to left text swaps the ends.
pub(super) fn farthest(range: Range<usize>, direction: UITextLayoutDirection) -> usize {
    match direction {
        UITextLayoutDirection::Left | UITextLayoutDirection::Up => range.start,
        _ => range.end,
    }
}

/// The offsets of a range, made by this view or from two of its positions.
pub(super) fn range(range: &UITextRange) -> Option<Range<usize>> {
    if let Some(range) = range.downcast_ref::<TextRange>() {
        return Some(range.range());
    }
    let start = offset(&range.start())?;
    let end = offset(&range.end())?;
    Some(start.min(end)..start.max(end))
}
