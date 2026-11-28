//! Which full-width undocked keyboards, and which accessory rows, take a
//! bottom inset. A docked keyboard covers the window's bottom, so the window
//! ends above it. An undocked one usually leaves it alone: the user moved it
//! up, and the lines under it stay visible (`layout/keyboard_geometry.rs`). A
//! split keyboard, and one just unsplit, is undocked too but still sits at
//! the screen's bottom edge, so it hides the window's last lines unless it
//! takes an inset (yebisu#277). The consumer's accessory row
//! (`keyboard_accessory`) covers the window's bottom below a floating
//! keyboard, and the lines above a keyboard that takes an inset
//! (`accessory_inset`).
//!
//! Plain arithmetic in points, with no UIKit types, so host tests can run it
//! (`tests/keyboard_inset.rs`).

use super::keyboard_placement::{KeyboardPlacement, TOLERANCE, placement};

/// How far, in points, a full-width undocked keyboard's bottom edge may sit
/// above the screen's and still take a bottom inset (`near_bottom`). On the
/// iPad mini (A20 Pro) Simulator, iPadOS 27.2, a split keyboard and one just
/// unsplit end 11 pt above the screen's bottom edge, while one moved up ends
/// hundreds of points above it (260 pt in the run; yebisu#277). The margin
/// allows for a device whose gap is a little larger; such a keyboard leaves
/// at most this much of the window unused below it.
pub const NEAR_BOTTOM_GAP: f64 = 32.0;

/// How long, in seconds, a near-bottom undocked keyboard's inset waits
/// before it follows a frame notification that moves the keyboard below the
/// screen (`holds_inset`). While split, the Simulator posted such a frame
/// about once a second and the keyboard's own frame came back 0.25-0.27 s
/// later (yebisu#277); without the wait the window would shrink and grow.
pub const OFF_SCREEN_HOLD_SECS: f64 = 0.5;

/// Whether `keyboard` is full width, `Undocked`, and its bottom edge at most
/// `NEAR_BOTTOM_GAP` points above the `screen`'s, both (x, y, width, height)
/// in the screen's points: a split keyboard, or one just unsplit, still at
/// the bottom. It covers the window's last lines as a docked keyboard does,
/// so it takes a bottom inset (`near_bottom_inset`); its placement stays
/// `Undocked`.
pub fn near_bottom(screen: (f64, f64, f64, f64), keyboard: (f64, f64, f64, f64)) -> bool {
    let (_, sy, _, sh) = screen;
    let (_, ky, _, kh) = keyboard;
    placement(screen, keyboard) == KeyboardPlacement::Undocked
        && sy + sh - (ky + kh) <= NEAR_BOTTOM_GAP
}

/// The bottom inset a `near_bottom` keyboard gives a `view`, both in the
/// same points: from the keyboard's top edge to the view's bottom edge, so
/// the view's content ends above the keyboard, the gap under the keyboard
/// included. A keyboard that misses the view horizontally, or starts below
/// it, gives none.
pub fn near_bottom_inset(view: (f64, f64, f64, f64), keyboard: (f64, f64, f64, f64)) -> f64 {
    let (vx, vy, vw, vh) = view;
    let (kx, ky, kw, kh) = keyboard;
    let overlaps_horizontally = kx < vx + vw && kx + kw > vx;
    // Also refuses a NaN height.
    if kh.is_nan() || kh <= 0.0 || !overlaps_horizontally {
        return 0.0;
    }
    (vy + vh - ky).clamp(0.0, vh.max(0.0))
}

/// Whether a relevant frame notification ending at `end` waits
/// `OFF_SCREEN_HOLD_SECS` before it changes the bottom inset: the last
/// relevant one (`previous`) put the keyboard `near_bottom`, and this one
/// moves a full-width keyboard to or below the screen's bottom edge. A newer
/// notification within the wait replaces it; a real hide follows the wait.
pub fn holds_inset(
    screen: (f64, f64, f64, f64),
    previous: (f64, f64, f64, f64),
    end: (f64, f64, f64, f64),
) -> bool {
    let (sx, sy, sw, sh) = screen;
    let (ex, ey, ew, eh) = end;
    let below = [ex, ey, ew, eh].iter().all(|v| v.is_finite())
        && eh > 0.0
        && ey >= sy + sh - TOLERANCE
        && ex <= sx + TOLERANCE
        && ex + ew >= sx + sw - TOLERANCE;
    below && near_bottom(screen, previous)
}

/// How far, in points, the accessory row's bottom edge may be from the top
/// of the keyboard inset below it and still sit on that keyboard.
pub const ROW_ON_KEYBOARD_GAP: f64 = 4.0;

/// The bottom inset the accessory `row` gives a `view` on a `screen`, all
/// (x, y, width, height) in the screen's points, when the keyboard alone
/// gives `keyboard_inset`. A full-width row on the screen's bottom edge (below
/// a floating keyboard: UIKit puts it there) or on top of the keyboard's inset
/// covers the window from its top edge down, so the window ends above it. A
/// row above a keyboard the user moved up, which takes no inset, gives none,
/// as that keyboard does; so does a narrower row.
pub fn accessory_inset(
    screen: (f64, f64, f64, f64),
    view: (f64, f64, f64, f64),
    row: (f64, f64, f64, f64),
    keyboard_inset: f64,
) -> f64 {
    let (sx, sy, sw, sh) = screen;
    let (rx, ry, rw, rh) = row;
    let (_, vy, _, vh) = view;
    let finite = [sx, sy, sw, sh, rx, ry, rw, rh, vy, vh, keyboard_inset]
        .iter()
        .all(|v| v.is_finite());
    if !finite || rw <= 0.0 || rh <= 0.0 {
        return 0.0;
    }
    let spans = rx <= sx + TOLERANCE && rx + rw >= sx + sw - TOLERANCE;
    let row_bottom = ry + rh;
    let at_bottom = (sy + sh - row_bottom).abs() <= NEAR_BOTTOM_GAP;
    let on_keyboard = keyboard_inset > 0.0
        && (row_bottom - (vy + vh - keyboard_inset)).abs() <= ROW_ON_KEYBOARD_GAP;
    if !spans || !(at_bottom || on_keyboard) {
        return 0.0;
    }
    near_bottom_inset(view, row)
}

/// Whether the accessory `row` (its frame on the `screen`, both in screen
/// points) sits full width on the screen's bottom edge. UIKit puts it there
/// only below a floating keyboard; above a docked or split keyboard it is
/// that keyboard's height higher. So the keyboard is not docked then,
/// whatever the last frame notification said: on the iPad Pro 13-inch (M5)
/// Simulator a pinch to floating posted no notification the window took, and
/// the docked keyboard's last one stayed (yebisu#276 run r19).
pub fn row_alone_at_bottom(screen: (f64, f64, f64, f64), row: (f64, f64, f64, f64)) -> bool {
    let (sx, sy, sw, sh) = screen;
    let (rx, ry, rw, rh) = row;
    [sx, sy, sw, sh, rx, ry, rw, rh]
        .iter()
        .all(|v| v.is_finite())
        && rw > 0.0
        && rh > 0.0
        && rx <= sx + TOLERANCE
        && rx + rw >= sx + sw - TOLERANCE
        && (ry + rh - (sy + sh)).abs() <= TOLERANCE
}
