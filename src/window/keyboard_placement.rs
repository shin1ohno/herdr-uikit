//! Where the software keyboard is, from the frame UIKit's keyboard frame
//! notification ends at. A floating keyboard leaves no bottom inset
//! (`layout/keyboard_geometry.rs`), so without this a consumer cannot tell it
//! from a hidden keyboard, and UIKit shows no shortcuts bar on a floating
//! keyboard (yebisu#229: measured on the iPadOS 27.2 Simulator).
//!
//! The frame alone is not enough: see `combine`.
//!
//! Plain arithmetic in points, with no UIKit types, so host tests can run it
//! (`tests/keyboard_placement.rs`).

/// How far, in points, a keyboard edge may be from the screen's and still
/// touch it. UIKit rounds frames to the screen's pixels.
pub const TOLERANCE: f64 = 1.0;

/// The software keyboard's place on its screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardPlacement {
    /// No keyboard is on screen, or none was reported yet.
    #[default]
    Hidden,
    /// Full width and on the screen's bottom edge: the shortcuts bar shows.
    Docked,
    /// Full width but away from the bottom edge (undocked, or split and
    /// moved up).
    Undocked,
    /// Narrower than the screen: the floating keyboard. Its shortcuts bar
    /// shows no consumer items.
    Floating,
}

/// The placement of a `keyboard` frame on a `screen`, both (x, y, width,
/// height) in the screen's points. A frame that is empty, not finite, or off
/// the screen is `Hidden`.
pub fn placement(
    screen: (f64, f64, f64, f64),
    keyboard: (f64, f64, f64, f64),
) -> KeyboardPlacement {
    let (sx, sy, sw, sh) = screen;
    let (kx, ky, kw, kh) = keyboard;
    let finite = [sx, sy, sw, sh, kx, ky, kw, kh]
        .iter()
        .all(|v| v.is_finite());
    if !finite || kw <= 0.0 || kh <= 0.0 || sw <= 0.0 || sh <= 0.0 {
        return KeyboardPlacement::Hidden;
    }
    let screen_bottom = sy + sh;
    let screen_right = sx + sw;
    let off_screen = ky >= screen_bottom - TOLERANCE
        || ky + kh <= sy + TOLERANCE
        || kx >= screen_right - TOLERANCE
        || kx + kw <= sx + TOLERANCE;
    if off_screen {
        return KeyboardPlacement::Hidden;
    }
    let spans = kx <= sx + TOLERANCE && kx + kw >= screen_right - TOLERANCE;
    let on_bottom = ky + kh >= screen_bottom - TOLERANCE;
    match (spans, on_bottom) {
        (true, true) => KeyboardPlacement::Docked,
        (true, false) => KeyboardPlacement::Undocked,
        (false, _) => KeyboardPlacement::Floating,
    }
}

/// The placement of the drawing view's keyboard guide, which follows
/// undocked keyboards: `guide` in screen points, over a window whose bottom
/// safe area is `safe_bottom` points. With no keyboard up the guide is a
/// full-width strip as tall as that safe area, which is `Hidden`.
pub fn guide_placement(
    screen: (f64, f64, f64, f64),
    guide: (f64, f64, f64, f64),
    safe_bottom: f64,
) -> KeyboardPlacement {
    match placement(screen, guide) {
        KeyboardPlacement::Docked if guide.3 <= safe_bottom.max(0.0) + TOLERANCE => {
            KeyboardPlacement::Hidden
        }
        placement => placement,
    }
}

/// Whether the shortcuts bar is split, from where UIKit drew the bar's
/// sentinel (`sentinel`, the custom view of the first trailing item, in the
/// screen's points; `None` while UIKit draws no bar). A docked or undocked
/// keyboard puts the trailing groups on the screen's right half; a split one
/// puts every group over its left half (yebisu#276, measured on the iPad mini
/// (A20 Pro) Simulator, also with no consumer items: see the host tests).
/// The frame notification cannot tell them apart: a split keyboard's frame
/// stays full width. The sentinel must end left of the screen's middle (not
/// on it: while a keyboard shrinks into a floating one, UIKit drew it at
/// x 377.1-378.1 on the 756-point screen for 0.7 s). A sentinel at or below
/// the screen's bottom edge (a keyboard sliding away) is not split.
pub fn bar_split(screen: (f64, f64, f64, f64), sentinel: Option<(f64, f64, f64, f64)>) -> bool {
    let Some((x, y, w, h)) = sentinel else {
        return false;
    };
    let (sx, sy, sw, sh) = screen;
    let finite = [sx, sy, sw, sh, x, y, w, h].iter().all(|v| v.is_finite());
    if !finite || sw <= 0.0 || sh <= 0.0 || w < 0.0 || h <= 0.0 {
        return false;
    }
    let on_screen = y >= sy - TOLERANCE && y < sy + sh - TOLERANCE && x + w > sx;
    on_screen && x + w < sx + sw / 2.0
}

/// Whether the bar is split after a sentinel sample, from the last answer
/// (`was`). A floating keyboard is never split. Otherwise a drawn sentinel
/// decides (`bar_split`), also while a frame notification briefly says
/// `Hidden` (UIKit posts off-screen frames during a split). An undrawn
/// sentinel keeps the last answer while the keyboard is docked or undocked:
/// UIKit takes the sentinel out of its window for a moment whenever the
/// bar's groups are replaced, as when a consumer empties its bar on a split
/// keyboard. With a hidden keyboard an undrawn sentinel is not split.
pub fn split_after(
    was: bool,
    screen: (f64, f64, f64, f64),
    sentinel: Option<(f64, f64, f64, f64)>,
    placement: KeyboardPlacement,
) -> bool {
    match (sentinel, placement) {
        (_, KeyboardPlacement::Floating) => false,
        (Some(_), _) => bar_split(screen, sentinel),
        (None, KeyboardPlacement::Docked | KeyboardPlacement::Undocked) => was,
        (None, KeyboardPlacement::Hidden) => false,
    }
}

/// The placement to report from the last relevant frame notification's
/// (`notified`) and the keyboard guide's (`guide`, `guide_placement`). A
/// notification with a keyboard frame decides. One saying `Hidden` defers to
/// the guide: UIKit ends a float with a zero-tall frame while the keyboard
/// stays up, and a floating keyboard's own frame notification can be another
/// screen object's (not taken as this window's). A guide that lags a real
/// frame never overrides it; after a hide, the next layout pass brings the
/// guide back to the safe area.
pub fn combine(notified: KeyboardPlacement, guide: KeyboardPlacement) -> KeyboardPlacement {
    match notified {
        KeyboardPlacement::Hidden => guide,
        notified => notified,
    }
}
