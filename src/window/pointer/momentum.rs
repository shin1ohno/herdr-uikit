//! Two-finger trackpad scrolling, and the coast after the fingers lift.
//!
//! UIKit's scroll-type pan recognizer reports the fingers and stops at the
//! lift: a `UIScrollView` coasts by itself, and GPUI views get no such view.
//! So a lift fast enough to fling keeps scrolling here, slowing as
//! `UIScrollView`'s normal deceleration does (0.998 per millisecond), one
//! event per display frame, until it is slower than a few points a second.
//! The coast continues the lifted gesture: its events are `Moved`, and the
//! gesture's `Ended` comes when the coast ends or is stopped, so a consumer
//! that keeps a gesture's remainder (yebisu's wheel accumulator) sees one
//! gesture. A wheel notch is one line, apart from all this.
//!
//! Pure Rust without UIKit or GPUI types, so the host tests include this
//! file (`tests/scroll_momentum.rs`).

use std::time::Instant;

/// The slowest lift that coasts, in points a second: GPUI's own fling
/// threshold for touch pans (`min_fling_velocity`).
pub(crate) const START_SPEED: f32 = 50.;
/// A coast ends once slower than this, in points a second.
pub(crate) const STOP_SPEED: f32 = 4.;
/// How much speed is left after each millisecond.
pub(crate) const DECAY_PER_MS: f32 = 0.998;

/// The coast after one lift.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Momentum {
    /// Points a second on each axis, signed as the fingers moved.
    velocity: (f32, f32),
    last: Instant,
}

fn speed((x, y): (f32, f32)) -> f32 {
    x.hypot(y)
}

impl Momentum {
    /// A coast for a lift at `velocity`, or none when it is too slow (or not
    /// a number) to fling.
    pub(crate) fn start(velocity: (f32, f32), now: Instant) -> Option<Self> {
        let speed = speed(velocity);
        (speed.is_finite() && speed >= START_SPEED).then_some(Self {
            velocity,
            last: now,
        })
    }

    /// The distance travelled since the last step, or `None` once the coast
    /// has ended. The speed decays as `v0 * DECAY_PER_MS^ms`; the distance is
    /// its integral over the elapsed time, so a late frame catches up.
    pub(crate) fn step(&mut self, now: Instant) -> Option<(f32, f32)> {
        if speed(self.velocity) < STOP_SPEED {
            return None;
        }
        let ms = now.saturating_duration_since(self.last).as_secs_f32() * 1000.;
        self.last = now;
        let left = DECAY_PER_MS.powf(ms);
        // Integral of v0 * d^t over [0, ms], with v0 per second.
        let travelled = (left - 1.) / DECAY_PER_MS.ln() / 1000.;
        let (x, y) = self.velocity;
        self.velocity = (x * left, y * left);
        Some((x * travelled, y * travelled))
    }

    #[cfg(test)]
    pub(crate) fn velocity(&self) -> (f32, f32) {
        self.velocity
    }
}

/// The phase of one scroll event, as GPUI's `TouchPhase` has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// One scroll event to dispatch: a pixel delta following the fingers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Emit {
    pub(crate) delta: (f32, f32),
    pub(crate) phase: Phase,
}

impl Emit {
    fn new(delta: (f32, f32), phase: Phase) -> Self {
        Self { delta, phase }
    }
}

/// One trackpad gesture and its coast. The recognizer's states and the
/// display link's frames go in; the events to dispatch come out.
#[derive(Debug, Default)]
pub(crate) struct Gesture {
    coast: Option<Momentum>,
}

impl Gesture {
    /// Fingers came down on the trackpad: a coast still running ends first.
    pub(crate) fn began(&mut self, delta: (f32, f32)) -> Vec<Emit> {
        let mut events = Vec::from_iter(self.stop());
        events.push(Emit::new(delta, Phase::Started));
        events
    }

    pub(crate) fn changed(&mut self, delta: (f32, f32)) -> Emit {
        Emit::new(delta, Phase::Moved)
    }

    /// The fingers lifted at `velocity`. A fling's lift is one more move and
    /// the coast starts; a slow lift ends the gesture now.
    pub(crate) fn ended(&mut self, delta: (f32, f32), velocity: (f32, f32), now: Instant) -> Emit {
        self.coast = Momentum::start(velocity, now);
        if self.coast.is_some() {
            Emit::new(delta, Phase::Moved)
        } else {
            Emit::new(delta, Phase::Ended)
        }
    }

    pub(crate) fn cancelled(&mut self, delta: (f32, f32)) -> Emit {
        self.coast = None;
        Emit::new(delta, Phase::Cancelled)
    }

    /// Ends a coast (a new gesture, a finger on the screen, the window
    /// leaving): the coasting gesture's deferred `Ended`, if one was coasting.
    pub(crate) fn stop(&mut self) -> Option<Emit> {
        self.coast.take().map(|_| Emit::new((0., 0.), Phase::Ended))
    }

    /// One display frame: the coast's next move, or its end.
    pub(crate) fn frame(&mut self, now: Instant) -> Option<Emit> {
        let coast = self.coast.as_mut()?;
        match coast.step(now) {
            Some(delta) => Some(Emit::new(delta, Phase::Moved)),
            None => self.stop(),
        }
    }

    pub(crate) fn coasting(&self) -> bool {
        self.coast.is_some()
    }
}

/// One wheel notch as whole lines on each axis, signed as the translation:
/// GPUI scrolls `Lines` by its own line height, and yebisu multiplies one by
/// Herdr's `ui.mouse_scroll_lines`. `None` for a callback that did not move.
pub(crate) fn notch((x, y): (f32, f32)) -> Option<(f32, f32)> {
    let sign = |value: f32| {
        if value > 0. {
            1.
        } else if value < 0. {
            -1.
        } else {
            0.
        }
    };
    let lines = (sign(x), sign(y));
    (lines != (0., 0.)).then_some(lines)
}
