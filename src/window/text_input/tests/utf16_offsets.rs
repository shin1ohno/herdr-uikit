use super::*;

#[test]
fn length_counts_code_units() {
    assert_eq!(utf16::len(""), 0);
    assert_eq!(utf16::len("abc"), 3);
    assert_eq!(utf16::len("かな"), 2);
    assert_eq!(utf16::len("😀"), 2, "astral characters are surrogate pairs");
}

#[test]
fn ns_ranges_map_to_offsets() {
    assert_eq!(utf16::range_from_ns(2, 3), Some(2..5));
    assert_eq!(utf16::range_from_ns(0, 0), Some(0..0));
    let not_found = usize::try_from(isize::MAX).unwrap_or(usize::MAX);
    assert_eq!(utf16::range_from_ns(not_found, 0), None);
    assert_eq!(utf16::range_from_ns(usize::MAX - 1, 5), None);
}

#[test]
fn clamp_orders_and_bounds_ranges() {
    assert_eq!(utf16::clamp(1..3, 5), 1..3);
    #[allow(
        clippy::reversed_empty_ranges,
        reason = "UIKit can send reversed ranges"
    )]
    let reversed = 4..2;
    assert_eq!(utf16::clamp(reversed, 5), 2..4);
    assert_eq!(utf16::clamp(3..9, 5), 3..5);
    assert_eq!(utf16::clamp(7..9, 5), 5..5);
}

#[test]
fn positions_stay_inside_the_document() {
    assert_eq!(utf16::offset_by(2, 1, 5), Some(3));
    assert_eq!(utf16::offset_by(2, -2, 5), Some(0));
    assert_eq!(utf16::offset_by(2, -3, 5), None);
    assert_eq!(utf16::offset_by(4, 1, 5), Some(5));
    assert_eq!(utf16::offset_by(5, 1, 5), None);
}

#[test]
fn distances_are_signed() {
    assert_eq!(utf16::distance(2, 5), 3);
    assert_eq!(utf16::distance(5, 2), -3);
    assert_eq!(utf16::distance(4, 4), 0);
    assert_eq!(utf16::between(5, 2), 2..5);
}

#[test]
fn previous_character_spans_surrogate_pairs() {
    let emoji: Vec<u16> = "a😀".encode_utf16().collect();
    assert_eq!(utf16::previous_char_len(&emoji), 2);
    let plain: Vec<u16> = "ab".encode_utf16().collect();
    assert_eq!(utf16::previous_char_len(&plain), 1);
    let kana: Vec<u16> = "か".encode_utf16().collect();
    assert_eq!(utf16::previous_char_len(&kana), 1);
    assert_eq!(utf16::previous_char_len(&[]), 0);
    // A lone low surrogate is one unit, not half of a pair.
    assert_eq!(utf16::previous_char_len(&[0x61, 0xdc00]), 1);
}

#[test]
fn removing_units_keeps_whole_characters() {
    assert_eq!(utf16::remove("かなに", 1..2).as_deref(), Some("かに"));
    assert_eq!(utf16::remove("a😀b", 1..3).as_deref(), Some("ab"));
    assert_eq!(utf16::remove("abc", 0..3).as_deref(), Some(""));
    assert_eq!(utf16::remove("a😀b", 1..2), None, "splits a surrogate pair");
    assert_eq!(utf16::remove("abc", 2..5), None, "out of bounds");
}

#[test]
fn character_before_the_caret_spans_surrogate_pairs() {
    assert_eq!(utf16::char_before(3, 1..3, "😀"), 1..3);
    assert_eq!(utf16::char_before(3, 1..3, "ab"), 2..3);
    assert_eq!(utf16::char_before(1, 0..1, "a"), 0..1);
    // No text before the caret still deletes one unit.
    assert_eq!(utf16::char_before(2, 2..2, ""), 1..2);
}

#[test]
fn character_before_the_caret_guesses_one_unit_for_trimmed_ranges() {
    assert_eq!(utf16::char_before(4, 2..3, "😀"), 3..4);
}

#[test]
fn character_before_the_caret_stays_inside_the_document() {
    // A handler asked for 0..1 returned a whole pair without adjusting the
    // range; the deletion must not reach before offset 0.
    assert_eq!(utf16::char_before(1, 0..1, "😀"), 0..1);
    assert_eq!(utf16::char_before(0, 0..0, "😀"), 0..0);
}
