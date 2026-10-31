use crate::text_system::layout::StringIndexConverter;

fn utf8_ix(text: &str, utf16_ix: usize) -> usize {
    let mut converter = StringIndexConverter::new(text);
    converter.advance_to_utf16_ix(utf16_ix);
    converter.utf8_ix
}

#[test]
fn ascii_offsets_are_equal() {
    assert_eq!(utf8_ix("hello", 0), 0);
    assert_eq!(utf8_ix("hello", 3), 3);
}

#[test]
fn multibyte_and_surrogate_pairs_map_to_their_utf8_start() {
    // UTF-16: a=0, é=1, 😀=2..4, b=4. UTF-8: a=0, é=1..3, 😀=3..7, b=7.
    let text = "aé😀b";
    assert_eq!(utf8_ix(text, 1), 1);
    assert_eq!(utf8_ix(text, 2), 3);
    assert_eq!(utf8_ix(text, 4), 7);
}

#[test]
fn offsets_past_the_end_clamp_to_the_length() {
    assert_eq!(utf8_ix("ab", 9), 2);
    assert_eq!(utf8_ix("", 1), 0);
}

#[test]
fn seeking_forward_continues_from_the_last_position() {
    let text = "日本語abc";
    let mut converter = StringIndexConverter::new(text);
    converter.advance_to_utf16_ix(1);
    assert_eq!((converter.utf8_ix, converter.utf16_ix), (3, 1));
    converter.advance_to_utf16_ix(4);
    assert_eq!((converter.utf8_ix, converter.utf16_ix), (10, 4));
}
