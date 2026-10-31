//! Where a herdr-core session listens, resolved the way `hc` resolves it:
//! `HC_SOCKET`, else `socket` in its config, else the user's runtime folder,
//! else beside the session's state. Windows uses a named pipe per user.

use crate::{Error, Result};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// One core session's endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The Unix socket, or the Windows pipe name.
    pub socket: PathBuf,
    /// `HC_SESSION`, `default` when unset.
    pub session: String,
}

/// What resolution reads from the machine, kept apart so tests need none.
pub(crate) struct Inputs<'a> {
    /// `/run/user/<uid>` when that folder exists. Windows names a pipe
    /// instead, so it has no such folder.
    #[cfg(unix)]
    pub run_user: Option<PathBuf>,
    /// The contents of herdr-core's `config.toml`, when it can be read.
    pub config: &'a dyn Fn(&Path) -> Option<String>,
}

impl Endpoint {
    /// The endpoint in an environment read through `var`, such as the
    /// login shell's. Blocking: it checks folders and reads the config.
    pub fn resolve(var: impl Fn(&str) -> Option<OsString>) -> Result<Self> {
        let inputs = Inputs {
            #[cfg(unix)]
            run_user: run_user(),
            config: &|path| std::fs::read_to_string(path).ok(),
        };
        Self::resolve_with(&var, &inputs)
    }

    pub(crate) fn resolve_with(
        var: &dyn Fn(&str) -> Option<OsString>,
        inputs: &Inputs<'_>,
    ) -> Result<Self> {
        let set = |name: &str| var(name).filter(|value| !value.is_empty());
        let session = session(set("HC_SESSION"))?;
        let (state_home, config_home) = data_dirs(&set)?;
        let state_home = set("XDG_STATE_HOME").map_or(state_home, PathBuf::from);
        let config_home = set("XDG_CONFIG_HOME").map_or(config_home, PathBuf::from);
        let state = state_home.join("herdr-core").join(&session);
        let config = config_home.join("herdr-core").join("config.toml");
        let chosen = set("HC_SOCKET")
            .map(PathBuf::from)
            .or_else(|| configured_socket(&(inputs.config)(&config)?));
        let socket = match chosen {
            Some(socket) => socket,
            None => default_socket(&set, inputs, &state, &session),
        };
        Ok(Self { socket, session })
    }
}

#[derive(serde::Deserialize)]
struct Config {
    socket: Option<String>,
}

/// `socket` in herdr-core's config; a config `hc` cannot read is ignored
/// there too.
fn configured_socket(text: &str) -> Option<PathBuf> {
    toml::from_str::<Config>(text)
        .ok()?
        .socket
        .map(PathBuf::from)
}

fn session(raw: Option<OsString>) -> Result<String> {
    let raw = raw.map(|raw| raw.to_string_lossy().into_owned());
    let name = raw
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("default");
    if name == "."
        || name == ".."
        || name
            .chars()
            .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c))
    {
        return Err(Error::Session(name.to_owned()));
    }
    Ok(name.to_owned())
}

#[cfg(unix)]
fn data_dirs(set: &dyn Fn(&str) -> Option<OsString>) -> Result<(PathBuf, PathBuf)> {
    let home = set("HOME").map(PathBuf::from).ok_or(Error::NoHome)?;
    Ok((home.join(".local/state"), home.join(".config")))
}

#[cfg(windows)]
fn data_dirs(set: &dyn Fn(&str) -> Option<OsString>) -> Result<(PathBuf, PathBuf)> {
    let profile = set("USERPROFILE").map(PathBuf::from);
    let under_profile = |dir: &str| {
        profile
            .as_ref()
            .map(|profile| profile.join(dir))
            .ok_or(Error::NoHome)
    };
    let local = set("LOCALAPPDATA")
        .map_or_else(|| under_profile(r"AppData\Local"), |dir| Ok(dir.into()))?;
    let roaming =
        set("APPDATA").map_or_else(|| under_profile(r"AppData\Roaming"), |dir| Ok(dir.into()))?;
    Ok((local, roaming))
}

#[cfg(unix)]
fn default_socket(
    set: &dyn Fn(&str) -> Option<OsString>,
    inputs: &Inputs<'_>,
    state: &Path,
    session: &str,
) -> PathBuf {
    let name = format!("{session}.sock");
    // Logins without XDG_RUNTIME_DIR (some SSH setups) still find the core.
    match set("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| inputs.run_user.clone())
    {
        Some(run) => run.join("herdr-core").join(name),
        None => state.join(name),
    }
}

#[cfg(windows)]
fn default_socket(
    set: &dyn Fn(&str) -> Option<OsString>,
    _inputs: &Inputs<'_>,
    _state: &Path,
    session: &str,
) -> PathBuf {
    let user: String = set("USERNAME")
        .map(|user| {
            user.to_string_lossy()
                .chars()
                .filter(char::is_ascii_alphanumeric)
                .collect()
        })
        .unwrap_or_default();
    PathBuf::from(format!(r"\\.\pipe\herdr-core-{user}-{session}.sock"))
}

#[cfg(unix)]
fn run_user() -> Option<PathBuf> {
    let dir = PathBuf::from(format!("/run/user/{}", rustix::process::getuid().as_raw()));
    dir.is_dir().then_some(dir)
}
