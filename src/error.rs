use thiserror::Error;

/// Failures inside the iPadOS platform. GPUI's own trait methods return
/// `anyhow::Result`, so these convert at that boundary and keep their source.
#[derive(Debug, Error)]
pub enum Error {
    /// No Metal device, which every iPad that runs iPadOS 17 has.
    #[error("no Metal device is available")]
    NoMetalDevice,
    /// A Metal shader library or pipeline failed to build.
    #[error("Metal {stage} failed: {message}")]
    Metal {
        stage: &'static str,
        message: String,
    },
    /// The display link could not be created for a screen.
    #[error("CADisplayLink could not be created")]
    DisplayLink,
    /// A window was asked for but the app has no window scene to host it, and
    /// asking the system for one was refused.
    #[error("no window scene is available for a new window")]
    NoScene,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
