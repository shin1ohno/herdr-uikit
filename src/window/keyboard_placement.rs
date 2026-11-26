//! Where the software keyboard is, from the frame UIKit's keyboard frame
//! notification ends at. A floating keyboard leaves no bottom inset
//! (`layout/keyboard_geometry.rs`), so without this a consumer cannot tell it
//! from a hidden keyboard, and UIKit shows no shortcuts bar on a floating
//! keyboard (yebisu#229: measured on the iPadOS 27.2 Simulator).
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
