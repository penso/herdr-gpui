// Build-time tools (git, gh) run under Cargo, not beside the app: no window to avoid.
#![allow(clippy::disallowed_methods)]

mod build_identity;
mod locales;
mod release_version;
mod windows_resource;

/// The build script's own failures, as a typed error rather than the `&str`
/// Cargo accepts here. Hand-written rather than derived: `tests/build_identity`
/// recompiles this script in an isolated offline crate, so it must keep
/// building with no dependencies of its own.
#[derive(Debug)]
enum BuildError {
    MissingManifestDir,
    MissingOutDir,
    Locales(locales::Error),
    InvalidPrNumber,
    PrNumberEncoding(std::env::VarError),
    MockupFileEncoding,
    InvalidMockupFile,
    WindowsResource(std::io::Error),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingManifestDir => f.write_str("CARGO_MANIFEST_DIR is not set"),
            Self::MissingOutDir => f.write_str("OUT_DIR is not set"),
            Self::Locales(error) => write!(f, "invalid messages: {error}"),
            Self::InvalidPrNumber => {
                f.write_str("HERDR_BUILD_PR_NUMBER must be empty or a positive decimal integer")
            }
            Self::PrNumberEncoding(_) => f.write_str("HERDR_BUILD_PR_NUMBER is not valid Unicode"),
            Self::MockupFileEncoding => f.write_str("HERDR_MOCKUP_FILE is not valid Unicode"),
            Self::InvalidMockupFile => {
                f.write_str("HERDR_MOCKUP_FILE must be an absolute path on one line")
            }
            Self::WindowsResource(_) => {
                f.write_str("could not embed the Windows executable resources")
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PrNumberEncoding(error) => Some(error),
            Self::WindowsResource(error) => Some(error),
            Self::Locales(error) => Some(error),
            Self::MissingManifestDir
            | Self::MissingOutDir
            | Self::InvalidPrNumber
            | Self::MockupFileEncoding
            | Self::InvalidMockupFile => None,
        }
    }
}

fn main() -> Result<(), BuildError> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_identity.rs");
    println!("cargo:rerun-if-changed=locales.rs");
    println!("cargo:rerun-if-changed=release_version.rs");
    println!("cargo:rerun-if-changed=windows_resource.rs");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_PR_NUMBER");
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").ok_or(BuildError::MissingManifestDir)?,
    );
    let out =
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or(BuildError::MissingOutDir)?);
    locales::generate(&manifest, &out).map_err(BuildError::Locales)?;
    let identity = build_identity::detect(&manifest);
    windows_resource::embed(&manifest, identity.worktree).map_err(BuildError::WindowsResource)?;
    for path in &identity.watched {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let pr = match std::env::var("HERDR_BUILD_PR_NUMBER") {
        Ok(value) => build_identity::validate_pr(&value)
            .ok_or(BuildError::InvalidPrNumber)?
            .to_owned(),
        Err(std::env::VarError::NotPresent) if identity.worktree && !identity.detached => {
            build_identity::lookup_pr(
                &manifest,
                &identity.branch,
                std::path::Path::new("gh"),
                std::time::Duration::from_secs(2),
            )
            .unwrap_or_default()
        }
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(error) => return Err(BuildError::PrNumberEncoding(error)),
    };
    println!(
        "cargo:rustc-env=HERDR_BUILD_WORKTREE={}",
        u8::from(identity.worktree)
    );
    println!("cargo:rustc-env=HERDR_BUILD_BRANCH={}", identity.branch);
    println!("cargo:rustc-env=HERDR_BUILD_PR={pr}");
    println!("cargo:rustc-env=HERDR_BUILD_COMMIT={}", identity.commit);
    if std::env::var_os("CARGO_FEATURE_MOCKUP").is_some() {
        mockup_scratch()?;
    }
    Ok(())
}

/// Points the `mockup` feature at the variants file an agent wrote, which
/// lives outside the repository. The crate `include!`s it by absolute path, so
/// compiler errors name the real file and rustc tracks it for rebuilds.
/// Without one, the mode shows its built-in demo.
fn mockup_scratch() -> Result<(), BuildError> {
    println!("cargo:rerun-if-env-changed=HERDR_MOCKUP_FILE");
    println!("cargo:rustc-check-cfg=cfg(herdr_mockup_scratch)");
    let Some(file) = std::env::var_os("HERDR_MOCKUP_FILE").filter(|file| !file.is_empty()) else {
        return Ok(());
    };
    let file = file
        .into_string()
        .map_err(|_| BuildError::MockupFileEncoding)?;
    if !std::path::Path::new(&file).is_absolute() || file.contains(['\n', '\r']) {
        return Err(BuildError::InvalidMockupFile);
    }
    println!("cargo:rerun-if-changed={file}");
    println!("cargo:rustc-cfg=herdr_mockup_scratch");
    println!("cargo:rustc-env=HERDR_MOCKUP_SCRATCH={file}");
    Ok(())
}
