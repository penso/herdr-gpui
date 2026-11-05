//! Where this user's files and tools live, for an app started from the
//! desktop rather than a shell: the home directory, the configuration root,
//! Sheprd's state directory, and a `PATH` with the directories a GUI launch
//! leaves out.
use std::{ffi::OsString, path::PathBuf};

// Probing Linuxbrew on macOS triggers /home's automounter and directory lookups.
pub const HOMEBREW_DIRS: &[&str] = if cfg!(target_os = "linux") {
    &[
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/home/linuxbrew/.linuxbrew/bin",
    ]
} else if cfg!(unix) {
    &["/opt/homebrew/bin", "/usr/local/bin"]
} else {
    &[]
};

/// Why the configuration root is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("HOME is not set")]
    MissingHome,
    #[error("XDG_CONFIG_HOME must be an absolute path")]
    RelativeConfigRoot,
}

/// Where configuration lives: `XDG_CONFIG_HOME` (also as the login shell
/// sets it, see [`crate::local_var`]), else `%APPDATA%` on Windows, else
/// `~/.config`. Herdr's own files are under its `herdr` directory.
pub fn config_root() -> Result<PathBuf, PathError> {
    match crate::local_var("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(PathError::RelativeConfigRoot);
            }
            Ok(path)
        }
        None => {
            #[cfg(windows)]
            if let Some(roaming) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
                return Ok(PathBuf::from(roaming));
            }
            let home = home_dir().ok_or(PathError::MissingHome)?;
            #[cfg(windows)]
            return Ok(home.join("AppData").join("Roaming"));
            #[cfg(not(windows))]
            Ok(home.join(".config"))
        }
    }
}

/// This user's home directory: `HOME`, or `USERPROFILE` on Windows. None
/// when neither is set to something non-empty.
pub fn home_dir() -> Option<PathBuf> {
    let variable = |name| std::env::var_os(name).filter(|value: &OsString| !value.is_empty());
    variable("HOME")
        .or_else(|| {
            if cfg!(windows) {
                variable("USERPROFILE")
            } else {
                None
            }
        })
        .map(PathBuf::from)
}

/// The first line of Sheprd's managed `herdr/config-gpui.toml`, which it
/// rewrites at every launch and never reads. A file without it is a
/// personal config from before `config-gpui.local.toml`, which Sheprd moves
/// into that file. `herdr-remote-control enroll` edits the same files.
pub const SETTINGS_MANAGED_HEADER: &str = "# DO NOT EDIT -- WILL BE OVERWRITTEN";

/// What Sheprd's first launch writes to `herdr/config-gpui.local.toml`.
/// Existing overrides and moved personal configs are never rewritten, so
/// settings placed here reach new installs without changing what current
/// users see.
// i18n: data
pub const SETTINGS_OVERRIDES_SEED: &str = "# Sheprd overrides. Saved changes reload automatically.\n# Unset keys inherit config-gpui.toml; tables merge key by key.\n\n# New installs start with the roomy rounded sidebar. Remove this line for\n# the managed default, or pick another layout listed in config-gpui.toml.\nlayout = \"comfortable-rounded\"\n";

/// Sheprd's own state directory, shared by its preferences, logs, and
/// saved devices.
pub fn state_dir() -> Option<PathBuf> {
    state_dir_with(|name| std::env::var_os(name))
}

fn state_dir_with(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name| var(name).filter(|value| !value.is_empty());
    let root = var("XDG_STATE_HOME").map(PathBuf::from).or_else(|| {
        // Existing Windows installations with HOME already store preferences
        // here. Keep selecting the same directory after an upgrade.
        if let Some(home) = var("HOME") {
            return Some(PathBuf::from(home).join(".local").join("state"));
        }
        #[cfg(windows)]
        {
            if let Some(local) = var("LOCALAPPDATA") {
                return Some(PathBuf::from(local));
            }
            if let Some(profile) = var("USERPROFILE") {
                return Some(PathBuf::from(profile).join("AppData").join("Local"));
            }
        }
        None
    });
    root.map(|root| root.join("herdr").join("gpui"))
}

/// GUI launches omit directories commonly used by user-installed tools.
pub fn local_path() -> OsString {
    let mut dirs = Vec::new();
    if let Some(home) = home_dir() {
        dirs.extend(
            [".local/bin", ".cargo/bin", ".bun/bin", ".npm-global/bin"].map(|dir| home.join(dir)),
        );
    }
    dirs.extend(HOMEBREW_DIRS.iter().copied().map(PathBuf::from));
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    dirs.extend(std::env::split_paths(&inherited));
    // launchd's `PATH` stops at `/usr/bin:/bin`; system tools such as `lsof`,
    // which the Tailscale CLI runs on macOS, live in the `sbin` directories.
    if cfg!(unix) {
        for system in ["/usr/sbin", "/sbin"].map(PathBuf::from) {
            if !dirs.contains(&system) {
                dirs.push(system);
            }
        }
    }
    std::env::join_paths(dirs).unwrap_or(inherited)
}

#[cfg(all(test, unix))]
mod tests;
