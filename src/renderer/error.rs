use thiserror::Error;

/// Structured renderer failures, retained until GPUI's reporting boundary.
#[derive(Debug, Error)]
pub enum Error {
    // keep-sorted start
    #[error("Metal returned nil allocating {resource}")]
    Allocation { resource: &'static str },
    #[error("atlas allocation failed")]
    AtlasAllocation,
    #[error("atlas byte count mismatch: expected {expected}, got {actual}")]
    AtlasByteCount { expected: usize, actual: usize },
    #[error("invalid atlas dimensions {width} × {height}")]
    AtlasDimensions { width: i32, height: i32 },
    #[error("atlas texture is missing")]
    AtlasTextureMissing,
    #[error("render pass has no color attachment")]
    ColorAttachmentMissing,
    #[error("rendering scene {stats}: {source}")]
    Frame {
        stats: SceneStats,
        #[source]
        source: Box<Error>,
    },
    #[error("instance buffer alignment {alignment} exceeds 256 bytes")]
    InstanceBufferAlignment { alignment: usize },
    #[error("instance buffer exceeds {limit} bytes ({count} elements of {element_size} bytes)")]
    InstanceBufferLimit {
        count: usize,
        element_size: usize,
        limit: usize,
    },
    #[error("path intermediate texture is missing")]
    IntermediateTextureMissing,
    #[error("no Metal device is available")]
    NoMetalDevice,
    #[error("pipeline build failed for {function}: {message}")]
    PipelineBuild { function: String, message: String },
    #[error("shader library build failed: {message}")]
    ShaderLibrary { message: String },
    #[error("subpixel sprites are unsupported on iOS")]
    SubpixelUnsupported,
    // keep-sorted end
}

/// Counts only: errors must never include terminal or scene contents.
#[derive(Debug)]
pub struct SceneStats {
    paths: usize,
    shadows: usize,
    quads: usize,
    underlines: usize,
    monochrome: usize,
    polychrome: usize,
    surfaces: usize,
}

impl From<&gpui::Scene> for SceneStats {
    fn from(scene: &gpui::Scene) -> Self {
        Self {
            paths: scene.paths.len(),
            shadows: scene.shadows.len(),
            quads: scene.quads.len(),
            underlines: scene.underlines.len(),
            monochrome: scene.monochrome_sprites.len(),
            polychrome: scene.polychrome_sprites.len(),
            surfaces: scene.surfaces.len(),
        }
    }
}

// The integrator owns crate::Error. Until it adds Renderer(#[from] renderer::Error),
// adapt only at the unchanged platform constructor's reporting boundary.
pub(super) fn into_platform_error(error: Error) -> crate::Error {
    match error {
        Error::NoMetalDevice => crate::Error::NoMetalDevice,
        error => crate::Error::Metal {
            stage: "renderer",
            message: error.to_string(),
        },
    }
}

impl std::fmt::Display for SceneStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} paths, {} shadows, {} quads, {} underlines, {} mono, {} poly, {} surfaces",
            self.paths,
            self.shadows,
            self.quads,
            self.underlines,
            self.monochrome,
            self.polychrome,
            self.surfaces
        )
    }
}

#[cfg(test)]
#[path = "tests/errors.rs"]
mod tests;
