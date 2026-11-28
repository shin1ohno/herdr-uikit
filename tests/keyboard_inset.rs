//! Host tests for which undocked keyboards take a bottom inset (std only).
//! The frames are the ones the iPadOS 27.2 Simulator's keyboard frame
//! notification reported on an iPad mini (A20 Pro) in portrait, 756 x 1158
//! points with a 20 pt bottom safe area (yebisu#277 runs 1 and 2: split with
//! `setSplit:YES animated:YES`, unsplit, moved up, floating).
#[allow(dead_code)]
#[path = "../src/window/keyboard_placement.rs"]
mod keyboard_placement;

#[path = "../src/window/keyboard_inset.rs"]
mod keyboard_inset;

use keyboard_inset::{
    NEAR_BOTTOM_GAP, OFF_SCREEN_HOLD_SECS, holds_inset, near_bottom, near_bottom_inset,
};
use keyboard_placement::{KeyboardPlacement, placement};

const SCREEN: (f64, f64, f64, f64) = (0.0, 0.0, 756.0, 1158.0);
const DOCKED: (f64, f64, f64, f64) = (0.0, 818.0, 756.0, 340.0);
const SPLIT: (f64, f64, f64, f64) = (0.0, 811.0, 756.0, 336.0);
const UNSPLIT: (f64, f64, f64, f64) = (0.0, 807.0, 756.0, 340.0);
const MOVED_UP: (f64, f64, f64, f64) = (0.0, 558.0, 756.0, 340.0);
const FLOATING: (f64, f64, f64, f64) = (210.0, 655.0, 334.0, 236.5);
/// The frame a split keyboard's notification briefly moves to.
const SPLIT_OFF_SCREEN: (f64, f64, f64, f64) = (0.0, 1158.0, 756.0, 336.0);

#[test]
fn a_split_or_just_unsplit_keyboard_at_the_bottom_covers_the_last_lines() {
    for keyboard in [SPLIT, UNSPLIT] {
        // The placement stays Undocked (11 pt above the bottom edge).
        assert_eq!(placement(SCREEN, keyboard), KeyboardPlacement::Undocked);
        assert!(near_bottom(SCREEN, keyboard), "{keyboard:?}");
    }
    // The window ends above the keyboard: its overlap, the gap included.
    assert_eq!(near_bottom_inset(SCREEN, SPLIT), 347.0);
    assert_eq!(near_bottom_inset(SCREEN, UNSPLIT), 351.0);
}

#[test]
fn a_keyboard_moved_up_keeps_the_window_as_it_is() {
    assert_eq!(placement(SCREEN, MOVED_UP), KeyboardPlacement::Undocked);
    assert!(!near_bottom(SCREEN, MOVED_UP));
}

#[test]
fn docked_and_floating_keyboards_are_not_near_bottom_undocked() {
    // A docked keyboard keeps its own inset (its overlap, 340 pt;
    // `layout/keyboard_geometry/tests.rs`).
    assert_eq!(placement(SCREEN, DOCKED), KeyboardPlacement::Docked);
    assert!(!near_bottom(SCREEN, DOCKED));
    assert_eq!(placement(SCREEN, FLOATING), KeyboardPlacement::Floating);
    assert!(!near_bottom(SCREEN, FLOATING));
    // Dragged against the bottom-left corner, it still floats.
    let cornered = (0.0, 1158.0 - 236.5, 334.0, 236.5);
    assert_eq!(placement(SCREEN, cornered), KeyboardPlacement::Floating);
    assert!(!near_bottom(SCREEN, cornered));
    // Hidden frames are not near the bottom either.
    for hidden in [
        SPLIT_OFF_SCREEN,
        (0.0, 0.0, 0.0, 0.0),
        (0.0, 1158.0, 756.0, 0.0),
    ] {
        assert!(!near_bottom(SCREEN, hidden), "{hidden:?}");
    }
}

#[test]
fn the_gap_threshold_is_inclusive() {
    let ending_at = |gap: f64| (0.0, 1158.0 - gap - 336.0, 756.0, 336.0);
    assert!(near_bottom(SCREEN, ending_at(NEAR_BOTTOM_GAP)));
    assert!(!near_bottom(SCREEN, ending_at(NEAR_BOTTOM_GAP + 0.5)));
    // Between the measured 11 pt and the moved-up 260 pt.
    const { assert!(NEAR_BOTTOM_GAP > 11.0 && NEAR_BOTTOM_GAP < 260.0) };
}

#[test]
fn the_screen_origin_is_respected() {
    let screen = (100.0, 50.0, 756.0, 1158.0);
    let split = (100.0, 861.0, 756.0, 336.0);
    assert!(near_bottom(screen, split));
    assert_eq!(near_bottom_inset(screen, split), 347.0);
    assert!(!near_bottom(screen, (100.0, 608.0, 756.0, 340.0)));
}

#[test]
fn the_inset_is_the_views_own() {
    // A Stage Manager window ending above the keyboard's bottom edge.
    let window = (80.0, 120.0, 600.0, 900.0);
    assert_eq!(near_bottom_inset(window, SPLIT), 1020.0 - 811.0);
    // One ending above the keyboard's top edge is not covered.
    assert_eq!(near_bottom_inset((80.0, 120.0, 600.0, 600.0), SPLIT), 0.0);
    // A keyboard that misses the window horizontally covers nothing.
    assert_eq!(near_bottom_inset((800.0, 0.0, 300.0, 1158.0), SPLIT), 0.0);
    // Never more than the window's height.
    assert_eq!(near_bottom_inset((0.0, 900.0, 756.0, 200.0), SPLIT), 200.0);
    assert_eq!(near_bottom_inset(SCREEN, (0.0, 811.0, 756.0, 0.0)), 0.0);
}

#[test]
fn a_split_keyboards_moment_off_screen_holds_the_inset() {
    // While split, the notification moves the keyboard below the screen and
    // the keyboard's frame comes back 0.25-0.27 s later.
    assert!(holds_inset(SCREEN, SPLIT, SPLIT_OFF_SCREEN));
    assert!(holds_inset(SCREEN, UNSPLIT, (0.0, 1158.0, 756.0, 340.0)));
    // Longer than the measured 0.27 s, short enough that a real hide stays
    // prompt.
    const { assert!(OFF_SCREEN_HOLD_SECS > 0.27 && OFF_SCREEN_HOLD_SECS <= 1.0) };
}

#[test]
fn other_frames_change_the_inset_at_once() {
    // A docked, moved-up or floating keyboard hides as before.
    for previous in [DOCKED, MOVED_UP, FLOATING, (0.0, 0.0, 0.0, 0.0)] {
        assert!(
            !holds_inset(SCREEN, previous, SPLIT_OFF_SCREEN),
            "{previous:?}"
        );
    }
    // From a near-bottom keyboard, only a full-width frame below the screen
    // waits: not an empty or zero-tall frame, not a narrower one, and not
    // another frame on screen.
    for end in [
        (0.0, 0.0, 0.0, 0.0),
        (0.0, 1158.0, 756.0, 0.0),
        (210.0, 1158.0, 334.0, 236.5),
        (f64::NAN, 1158.0, 756.0, 336.0),
        SPLIT,
        UNSPLIT,
        MOVED_UP,
        DOCKED,
        FLOATING,
    ] {
        assert!(!holds_inset(SCREEN, SPLIT, end), "{end:?}");
    }
}
