use thiserror::Error;

/// Why a window could not open on iPadOS.
#[derive(Debug, Error)]
pub(crate) enum Error {
    /// Every iPadOS window is a scene of its own, which the system sizes and
    /// arranges; none floats above another or is modal to it. Refused rather
    /// than opened as an ordinary scene, which the caller did not ask for.
    #[error("iPadOS has no {0} windows")]
    UnsupportedKind(&'static str),
}
