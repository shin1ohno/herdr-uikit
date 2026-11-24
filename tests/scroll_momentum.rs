//! Host tests for the trackpad coast and the wheel notch (std only). The
//! window code dispatches exactly what `Gesture` and `notch` return
//! (`src/window/pointer.rs`, `src/window/frame.rs`).
#[path = "../src/window/pointer/momentum.rs"]
mod momentum;

use momentum::*;
use std::time::{Duration, Instant};

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

/// T16: a lift slower than GPUI's fling threshold does not coast.
#[test]
fn a_lift_coasts_only_from_the_fling_speed() {
    let now = Instant::now();
    assert!(Momentum::start((0., START_SPEED - 1.), now).is_none());
    assert!(Momentum::start((0., START_SPEED), now).is_some());
    assert!(
        Momentum::start((-30., -40.), now).is_some(),
        "speed is the length"
    );
    assert!(Momentum::start((f32::NAN, 800.), now).is_none());
    assert!(Momentum::start((f32::INFINITY, 0.), now).is_none());
}

/// T17: each frame moves less than the one before, the same way, and the
/// coast ends below the stop speed after a finite distance.
#[test]
fn a_coast_slows_every_frame_and_ends() {
    let start = Instant::now();
    let mut coast = Momentum::start((0., 800.), start).unwrap_or_else(|| panic!("coast"));
    let mut last = f32::INFINITY;
    let mut total = 0.;
    let mut frames = 0;
    let mut now = start;
    while let Some((x, y)) = coast.step(now + ms(16)) {
        now += ms(16);
        assert_eq!(x, 0.);
        assert!(y > 0. && y < last, "frame {frames}: {y} after {last}");
        last = y;
        total += y;
        frames += 1;
        assert!(frames < 1000, "the coast never ended");
    }
    assert!(coast.velocity().1 < STOP_SPEED);
    // v0 / ln(1 / d) per millisecond: 800 / 1000 / 0.002 = 400 pt, less
    // the tail below the stop speed.
    assert!((390. ..=400.).contains(&total), "{total}");
    assert!(
        coast.step(now + ms(16)).is_none(),
        "an ended coast stays ended"
    );
}

/// T18: the speed follows v0 * 0.998^ms, and the distance is its integral
/// however the frames fall.
#[test]
fn the_speed_decays_as_uiscrollviews_normal_rate() {
    for elapsed in [100, 500] {
        let start = Instant::now();
        let mut coast = Momentum::start((0., 1000.), start).unwrap_or_else(|| panic!("coast"));
        let one = coast
            .step(start + ms(elapsed))
            .unwrap_or_else(|| panic!("step"))
            .1;
        // UIScrollView's normal rate, written out: the constant must not drift.
        let expected = 1000. * 0.998_f32.powf(elapsed as f32);
        let speed = coast.velocity().1;
        assert!(
            (speed - expected).abs() <= expected * 0.01,
            "{elapsed} ms: {speed} vs {expected}"
        );
        let mut split = Momentum::start((0., 1000.), start).unwrap_or_else(|| panic!("coast"));
        let mut two = 0.;
        for step in 1..=10 {
            two += split
                .step(start + ms(elapsed * step / 10))
                .unwrap_or_else(|| panic!("step"))
                .1;
        }
        assert!((one - two).abs() <= one * 0.01, "{one} vs {two}");
    }
}

/// T19: a stopped coast moves no more.
#[test]
fn a_stopped_coast_moves_no_more() {
    let start = Instant::now();
    let mut gesture = Gesture::default();
    gesture.ended((0., 4.), (0., 900.), start);
    assert!(gesture.coasting());
    assert_eq!(
        gesture.stop(),
        Some(Emit {
            delta: (0., 0.),
            phase: Phase::Ended
        })
    );
    assert!(!gesture.coasting());
    assert_eq!(gesture.frame(start + ms(16)), None);
    assert_eq!(gesture.stop(), None, "only one Ended");
}

fn phases(events: &[Emit]) -> Vec<Phase> {
    events.iter().map(|event| event.phase).collect()
}

/// T20: a fling's lift is one more move, the coast moves on frames, and the
/// gesture ends once, when the coast does.
#[test]
fn a_fling_coasts_as_one_gesture_ending_once() {
    let start = Instant::now();
    let mut gesture = Gesture::default();
    let mut events = gesture.began((0., 2.));
    events.push(gesture.changed((0., 6.)));
    events.push(gesture.ended((0., 3.), (0., 800.), start));
    assert_eq!(events[2].delta, (0., 3.), "the lift's own motion is kept");
    let mut now = start;
    while let Some(event) = gesture.frame(now + ms(16)) {
        now += ms(16);
        events.push(event);
        assert!(events.len() < 1000);
    }
    let mut expected = vec![Phase::Started, Phase::Moved, Phase::Moved];
    expected.resize(events.len() - 1, Phase::Moved);
    expected.push(Phase::Ended);
    assert_eq!(phases(&events), expected);
    assert!(events.len() > 10, "the coast moved on frames");
    assert!(!gesture.coasting());
}

/// T20: a slow lift ends the gesture at once; a cancel never coasts.
#[test]
fn a_slow_lift_or_a_cancel_ends_without_a_coast() {
    let start = Instant::now();
    let mut gesture = Gesture::default();
    gesture.began((0., 1.));
    assert_eq!(
        gesture.ended((0., 1.), (0., 20.), start).phase,
        Phase::Ended
    );
    assert_eq!(gesture.frame(start + ms(16)), None);
    gesture.began((0., 1.));
    gesture.ended((0., 1.), (0., 900.), start);
    assert_eq!(gesture.cancelled((0., 0.)).phase, Phase::Cancelled);
    assert_eq!(gesture.frame(start + ms(16)), None);
}

/// T20: fingers coming down again stop the coast: the coasting gesture ends
/// before the new one starts, and no coast move follows.
#[test]
fn new_fingers_stop_the_coast_before_their_gesture_starts() {
    let start = Instant::now();
    let mut gesture = Gesture::default();
    gesture.began((0., 2.));
    gesture.ended((0., 2.), (0., 900.), start);
    assert!(gesture.frame(start + ms(16)).is_some());
    let events = gesture.began((0., -1.));
    assert_eq!(phases(&events), [Phase::Ended, Phase::Started]);
    assert_eq!(gesture.frame(start + ms(32)), None);
}

/// T21: a wheel notch is one line on each axis it moved, signed as the
/// translation; a callback that did not move is nothing.
#[test]
fn a_wheel_notch_is_one_line_per_moving_axis() {
    assert_eq!(notch((0., -10.)), Some((0., -1.)));
    assert_eq!(notch((0., 0.4)), Some((0., 1.)));
    assert_eq!(notch((7., 0.)), Some((1., 0.)));
    assert_eq!(notch((-3., 12.)), Some((-1., 1.)));
    assert_eq!(notch((0., 0.)), None);
}
