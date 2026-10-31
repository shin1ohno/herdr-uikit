use thiserror::Error;

/// Why a `gpui::Platform` request could not be carried out on iPadOS. GPUI's
/// trait methods return `anyhow::Result`, so these convert at that boundary.
#[derive(Debug, Error)]
pub(crate) enum Error {
    /// iPadOS apps cannot start processes, so they ship no helper executables.
    #[error("iPadOS apps cannot run auxiliary executables")]
    AuxiliaryExecutable,
    /// GPUI's credential store is not wired to the Keychain on iPadOS;
    /// `herdr-credentials` keeps Herdr's sign-ins there instead.
    #[error("GPUI's credential store is not available on iPadOS")]
    Credentials,
    /// UIKit is only touched from the main thread, and GPUI called the
    /// platform from another one.
    #[error("the iPadOS platform was used off the main thread")]
    NotMainThread,
    /// There are no file or folder pickers for paths on the daemon's computer.
    #[error("choosing files and folders is not available on iPad")]
    PathPrompt,
    /// URL schemes are declared in `Info.plist` and cannot be registered at
    /// run time.
    #[error("iPadOS apps declare URL schemes in Info.plist and cannot register them at run time")]
    UrlSchemeRegistration,
}
