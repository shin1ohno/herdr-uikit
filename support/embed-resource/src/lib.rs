//! Stands in for crates.io's `embed-resource`, which only GPUI's build script
//! uses, to embed its `gpui.manifest.xml` as a Windows executable's manifest.
//!
//! herdr-uikit never builds for Windows, but Cargo resolves the lockfile for
//! every target, and GPUI's platform crate turns GPUI's `windows-manifest`
//! feature on for Windows builds, so the real crate and its Windows-only
//! dependencies would enter the lockfile. herdr-gpui patches this same no-op
//! in (`crates/embed-resource`), so patching it here keeps this repository's
//! lockfile a subset of herdr-gpui's that resolves offline. A `[patch]` applies
//! only in the root manifest: consumers of herdr-uikit never see this crate.
//! It provides only what GPUI's build script calls.

/// Include directories for the resource compiler; ignored.
pub struct ParamsIncludeDirs<I>(pub I);

/// The result of [`compile`], which never fails.
#[derive(Debug)]
pub struct CompilationResult;

impl CompilationResult {
    /// Always `Ok`: there is nothing to compile.
    ///
    /// # Errors
    ///
    /// Never; the signature matches the crate this one replaces.
    pub fn manifest_required(self) -> Result<(), Self> {
        Ok(())
    }
}

/// Compiles and links nothing.
pub fn compile<T, P>(_resource_file: T, _parameters: P) -> CompilationResult {
    CompilationResult
}
