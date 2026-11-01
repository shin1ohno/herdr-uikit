use thiserror::Error;

/// Failures inside the iPadOS platform. GPUI's own trait methods return
/// `anyhow::Result`, so these convert at that boundary and keep their source.
#[derive(Debug, Error)]
pub enum Error {
    /// The display link could not be created for a screen.
    #[error("CADisplayLink could not be created")]
    DisplayLink,
    /// A window was asked for but the app has no window scene to host it, and
    /// asking the system for one was refused.
    #[error("no window scene is available for a new window")]
    NoScene,
    /// UIKit state was changed from a thread other than the main thread.
    #[error("the iPadOS platform was used off the main thread")]
    NotMainThread,
    #[error(transparent)]
    Renderer(#[from] crate::renderer::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
