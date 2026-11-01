use super::*;

#[test]
fn a_caret_at_zero_samples_following_text() {
    assert_eq!(context_range(&(0..0), Some(10)), 0..10);
}

#[test]
fn context_stays_bounded_on_both_sides_of_the_caret() {
    let caret = 100;
    let range = context_range(&(caret..caret), Some(1_000));
    assert_eq!(range, caret - CONTEXT..caret + CONTEXT);
}

#[test]
fn unknown_documents_sample_only_before_the_caret() {
    assert_eq!(context_range(&(5..5), None), 0..5);
}

#[test]
fn context_never_overflows_or_leaves_a_known_document() {
    assert_eq!(context_range(&(9..9), Some(10)), 0..10);
    assert_eq!(context_range(&(20..20), Some(10)), 0..10);
    assert_eq!(
        context_range(&(usize::MAX..usize::MAX), Some(usize::MAX)),
        usize::MAX - CONTEXT..usize::MAX
    );
}
