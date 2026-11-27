//! Whether a window still fills its scene after the scene's geometry changed.
//!
//! A `UIWindow` made with `initWithWindowScene:` should take its scene's
//! bounds, and its root view controller's view the window's bounds. When the
//! scene grows and the window or that view keeps its old frame, nothing in
//! the drawing view changes size. UIKit then sends no `layoutSubviews`, GPUI
//! keeps drawing at the old size, and the rest of the scene stays black.
//! In TestFlight 0.1.1, an iPad window on an external display that the owner
//! had just widened showed this (yebisu task 87ef7835b9ab449a). [Inferred]
//! The black part was as wide as the window's distance from the screen's
//! left edge, so the window appears to have kept a frame from before the
//! resize.
//!
//! Plain arithmetic in points, with no UIKit types, so host tests can run it
//! (`tests/scene_fit.rs`).

/// A rectangle in points, as UIKit's `CGRect` carries it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Frame {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// The same size at the origin: a frame that fills a parent whose bounds
    /// are `self`.
    fn at_origin(self) -> Self {
        Self::new(0.0, 0.0, self.width, self.height)
    }

    /// Whether this is finite, with a positive width and height.
    fn drawable(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width > 0.0
            && self.height > 0.0
    }

    /// Whether every edge is within [`TOLERANCE`] of `other`'s.
    fn matches(self, other: Self) -> bool {
        [
            (self.x, other.x),
            (self.y, other.y),
            (self.width, other.width),
            (self.height, other.height),
        ]
        .iter()
        .all(|(a, b)| (a - b).abs() <= TOLERANCE)
    }
}

/// How far a frame may be from the one it should have, in points, before it
/// counts as left behind. UIKit rounds frames to the screen's pixels, which
/// is at most half a point at a scale of 1.
pub const TOLERANCE: f64 = 0.5;

/// The frames to set so the window fills its scene again; `None` for a frame
/// that already fits.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fit {
    /// The window's new frame, in its scene's coordinates.
    pub window: Option<Frame>,
    /// The root view's new frame, in the window's coordinates.
    pub view: Option<Frame>,
}

impl Fit {
    pub fn is_empty(self) -> bool {
        self.window.is_none() && self.view.is_none()
    }
}

/// What to set after a geometry change. `scene` is the scene's coordinate
/// space bounds, `window` the window's frame in it and `view` the root
/// view's frame in the window. A scene with no drawable size (one being
/// torn down, or a NaN from UIKit) changes nothing, nor does a frame within
/// [`TOLERANCE`].
pub fn fit(scene: Frame, window: Frame, view: Frame) -> Fit {
    if !scene.drawable() {
        return Fit::default();
    }
    let window_target = scene.at_origin();
    let view_target = window_target.at_origin();
    Fit {
        window: (!window.matches(window_target)).then_some(window_target),
        view: (!view.matches(view_target)).then_some(view_target),
    }
}
