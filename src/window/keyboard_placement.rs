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
