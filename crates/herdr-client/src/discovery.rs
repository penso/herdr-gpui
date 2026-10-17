//! Local socket rules adapted from herdr src/server/socket_paths.rs and
//! src/session.rs (Apache-2.0; see ../../herdr-protocol/NOTICE.md).
//! Modified: explicit release/dev selection, no server spawning or global state.
use crate::{Error, Result};
use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::OnceLock,
};

static LOGIN_CONFIG_HOME: OnceLock<PathBuf> = OnceLock::new();

/// Sets, once per process, the `XDG_CONFIG_HOME` of the user's login shell,
/// for a process whose own environment has none. An application opened from
/// Finder or a desktop launcher inherits the session's environment rather
/// than the shell's, while a daemon started from that shell keeps its sockets
/// under the shell's root. Only an absolute path is kept: a relative one
/// would resolve against this process's directory, not the shell's. Returns
/// `false` when it is refused or one was already set, which is kept.
pub fn set_login_config_home(path: PathBuf) -> bool {
    path.is_absolute() && LOGIN_CONFIG_HOME.set(path).is_ok()
}

/// A variable of this process's environment for resolving local paths. An
/// unset `XDG_CONFIG_HOME` falls back to [`set_login_config_home`]'s, so the
/// sockets, and an application's settings beside them, are where the daemon
/// keeps them.
pub fn local_var(name: &str) -> Option<OsString> {
    with_login_config_home(name, |name| env::var_os(name), LOGIN_CONFIG_HOME.get())
}

fn with_login_config_home(
    name: &str,
    var: impl Fn(&str) -> Option<OsString>,
    login: Option<&PathBuf>,
) -> Option<OsString> {
    var(name).or_else(|| {
        (name == "XDG_CONFIG_HOME")
            .then_some(login)
            .flatten()
            .map(Into::into)
    })
}

/// Upstream's configuration root, before the `herdr`/`herdr-dev` directory.
/// Windows has no XDG layout by default, so upstream falls back to `%APPDATA%`
/// there; matching that order is what makes both ends dial the same endpoint.
fn config_root(var: &impl Fn(&str) -> Option<OsString>) -> PathBuf {
    if let Some(dir) = var("XDG_CONFIG_HOME") {
        return dir.into();
    }
    #[cfg(windows)]
    {
        if let Some(dir) = var("APPDATA") {
            return dir.into();
        }
        if let Some(profile) = var("USERPROFILE") {
            return PathBuf::from(profile).join("AppData").join("Roaming");
        }
    }
    var("HOME")
        .map(|home| PathBuf::from(home).join(".config"))
        .unwrap_or_else(env::temp_dir)
}

/// A provider that creates machines used as endpoints. Closed: supporting
/// another provider adds a variant behind its own feature, and every match
/// that cares says so.
#[cfg(feature = "cloud")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    #[cfg(feature = "azure")]
    Azure,
    #[cfg(feature = "boat")]
    Boat,
    #[cfg(feature = "coder")]
    Coder,
    #[cfg(feature = "daytona")]
    Daytona,
    #[cfg(feature = "railway")]
    Railway,
}

#[cfg(feature = "cloud")]
impl CloudProvider {
    /// Every provider this build supports, in display order.
    pub const ALL: &[Self] = &[
        #[cfg(feature = "coder")]
        Self::Coder,
        #[cfg(feature = "daytona")]
        Self::Daytona,
        #[cfg(feature = "railway")]
        Self::Railway,
        #[cfg(feature = "boat")]
        Self::Boat,
        #[cfg(feature = "azure")]
        Self::Azure,
    ];

    /// The stable key used in saved files and endpoint IDs.
    pub fn key(self) -> &'static str {
        match self {
            #[cfg(feature = "azure")]
            Self::Azure => "azure",
            #[cfg(feature = "boat")]
            Self::Boat => "boat",
            #[cfg(feature = "coder")]
            Self::Coder => "coder",
            #[cfg(feature = "daytona")]
            Self::Daytona => "daytona",
            #[cfg(feature = "railway")]
            Self::Railway => "railway",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConnectTarget {
    /// Environment overrides, then HERDR_SESSION in the release config directory.
    #[default]
    Local,
    /// Explicit session bypasses socket overrides; "default" selects the root.
    Session { name: String, development: bool },
    /// Exact client protocol socket, not the JSON API socket.
    Socket(PathBuf),
    /// Noninteractive SSH attachment to an installed remote Herdr (POSIX hosts).
    Ssh { target: String, session: String },
    /// A machine a cloud provider created, reached through the application's
    /// connector, which holds the provider's credential and builds the command
    /// that runs the bridge there. `account` names the provider account or
    /// deployment (for Railway, the path of the SSH key); `id` is the
    /// provider's stable ID for the machine, which the connector reaches it by,
    /// and `machine` its name when it was saved, for display: a machine can be
    /// renamed, or deleted and its name reused.
    #[cfg(feature = "cloud")]
    Cloud {
        provider: CloudProvider,
        account: String,
        id: String,
        machine: String,
        session: String,
    },
    /// A WSL distribution on this Windows machine, attached through `wsl.exe`
    /// running the same bridge an SSH host runs.
    Wsl { distro: String, session: String },
}

/// Whether a name may become a session directory. Both ends derive the same
/// path from it, so a name that escapes the configuration root is refused.
pub(crate) fn valid_session_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !matches!(name, "." | "..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn app_dir(development: bool) -> &'static str {
    if development { "herdr-dev" } else { "herdr" }
}

/// The directory a release or development installation keeps its sessions in.
pub(crate) fn config_dir(development: bool) -> PathBuf {
    config_root(&local_var).join(app_dir(development))
}

pub fn session_socket(config_dir: &Path, name: &str) -> Result<PathBuf> {
    if !valid_session_name(name) {
        return Err(Error::InvalidSession);
    }
    Ok(if name == "default" {
        config_dir.to_owned()
    } else {
        config_dir.join("sessions").join(name)
    }
    .join("herdr-client.sock"))
}

impl ConnectTarget {
    /// Whether the daemon runs on another machine, on a cloud provider's
    /// machine, or inside a WSL distribution, so its paths, processes, and files are not
    /// this machine's.
    pub fn is_remote(&self) -> bool {
        match self {
            Self::Ssh { .. } | Self::Wsl { .. } => true,
            #[cfg(feature = "cloud")]
            Self::Cloud { .. } => true,
            _ => false,
        }
    }

    /// The session a remote target attaches to.
    pub fn remote_session(&self) -> Option<&str> {
        match self {
            Self::Ssh { session, .. } | Self::Wsl { session, .. } => Some(session),
            #[cfg(feature = "cloud")]
            Self::Cloud { session, .. } => Some(session),
            _ => None,
        }
    }

    pub fn socket_path(&self) -> Result<PathBuf> {
        self.socket_path_with(local_var)
    }

    /// Standard session endpoint, ignoring socket overrides (including `Socket`).
    /// This identifies a user-configured local location, not a daemon executable.
    pub fn local_session_socket_path(&self) -> Result<PathBuf> {
        self.local_session_socket_path_with(local_var)
    }

    fn local_session_socket_path_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<PathBuf> {
        let target = if matches!(self, Self::Socket(_)) {
            &Self::Local
        } else {
            self
        };
        target.socket_path_with(|name| match name {
            "HERDR_SOCKET_PATH" | "HERDR_CLIENT_SOCKET_PATH" => None,
            _ => var(name),
        })
    }

    fn socket_path_with(&self, var: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
        if self.is_remote() {
            return Err(Error::NoLocalSocket);
        }
        if let Self::Socket(path) = self {
            return Ok(path.clone());
        }
        if matches!(self, Self::Local) {
            if let Some(path) = var("HERDR_SOCKET_PATH") {
                let path = PathBuf::from(path);
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("herdr");
                return Ok(path
                    .parent()
                    .unwrap_or(Path::new(""))
                    .join(format!("{stem}-client.sock")));
            }
            if let Some(path) = var("HERDR_CLIENT_SOCKET_PATH") {
                return Ok(path.into());
            }
        }
        let development = matches!(
            self,
            Self::Session {
                development: true,
                ..
            }
        );
        let base = config_root(&var).join(app_dir(development));
        let name = match self {
            Self::Session { name, .. } => name.clone(),
            _ => var("HERDR_SESSION")
                .and_then(|name| name.into_string().ok())
                .unwrap_or_else(|| "default".into()),
        };
        session_socket(&base, &name)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
