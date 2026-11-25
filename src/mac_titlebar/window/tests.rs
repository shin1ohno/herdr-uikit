use super::*;

/// The scene window's mask on macOS 26.7.3: titled, closable,
/// miniaturizable, resizable (task b903b39987a54c31, `diag.jsonl`).
const SCENE_WINDOW: usize = 0xf;

#[test]
fn a_titled_window_gains_a_full_size_content_view() {
    assert_eq!(extended_mask(SCENE_WINDOW), Some(0x800f));
}

#[test]
fn a_window_without_a_title_bar_is_left_alone() {
    // The process's other window on that Mac: untitled, already full size.
    assert_eq!(extended_mask(0x8000), None);
    assert_eq!(extended_mask(0), None);
}

#[test]
fn an_extended_window_is_not_extended_again() {
    assert_eq!(extended_mask(0x800f), None);
}

#[test]
fn an_extended_window_ends_the_attempts() {
    assert_eq!(next(&Attempt::Extended(0.77), 0), Next::Done);
}

#[test]
fn a_missing_window_is_tried_again_for_five_seconds() {
    assert_eq!(next(&Attempt::NotYet, 0), Next::Retry);
    assert_eq!(next(&Attempt::NotYet, ATTEMPTS - 2), Next::Retry);
    assert_eq!(next(&Attempt::NotYet, ATTEMPTS - 1), Next::GiveUp);
    assert_eq!(
        i64::from(ATTEMPTS) * RETRY_NANOS,
        5_000_000_000,
        "five seconds in all"
    );
}

#[test]
fn a_mac_without_application_windows_gives_up_at_once() {
    assert_eq!(next(&Attempt::Unavailable, 0), Next::GiveUp);
}
