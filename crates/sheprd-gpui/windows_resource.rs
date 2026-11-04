//! Embeds the Windows executable's resources: the Herdr icon as icon resource
//! 1, which GPUI's Windows platform loads for the window class and Explorer and
//! the taskbar show, the version information Windows shows for the file, and
//! the application manifest (`windows.manifest`).

use std::{
    io::{Error, ErrorKind},
    path::Path,
};

use crate::release_version;
use winresource::{VersionInfo, WindowsResource};

const ICON: &str = "assets/icons/Sheprd.ico";
const WORKTREE_ICON: &str = "assets/icons/Sheprd-worktree.ico";
/// Windows shows FileDescription as the application name, as in Task Manager.
const APP_NAME: &str = "Sheprd";
/// The version shown for builds outside the release pipeline, as in the app.
const DEV_VERSION: &str = "dev";
/// The application manifest: per-monitor DPI, long paths, the UTF-8 code
/// page, the supported Windows versions, and no elevation. It replaces GPUI's own manifest, which
/// `crates/embed-resource` keeps GPUI's build script from embedding: an
/// executable holds one manifest.
const MANIFEST: &str = "crates/sheprd-gpui/windows.manifest";

/// Embeds the resources when the build targets Windows; does nothing otherwise.
///
/// Reads the target from `CARGO_CFG_TARGET_OS` rather than `cfg!(windows)`,
/// which describes the host running this script, so cross-compiled Windows
/// builds get the resources too.
pub fn embed(manifest: &Path, worktree: bool) -> std::io::Result<()> {
    if std::env::var_os("CARGO_CFG_TARGET_OS").is_none_or(|os| os != "windows") {
        return Ok(());
    }
    println!("cargo:rerun-if-env-changed=HERDR_RELEASE_VERSION");
    let workspace = manifest.join("../..");
    for file in [ICON, WORKTREE_ICON, MANIFEST] {
        println!("cargo:rerun-if-changed={}", workspace.join(file).display());
    }
    let icon = workspace.join(if worktree { WORKTREE_ICON } else { ICON });
    let icon = icon.to_str().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "the icon path is not valid Unicode",
        )
    })?;
    // A file rather than `set_manifest`'s string, which the resource script
    // would carry with its line breaks turned into spaces.
    let manifest = workspace.join(MANIFEST);
    let manifest = manifest.to_str().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            "the manifest path is not valid Unicode",
        )
    })?;
    let (version, numeric) = version()?;
    let mut resource = WindowsResource::new();
    // `set_icon` names the icon 1, the resource GPUI's `LoadImageW` asks for.
    resource
        .set_icon(icon)
        .set_manifest_file(manifest)
        .set("FileDescription", APP_NAME)
        .set("ProductName", APP_NAME)
        .set("FileVersion", &version)
        .set("ProductVersion", &version)
        .set_version_info(VersionInfo::FILEVERSION, numeric)
        .set_version_info(VersionInfo::PRODUCTVERSION, numeric);
    resource.compile()
}

/// The release version and its packed numeric form, or `dev` and 0 outside a
/// release. Cargo's package version never reaches users, so it is not used.
fn version() -> std::io::Result<(String, u64)> {
    let Some(version) = std::env::var_os("HERDR_RELEASE_VERSION") else {
        return Ok((DEV_VERSION.to_owned(), 0));
    };
    let malformed = |shown: &str| {
        Error::new(
            ErrorKind::InvalidInput,
            format!(
                "HERDR_RELEASE_VERSION {shown:?} is not YYYYMMDD.COUNTER with each part at most 65535"
            ),
        )
    };
    let version = version
        .into_string()
        .map_err(|version| malformed(&version.to_string_lossy()))?;
    let numeric = release_version::numeric(&version).ok_or_else(|| malformed(&version))?;
    Ok((version, numeric))
}
