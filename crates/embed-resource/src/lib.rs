//! Stands in for crates.io's `embed-resource`, which only GPUI's build script
//! uses, to embed its `gpui.manifest.xml` as the executable's manifest.
//!
//! Herdr's executables embed their own manifest instead
//! (`crates/sheprd-gpui/windows.manifest`, through `winresource`), and an
//! executable holds one: a second manifest resource fails the link with
//! CVT1100. GPUI's platform crate turns GPUI's `windows-manifest` feature on
//! for every Windows build, so the feature cannot be switched off; the
//! workspace patches this crate in, which compiles nothing. It provides only
//! what GPUI's build script calls.

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

/// Compiles and links nothing, so the executable keeps Herdr's manifest.
pub fn compile<T, P>(_resource_file: T, _parameters: P) -> CompilationResult {
    CompilationResult
}
