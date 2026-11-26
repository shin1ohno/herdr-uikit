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

/// Two scene windows restored together, neither extended yet, and a titled
/// window that shows no scene (a system panel, say).
fn two_scenes_and_a_panel() -> [Candidate<'static>; 3] {
    [
        Candidate {
            mask: SCENE_WINDOW,
            scene: Some("scene-a"),
        },
        Candidate {
            mask: SCENE_WINDOW,
            scene: Some("scene-b"),
        },
        Candidate {
            mask: SCENE_WINDOW,
            scene: None,
        },
    ]
}

#[test]
fn only_the_scenes_own_window_is_extended() {
    let windows = two_scenes_and_a_panel();
    assert_eq!(targets("scene-a", &windows), [(0, 0x800f)]);
    assert_eq!(targets("scene-b", &windows), [(1, 0x800f)]);
}

#[test]
fn a_scene_whose_window_is_not_listed_yet_extends_nothing() {
    // Scene B's window is already extended; scene C's has not appeared.
    let windows = [
        Candidate {
            mask: 0x800f,
            scene: Some("scene-b"),
        },
        Candidate {
            mask: SCENE_WINDOW,
            scene: None,
        },
    ];
    assert_eq!(targets("scene-c", &windows), []);
    assert_eq!(targets("scene-b", &windows), []);
}

#[test]
fn a_mac_whose_windows_do_not_tell_their_scene_extends_every_titled_window() {
    let windows = [
        Candidate {
            mask: SCENE_WINDOW,
            scene: None,
        },
        Candidate {
            mask: 0x8000,
            scene: None,
        },
        Candidate {
            mask: SCENE_WINDOW,
            scene: None,
        },
    ];
    assert_eq!(targets("scene-a", &windows), [(0, 0x800f), (2, 0x800f)]);
}
