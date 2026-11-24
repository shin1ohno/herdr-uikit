//! T20, T21 in GPUI's terms: what the trackpad and wheel recognizers
//! dispatch for the steps `momentum` decides (host tests:
//! `tests/scroll_momentum.rs`).
use super::super::pointer::{momentum, trackpad_event, wheel_event};
use gpui::{Modifiers, ScrollDelta, ScrollSource, TouchPhase, point, px};

#[test]
fn a_trackpad_step_is_a_pointer_pixel_scroll_in_its_phase() {
    let at = point(px(10.), px(20.));
    for (phase, touch) in [
        (momentum::Phase::Started, TouchPhase::Started),
        (momentum::Phase::Moved, TouchPhase::Moved),
        (momentum::Phase::Ended, TouchPhase::Ended),
        (momentum::Phase::Cancelled, TouchPhase::Cancelled),
    ] {
        let event = trackpad_event(
            at,
            Modifiers::default(),
            momentum::Emit {
                delta: (1.5, -4.),
                phase,
            },
        );
        assert_eq!(event.position, at);
        assert_eq!(event.touch_phase, touch);
        assert_eq!(event.source, ScrollSource::Pointer);
        let ScrollDelta::Pixels(delta) = event.delta else {
            panic!("a trackpad step is pixels");
        };
        assert_eq!(delta, point(px(1.5), px(-4.)));
    }
}

#[test]
fn a_wheel_notch_is_one_line_never_pixels() {
    let at = point(px(10.), px(20.));
    let Some(event) = wheel_event(at, Modifiers::default(), (0., -10.)) else {
        panic!("a moving notch scrolls");
    };
    let ScrollDelta::Lines(lines) = event.delta else {
        panic!("a notch is lines, never pixels");
    };
    assert_eq!(lines, point(0., -1.));
    assert!(!event.delta.precise(), "GPUI tells a notch from a trackpad");
    assert_eq!(event.touch_phase, TouchPhase::Moved);
    assert_eq!(event.source, ScrollSource::Pointer);
    assert!(wheel_event(at, Modifiers::default(), (0., 0.)).is_none());
}

#[test]
fn a_coast_starts_at_the_lift_velocity() {
    let now = std::time::Instant::now();
    let Some(coast) = momentum::Momentum::start((30., -40.), now) else {
        panic!("50 pt/s coasts");
    };
    assert_eq!(coast.velocity(), (30., -40.));
}
