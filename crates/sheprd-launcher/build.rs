// Build-time tools (git, gh) run under Cargo, not beside the app: no window to avoid.
#![allow(clippy::disallowed_methods)]

// The launcher carries the same icon (worktree variant included) and "Sheprd"
// version information as herdr-gpui, from herdr-gpui's own build modules.
// `release_version` sits at the root so `crate::release_version` in
// `windows_resource` resolves here exactly as it does there.
#[allow(
    dead_code,
    reason = "only `detect`'s worktree flag and watched paths are used here; the rest serves herdr-gpui"
)]
#[path = "../sheprd-gpui/build_identity.rs"]
mod build_identity;
#[path = "../sheprd-gpui/release_version.rs"]
mod release_version;
#[path = "../sheprd-gpui/windows_resource.rs"]
mod windows_resource;

/// The build script's own failures. Hand-written, as in herdr-gpui's build
/// script, so the script needs no dependency beyond `winresource`.
#[derive(Debug)]
enum BuildError {
    MissingManifestDir,
    WindowsResource(std::io::Error),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingManifestDir => f.write_str("CARGO_MANIFEST_DIR is not set"),
            Self::WindowsResource(_) => {
                f.write_str("could not embed the Windows executable resources")
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WindowsResource(error) => Some(error),
            Self::MissingManifestDir => None,
        }
    }
}

fn main() -> Result<(), BuildError> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../sheprd-gpui/build_identity.rs");
    println!("cargo:rerun-if-changed=../sheprd-gpui/release_version.rs");
    println!("cargo:rerun-if-changed=../sheprd-gpui/windows_resource.rs");
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").ok_or(BuildError::MissingManifestDir)?,
    );
    let identity = build_identity::detect(&manifest);
    windows_resource::embed(&manifest, identity.worktree).map_err(BuildError::WindowsResource)?;
    for path in &identity.watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    Ok(())
}
