//! Host tests for the flick arrow key's touch logic (std only): the arrow a
//! finger's place means, the dead zone, sending on release, the hold that
//! starts repeating and the faster repeats farther from the centre. The key
//! sizes are the accessory row's on an iPad mini (A20 Pro) in portrait
//! (44 x 40 pt) and the shortcuts bar's arrow key (52 x 36 pt).
#[path = "../src/keyboard_bar/arrow.rs"]
mod arrow;

use arrow::{
    Arrow, DEAD_ZONE_HEIGHT, DEAD_ZONE_WIDTH, FAST_REPEAT_SECS, FASTEST_REPEAT_SECS, HOLD_SECS,
    REPEAT_SECS, Timer, Tracker, arrow, repeat_interval,
};

const ROW_KEY: (f64, f64) = (44.0, 40.0);
const BAR_KEY: (f64, f64) = (52.0, 36.0);
const CENTRE: (f64, f64) = (22.0, 20.0);

#[test]
fn each_side_of_the_key_is_its_arrow() {
    assert_eq!(arrow(ROW_KEY, (22.0, 2.0)), Some(Arrow::Up));
    assert_eq!(arrow(ROW_KEY, (22.0, 38.0)), Some(Arrow::Down));
    assert_eq!(arrow(ROW_KEY, (2.0, 20.0)), Some(Arrow::Left));
    assert_eq!(arrow(ROW_KEY, (42.0, 20.0)), Some(Arrow::Right));
    // Outside the key the finger still means the arrow of its side.
    assert_eq!(arrow(ROW_KEY, (22.0, -60.0)), Some(Arrow::Up));
    assert_eq!(arrow(ROW_KEY, (200.0, 30.0)), Some(Arrow::Right));
}

#[test]
fn the_diagonals_divide_a_wide_key() {
    // 20 pt right and 14 pt down on a 52 x 36 key: 0.38 of the width,
    // 0.39 of the height, so down; a raw comparison would say right.
    assert_eq!(arrow(BAR_KEY, (46.0, 32.0)), Some(Arrow::Down));
    assert_eq!(arrow(BAR_KEY, (48.0, 26.0)), Some(Arrow::Right));
}

#[test]
fn the_centre_is_a_dead_zone() {
    assert_eq!(arrow(ROW_KEY, CENTRE), None);
    let inside_x = ROW_KEY.0 * DEAD_ZONE_WIDTH - 0.5;
    let inside_y = ROW_KEY.1 * DEAD_ZONE_HEIGHT - 0.5;
    assert_eq!(arrow(ROW_KEY, (22.0 + inside_x, 20.0 + inside_y)), None);
    assert_eq!(arrow(ROW_KEY, (22.0 - inside_x, 20.0 - inside_y)), None);
    // Just past the zone's edge on one axis.
    let past_x = ROW_KEY.0 * DEAD_ZONE_WIDTH + 0.5;
    assert_eq!(arrow(ROW_KEY, (22.0 + past_x, 20.0)), Some(Arrow::Right));
    let past_y = ROW_KEY.1 * DEAD_ZONE_HEIGHT + 0.5;
    assert_eq!(arrow(ROW_KEY, (22.0, 20.0 - past_y)), Some(Arrow::Up));
}

#[test]
fn an_empty_key_or_a_bad_point_means_no_arrow() {
    assert_eq!(arrow((0.0, 40.0), (0.0, 0.0)), None);
    assert_eq!(arrow(ROW_KEY, (f64::NAN, 2.0)), None);
    assert_eq!(arrow(ROW_KEY, (22.0, f64::INFINITY)), None);
}

#[test]
fn a_tap_sends_its_arrow_on_release_only() {
    let mut tracker = Tracker::default();
    assert_eq!(tracker.touch(ROW_KEY, (22.0, 2.0)), Timer::After(HOLD_SECS));
    assert_eq!(tracker.arrow(), Some(Arrow::Up));
    assert_eq!(tracker.release(ROW_KEY, (22.0, 2.0)), Some(Arrow::Up));
    assert_eq!(tracker, Tracker::default());
}

#[test]
fn a_flick_from_the_centre_sends_where_it_ends() {
    for (end, sent) in [
        ((22.0, -10.0), Arrow::Up),
        ((22.0, 50.0), Arrow::Down),
        ((-10.0, 20.0), Arrow::Left),
        ((54.0, 20.0), Arrow::Right),
    ] {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.touch(ROW_KEY, CENTRE), Timer::Keep);
        assert_eq!(tracker.touch(ROW_KEY, end), Timer::After(HOLD_SECS));
        assert_eq!(tracker.release(ROW_KEY, end), Some(sent));
    }
}

#[test]
fn lifting_in_the_dead_zone_sends_nothing() {
    let mut tracker = Tracker::default();
    tracker.touch(ROW_KEY, (22.0, 2.0));
    assert_eq!(tracker.touch(ROW_KEY, CENTRE), Timer::Stop);
    assert_eq!(tracker.release(ROW_KEY, CENTRE), None);
}

#[test]
fn a_hold_repeats_and_release_adds_nothing() {
    let mut tracker = Tracker::default();
    tracker.touch(ROW_KEY, (2.0, 20.0));
    assert_eq!(
        tracker.fire(ROW_KEY, (2.0, 20.0)),
        (Some(Arrow::Left), Timer::After(REPEAT_SECS))
    );
    assert_eq!(
        tracker.fire(ROW_KEY, (2.0, 20.0)),
        (Some(Arrow::Left), Timer::After(REPEAT_SECS))
    );
    assert_eq!(tracker.touch(ROW_KEY, (3.0, 20.0)), Timer::Keep);
    assert_eq!(tracker.release(ROW_KEY, (3.0, 20.0)), None);
}

#[test]
fn turning_while_held_starts_the_hold_again() {
    let mut tracker = Tracker::default();
    tracker.touch(ROW_KEY, (22.0, 2.0));
    tracker.fire(ROW_KEY, (22.0, 2.0));
    assert_eq!(
        tracker.touch(ROW_KEY, (42.0, 20.0)),
        Timer::After(HOLD_SECS)
    );
    // The new arrow was not sent yet, so lifting sends it.
    assert_eq!(tracker.release(ROW_KEY, (42.0, 20.0)), Some(Arrow::Right));
}

#[test]
fn repeats_speed_up_farther_from_the_centre() {
    assert_eq!(repeat_interval(ROW_KEY, (22.0, 2.0)), REPEAT_SECS);
    // Beyond the top edge.
    assert_eq!(repeat_interval(ROW_KEY, (22.0, -10.0)), FAST_REPEAT_SECS);
    // More than a key's height beyond it (reach 3.5).
    assert_eq!(repeat_interval(ROW_KEY, (22.0, -50.0)), FASTEST_REPEAT_SECS);
    let mut tracker = Tracker::default();
    tracker.touch(ROW_KEY, (22.0, 2.0));
    tracker.fire(ROW_KEY, (22.0, 2.0));
    assert_eq!(
        tracker.touch(ROW_KEY, (22.0, -10.0)),
        Timer::After(FAST_REPEAT_SECS)
    );
    assert_eq!(
        tracker.fire(ROW_KEY, (22.0, -10.0)),
        (Some(Arrow::Up), Timer::After(FAST_REPEAT_SECS))
    );
    assert_eq!(
        tracker.touch(ROW_KEY, (22.0, -50.0)),
        Timer::After(FASTEST_REPEAT_SECS)
    );
    assert_eq!(tracker.touch(ROW_KEY, (22.0, -52.0)), Timer::Keep);
}

#[test]
fn a_cancelled_touch_sends_nothing() {
    let mut tracker = Tracker::default();
    tracker.touch(ROW_KEY, (22.0, 2.0));
    tracker.cancel();
    assert_eq!(tracker, Tracker::default());
    assert_eq!(tracker.release(ROW_KEY, CENTRE), None);
}
