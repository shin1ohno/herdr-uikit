//! Adapted from gpui-pre-apple 0.3.6 (zed@bcf6582), Apache-2.0; see NOTICE.md.
//! Source: metal_renderer.rs. Modified for UIKit, checked allocation, and iOS storage.
use super::*;
pub(super) type PointF = Point<f32>;
#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct PathRasterizationVertex {
    pub xy_position: Point<ScaledPixels>,
    pub st_position: Point<f32>,
    pub color: Background,
    pub bounds: Bounds<ScaledPixels>,
}

#[repr(C)]
pub(super) enum ShadowInputIndex {
    Vertices = 0,
    Shadows = 1,
    ViewportSize = 2,
}

#[repr(C)]
pub(super) enum QuadInputIndex {
    Vertices = 0,
    Quads = 1,
    ViewportSize = 2,
}

#[repr(C)]
pub(super) enum UnderlineInputIndex {
    Vertices = 0,
    Underlines = 1,
    ViewportSize = 2,
}

#[repr(C)]
pub(super) enum SpriteInputIndex {
    Vertices = 0,
    Sprites = 1,
    ViewportSize = 2,
    AtlasTextureSize = 3,
    AtlasTexture = 4,
}

#[repr(C)]
pub(super) enum PathRasterizationInputIndex {
    Vertices = 0,
    ViewportSize = 1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct PathSprite {
    pub bounds: Bounds<ScaledPixels>,
}
