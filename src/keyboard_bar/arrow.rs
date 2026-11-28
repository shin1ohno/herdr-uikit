//! The flick arrow key's touch logic: which arrow a finger's place on the
//! key means, when a held key starts repeating, and how fast it repeats.
//! The behaviour follows yebisu#276's owner decision (2026-11-29), after
//! Blink Shell's arrow key: the arrow comes from where the finger is
//! relative to the key's centre, at the first touch and while it moves; a
//! dead zone at the centre means no arrow; the arrow is sent when the
//! finger lifts; holding it `HOLD_SECS` starts repeating, faster the farther
//! the finger is from the centre. No code was taken from Blink.
//!
//! std only, so host tests can run it (`tests/keyboard_arrow.rs`). Points
//! are the key's own, with the origin at its top left.

/// Half the dead zone's width, as a fraction of the key's width: a finger
/// whose horizontal distance from the centre is below this and whose
/// vertical distance is below `DEAD_ZONE_HEIGHT` means no arrow.
pub const DEAD_ZONE_WIDTH: f64 = 0.2;
/// Half the dead zone's height, as a fraction of the key's height.
pub const DEAD_ZONE_HEIGHT: f64 = 0.15;
/// How long, in seconds, an arrow is held before it repeats.
pub const HOLD_SECS: f64 = 0.5;
/// Seconds between repeats while the finger is on the key.
pub const REPEAT_SECS: f64 = 0.1;
/// Seconds between repeats once the finger is beyond the key's edge.
pub const FAST_REPEAT_SECS: f64 = 0.07;
/// Seconds between repeats once the finger is `FASTEST_REACH` half-sizes
/// from the centre: a key's width or height beyond its edge.
pub const FASTEST_REPEAT_SECS: f64 = 0.04;
/// The reach (`reach`) beyond which repeats are fastest.
pub const FASTEST_REACH: f64 = 3.0;

/// One of the four arrows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Up,
    Down,
    Left,
    Right,
}

/// The finger's offset from the centre of a key of `size` (width, height),
/// or `None` for an empty key or a point that is not finite.
fn offset(size: (f64, f64), point: (f64, f64)) -> Option<(f64, f64)> {
    let (width, height) = size;
    let (x, y) = point;
    let valid = width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0
        && x.is_finite()
        && y.is_finite();
    valid.then(|| (x - width / 2.0, y - height / 2.0))
}

/// The arrow a finger at `point` on a key of `size` means, or `None` in the
/// dead zone. The key's diagonals divide the four arrows, so a key wider
/// than it is tall still has a quarter for each.
pub fn arrow(size: (f64, f64), point: (f64, f64)) -> Option<Arrow> {
    let (dx, dy) = offset(size, point)?;
    let (width, height) = size;
    if dx.abs() < width * DEAD_ZONE_WIDTH && dy.abs() < height * DEAD_ZONE_HEIGHT {
        return None;
    }
    Some(if dx.abs() / width >= dy.abs() / height {
        if dx < 0.0 { Arrow::Left } else { Arrow::Right }
    } else if dy < 0.0 {
        Arrow::Up
    } else {
        Arrow::Down
    })
}

/// How far the finger is from the centre in half-sizes of the key: 1.0 on
/// its edge, more outside it. `None` as for `offset`.
pub fn reach(size: (f64, f64), point: (f64, f64)) -> Option<f64> {
    let (dx, dy) = offset(size, point)?;
    let (width, height) = size;
    Some((dx.abs() / (width / 2.0)).max(dy.abs() / (height / 2.0)))
}

/// Seconds between repeats for a finger at `point`.
pub fn repeat_interval(size: (f64, f64), point: (f64, f64)) -> f64 {
    match reach(size, point) {
        Some(reach) if reach > FASTEST_REACH => FASTEST_REPEAT_SECS,
        Some(reach) if reach > 1.0 => FAST_REPEAT_SECS,
        _ => REPEAT_SECS,
    }
}

/// What the key's timer does after a touch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Timer {
    /// Leave a pending timer as it is.
    Keep,
    /// Cancel a pending timer.
    Stop,
    /// Fire after this many seconds, replacing a pending timer.
    After(f64),
}

/// One touch on the key, from the finger landing to it lifting.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tracker {
    /// The arrow the finger means now.
    arrow: Option<Arrow>,
    /// The timer has sent `arrow` at least once: it repeats, and lifting the
    /// finger sends nothing more.
    repeating: bool,
    /// The interval the repeating timer was last set to.
    interval: f64,
}

impl Tracker {
    /// The arrow the finger means now, to draw.
    pub fn arrow(&self) -> Option<Arrow> {
        self.arrow
    }

    /// The finger landed or moved to `point`. A new arrow starts the hold
    /// over; the dead zone stops it; a repeating arrow follows the finger's
    /// reach.
    pub fn touch(&mut self, size: (f64, f64), point: (f64, f64)) -> Timer {
        let arrow = arrow(size, point);
        if arrow != self.arrow {
            self.arrow = arrow;
            self.repeating = false;
            return match arrow {
                Some(_) => Timer::After(HOLD_SECS),
                None => Timer::Stop,
            };
        }
        if self.repeating {
            let interval = repeat_interval(size, point);
            if interval != self.interval {
                self.interval = interval;
                return Timer::After(interval);
            }
        }
        Timer::Keep
    }

    /// The timer fired with the finger at `point`: the arrow to send now,
    /// and the next timer.
    pub fn fire(&mut self, size: (f64, f64), point: (f64, f64)) -> (Option<Arrow>, Timer) {
        let Some(arrow) = self.arrow else {
            return (None, Timer::Stop);
        };
        self.repeating = true;
        self.interval = repeat_interval(size, point);
        (Some(arrow), Timer::After(self.interval))
    }

    /// The finger lifted at `point`: the arrow to send, if any (none in the
    /// dead zone, or for an arrow the timer already sent). The touch ends;
    /// the timer stops.
    pub fn release(&mut self, size: (f64, f64), point: (f64, f64)) -> Option<Arrow> {
        let _ = self.touch(size, point);
        let send = if self.repeating { None } else { self.arrow };
        *self = Self::default();
        send
    }

    /// The touch was cancelled: nothing is sent; the timer stops.
    pub fn cancel(&mut self) {
        *self = Self::default();
    }
}
