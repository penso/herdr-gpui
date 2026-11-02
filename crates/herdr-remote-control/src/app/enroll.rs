//! `herdr-remote-control enroll` and `revoke`, which Herdr UI on another
//! computer runs on this host over SSH, so a phone allowed there reaches
//! this host's daemon directly (`control::hosts`).
//!
//! `enroll` reads one [`Input`] from standard input and prints the host's
//! [`Enrolled`] answer as JSON. On the way it:
//! - saves the APNs key the other computer sent, so this host can notify
//!   the phone too;
//! - makes sure the daemon runs: it installs the user service unless one is
//!   installed already, as Herdr UI on this host would, and turns on
//!   `[remote]` in this host's Herdr UI settings when there are any, so
//!   that app does not stop it again;
//! - asks the running daemon to add the phone, so `devices.json` keeps one
//!   writer.

use super::{
    config::RemoteConfig,
    service::{self, Host, Manager, Outcome},
};
use crate::control::{
    self, ErrorCode, Request, Response,
    hosts::{Enroll, Enrolled},
};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

/// How long `enroll` waits for a daemon it started to answer.
const START_WAIT: Duration = Duration::from_secs(20);
const START_POLL: Duration = Duration::from_millis(250);
/// The largest APNs config or key accepted; both are a few hundred bytes.
const MAX_APNS_FILE: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the APNs files sent are too large")]
    ApnsSize,
    #[error("{} is not valid TOML", path.display())]
    Config {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("this host has no home directory")]
    NoHome,
    #[error("the remote control daemon on this host did not start")]
    NotStarted(#[source] control::Error),
    #[error("the remote control daemon on this host is older than Herdr UI; update it")]
    Outdated,
    #[error("the remote control daemon gave an answer of the wrong kind")]
    Unexpected,
    // keep-sorted end
}

/// What Herdr UI sends `enroll` on standard input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Input {
    #[serde(flatten)]
    pub enroll: Enroll,
    /// The other computer's APNs credentials, for this host to notify the
    /// phone itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apns: Option<Apns>,
}

/// The contents of a daemon's `apns/config.json` and `apns/AuthKey.p8`.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Apns {
    pub config: String,
    pub key: String,
}

impl std::fmt::Debug for Apns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Apns").finish_non_exhaustive()
    }
}

impl Apns {
    /// Reads a daemon's APNs credentials from its state directory; `None`
    /// when it has none. Blocking.
    pub fn read(state_dir: &Path) -> Option<Self> {
        let dir = state_dir.join("apns");
        let read = |name| {
            fs::read_to_string(dir.join(name))
                .ok()
                .filter(|text| text.len() <= MAX_APNS_FILE)
        };
        Some(Self {
            config: read("config.json")?,
            key: read("AuthKey.p8")?,
        })
    }
}

/// The state directory Herdr UI on this host would give the daemon.
pub fn default_state_dir() -> Option<PathBuf> {
    herdr_client::local_path::state_dir().map(|dir| dir.join("remote"))
}

/// Adds the phone `input` names to this host's daemon, starting the daemon
/// first when it does not run. `program` is this executable, which a new
/// service runs. Blocking.
pub fn enroll(input: &Input, state_dir: &Path, program: PathBuf) -> super::Result<Enrolled> {
    let apns_changed = match &input.apns {
        Some(apns) => write_apns(state_dir, apns)?,
        None => false,
    };
    ensure_running(state_dir, program, apns_changed)?;
    let address = control::address(state_dir);
    match control::request(&address, &Request::Enroll(input.enroll.clone())) {
        Ok(Response::Enrolled(enrolled)) => Ok(enrolled),
        Ok(_) => Err(Error::Unexpected.into()),
        // A daemon from before `Enroll` closes without answering.
        Err(control::Error::NoAnswer | control::Error::Frame(_)) => Err(Error::Outdated.into()),
        Err(error) => Err(error.into()),
    }
}

/// Revokes `device` on this host's daemon; one already gone is fine.
/// Blocking.
pub fn revoke(state_dir: &Path, device: control::DeviceId) -> super::Result<()> {
    let address = control::address(state_dir);
    match control::request(&address, &Request::Revoke { device }) {
        Ok(_)
        | Err(control::Error::Daemon {
            code: ErrorCode::UnknownDevice,
            ..
        }) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Saves the APNs files unless this host has the same ones; says whether
/// it wrote, so a running daemon is restarted to read them.
fn write_apns(state_dir: &Path, apns: &Apns) -> super::Result<bool> {
    if apns.config.len() > MAX_APNS_FILE || apns.key.len() > MAX_APNS_FILE {
        return Err(Error::ApnsSize.into());
    }
    if Apns::read(state_dir).as_ref() == Some(apns) {
        return Ok(false);
    }
    let dir = state_dir.join("apns");
    private_dir(state_dir)?;
    private_dir(&dir)?;
    // The key first: a config without its key leaves APNs off, not broken.
    write_private(&dir.join("AuthKey.p8"), apns.key.as_bytes())?;
    write_private(&dir.join("config.json"), apns.config.as_bytes())?;
    Ok(true)
}

fn ensure_running(state_dir: &Path, program: PathBuf, restart: bool) -> super::Result<()> {
    let config = enable_settings()?;
    let manager = Manager::current()?;
    let home = herdr_client::local_path::home_dir().ok_or(Error::NoHome)?;
    let config_home = herdr_client::local_var("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let host = Host {
        file: manager.file(&home, &config_home, state_dir),
        manager,
        state_dir: state_dir.to_owned(),
        program: Some(program.clone()),
        // A daemon from another release than this program is restarted, so
        // the copy Herdr UI just put here is the one serving.
        release: Some(
            option_env!("HERDR_RELEASE_VERSION")
                .unwrap_or(env!("CARGO_PKG_VERSION"))
                .to_owned(),
        ),
    };
    let address = control::address(state_dir);
    let send = |request: &Request| control::request(&address, request);
    // Herdr UI on this host installed its own service: keep it, and only
    // start it, or restart it to read new APNs files.
    let theirs = fs::read_to_string(&host.file)
        .is_ok_and(|text| !text.contains(&*program.to_string_lossy()));
    let outcome = if theirs {
        let steps = if restart {
            host.manager.load(&host.file)
        } else if send(&Request::Status).is_err() {
            host.manager.start(&host.file)
        } else {
            Vec::new()
        };
        for step in &steps {
            service::run(step)?;
        }
        Outcome::Started
    } else {
        service::reconcile(&config, &host, send, service::run)?
    };
    if restart && outcome == Outcome::Unchanged {
        for step in host.manager.load(&host.file) {
            service::run(&step)?;
        }
    }
    wait_for_daemon(&address)
}

/// The `[remote]` settings of Herdr UI on this host, turned on. When that
/// app has a settings file, `enabled = true` is written to it, so it keeps
/// the daemon running rather than stopping it. Blocking.
fn enable_settings() -> super::Result<RemoteConfig> {
    match herdr_client::local_path::config_root() {
        Ok(root) => enable_settings_at(&root.join("herdr/config-gpui.toml")),
        Err(_) => Ok(enabled(RemoteConfig::default())),
    }
}

/// [`enable_settings`] for the settings file at `path`.
fn enable_settings_at(path: &Path) -> super::Result<RemoteConfig> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        remote: RemoteConfig,
    }
    let path = path.to_owned();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(enabled(RemoteConfig::default()));
        }
        Err(source) => {
            return Err(super::Error::Io {
                operation: "read",
                path,
                source,
            });
        }
    };
    let config_error = |source: Box<dyn std::error::Error + Send + Sync>| Error::Config {
        path: path.clone(),
        source,
    };
    let file: File = toml::from_str(&text).map_err(|error| config_error(Box::new(error)))?;
    if file.remote.enabled {
        return Ok(file.remote);
    }
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error| config_error(Box::new(error)))?;
    let (key, value) = super::config::Edit::Enabled(true).entry()?;
    let remote = document
        .entry("remote")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    if let Some(table) = remote.as_table_like_mut() {
        table.insert(key, toml_edit::Item::Value(value));
    }
    fs::write(&path, document.to_string()).map_err(|source| super::Error::Io {
        operation: "write",
        path: path.clone(),
        source,
    })?;
    Ok(enabled(file.remote))
}

fn enabled(config: RemoteConfig) -> RemoteConfig {
    RemoteConfig {
        enabled: true,
        ..config
    }
}

fn wait_for_daemon(address: &Path) -> super::Result<()> {
    let deadline = Instant::now() + START_WAIT;
    loop {
        match control::request(address, &Request::Status) {
            Ok(_) => return Ok(()),
            Err(error) if Instant::now() >= deadline => {
                return Err(Error::NotStarted(error).into());
            }
            Err(_) => thread::sleep(START_POLL),
        }
    }
}

fn private_dir(path: &Path) -> super::Result<()> {
    let failed = |source| super::Error::Io {
        operation: "create",
        path: path.to_owned(),
        source,
    };
    #[cfg(unix)]
    let created = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
    };
    #[cfg(not(unix))]
    let created = fs::create_dir_all(path);
    created.map_err(failed)
}

/// Writes `path` readable by this user alone.
fn write_private(path: &Path, bytes: &[u8]) -> super::Result<()> {
    let failed = |source| super::Error::Io {
        operation: "write",
        path: path.to_owned(),
        source,
    };
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(failed)?;
    io::Write::write_all(&mut file, bytes).map_err(failed)?;
    // A file that existed keeps its old mode through `open`.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(failed)?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
