//! UTF-16 offset arithmetic. UIKit and GPUI's input handler both count text
//! in UTF-16 code units, so positions are plain offsets; these helpers keep
//! them inside the document and away from the middle of a surrogate pair.

use std::ops::Range;

/// The length of `text` in UTF-16 code units.
pub(super) fn len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// A range from an `NSRange`'s location and length, or `None` for UIKit's
/// "no range" (`NSNotFound`) or one that overflows.
pub(super) fn range_from_ns(location: usize, length: usize) -> Option<Range<usize>> {
    let not_found = usize::try_from(isize::MAX).ok()?;
    if location >= not_found {
        return None;
    }
    Some(location..location.checked_add(length)?)
}

/// `range` ordered and clamped to `0..=len`.
pub(super) fn clamp(range: Range<usize>, len: usize) -> Range<usize> {
    let start = range.start.min(range.end).min(len);
    let end = range.start.max(range.end).min(len);
    start..end
}

/// The range between two offsets in either order.
pub(super) fn between(a: usize, b: usize) -> Range<usize> {
    a.min(b)..a.max(b)
}

/// `offset` moved by `delta`, or `None` when that leaves `0..=len`, which is
/// how `positionFromPosition:offset:` reports a position outside the text.
pub(super) fn offset_by(offset: usize, delta: isize, len: usize) -> Option<usize> {
    offset
        .checked_add_signed(delta)
        .filter(|moved| *moved <= len)
}

/// The signed distance from `from` to `to`.
pub(super) fn distance(from: usize, to: usize) -> isize {
    if to >= from {
        isize::try_from(to - from).unwrap_or(isize::MAX)
    } else {
        isize::try_from(from - to).map_or(isize::MIN, |distance| -distance)
    }
}

/// How many code units the character ending `units` takes: two for a
/// surrogate pair, one otherwise, zero for no text.
pub(super) fn previous_char_len(units: &[u16]) -> usize {
    match units {
        [] => 0,
        [.., high, low] if is_high_surrogate(*high) && is_low_surrogate(*low) => 2,
        _ => 1,
    }
}

/// The range of the character before `caret`, from the `text` a handler
/// returned for `returned` when asked for the two units before the caret.
/// A handler that trimmed the range differently gives units that do not end
/// at the caret; one unit is then the safe guess. One that returned more
/// text than it was asked for without saying so cannot move the start before
/// the document's.
pub(super) fn char_before(caret: usize, returned: Range<usize>, text: &str) -> Range<usize> {
    let len = if returned.end == caret {
        let units: Vec<u16> = text.encode_utf16().collect();
        previous_char_len(&units).max(1)
    } else {
        1
    };
    caret.saturating_sub(len)..caret
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xd800..=0xdbff).contains(&unit)
}

fn is_low_surrogate(unit: u16) -> bool {
    (0xdc00..=0xdfff).contains(&unit)
}

/// `text` without the code units in `range`, or `None` when the range is out
/// of bounds or would split a surrogate pair.
pub(super) fn remove(text: &str, range: Range<usize>) -> Option<String> {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    if range.start > range.end || range.end > units.len() {
        return None;
    }
    units.drain(range);
    String::from_utf16(&units).ok()
}
