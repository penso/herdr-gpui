//! `herdr-remote-control enroll` and `revoke`, which Sheprd on another
//! computer runs on this host over SSH, so a phone allowed there reaches
//! this host's daemon directly (`control::hosts`).
//!
//! `enroll` reads one [`Input`] from standard input and prints the host's
//! [`Enrolled`] answer as JSON. On the way it:
//! - saves the APNs key the other computer sent, so this host can notify
//!   the phone too;
//! - reads this host's Sheprd settings, refusing ones it could not change;
//! - makes sure the daemon runs: it installs the user service unless one is
//!   installed already, as Sheprd on this host would;
//! - asks the running daemon to add the phone, so `devices.json` keeps one
//!   writer;
//! - turns on `[remote]` in those settings, creating them when there are
//!   none, so that app, now or once installed, does not stop the daemon.
//!
//! The settings come last on purpose. A running Sheprd follows them and
//! rewrites the service for its own program, restarting the daemon, which
//! must not happen under the steps above; and a pairing that failed must
//! not leave remote control turned on for a later install.

use super::{
    config::RemoteConfig,
    service::{self, Host, Manager, Outcome},
};
use crate::control::{
    self, ErrorCode, Request, Response,
    hosts::{Enroll, Enrolled},
};
use serde::{Deserialize, Serialize};
use sheprd_client::settings_files::{self, State};
use std::{
    fs, io,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

/// How long `enroll` waits for a daemon it started to answer.
const START_WAIT: Duration = Duration::from_secs(20);
const START_POLL: Duration = Duration::from_millis(250);
/// How long `enroll` waits for Sheprd to finish a save of its settings.
/// With [`START_WAIT`], well inside the idle limit of the SSH job.
const SETTINGS_LOCK_WAIT: Duration = Duration::from_secs(10);
const SETTINGS_LOCK_POLL: Duration = Duration::from_millis(50);
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
    #[error("the remote control daemon on this host is older than Sheprd; update it")]
    Outdated,
    #[error("Sheprd on this host kept {} locked", path.display())]
    SettingsLocked { path: PathBuf },
    #[error("the remote control daemon gave an answer of the wrong kind")]
    Unexpected,
    // keep-sorted end
}

/// What Sheprd sends `enroll` on standard input.
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

/// The state directory Sheprd on this host would give the daemon.
pub fn default_state_dir() -> Option<PathBuf> {
    sheprd_client::local_path::state_dir().map(|dir| dir.join("remote"))
}

/// Adds the phone `input` names to this host's daemon, starting the daemon
/// first when it does not run. `program` is this executable, which a new
/// service runs. Blocking.
pub fn enroll(input: &Input, state_dir: &Path, program: PathBuf) -> super::Result<Enrolled> {
    let apns_changed = match &input.apns {
        Some(apns) => write_apns(state_dir, apns)?,
        None => false,
    };
    let settings = settings_path();
    enroll_then_keep(
        settings.as_deref(),
        state_dir,
        |config| ensure_running(config, state_dir, program, apns_changed),
        || add_phone(input, state_dir),
    )
}

/// `start`s the daemon with the settings at `path` and `add`s the phone,
/// then turns those settings on. Settings that cannot be changed stop it
/// before anything starts.
fn enroll_then_keep(
    path: Option<&Path>,
    state_dir: &Path,
    start: impl FnOnce(&RemoteConfig) -> super::Result<()>,
    add: impl FnOnce() -> super::Result<Enrolled>,
) -> super::Result<Enrolled> {
    let plan = path.map(plan_settings).transpose()?;
    match &plan {
        Some(plan) => start(&plan.config)?,
        None => start(&enabled(RemoteConfig::default()))?,
    }
    let enrolled = add()?;
    if let (Some(path), Some(plan)) = (path, plan) {
        keep_settings(path, plan.state, state_dir, &enrolled)?;
    }
    Ok(enrolled)
}

fn add_phone(input: &Input, state_dir: &Path) -> super::Result<Enrolled> {
    let address = control::address(state_dir);
    let answer = control::request(&address, &Request::Enroll(input.enroll.clone()));
    read_answer(input, state_dir, answer)
}

/// What the daemon's `answer` to adding the phone means.
fn read_answer(
    input: &Input,
    state_dir: &Path,
    answer: Result<Response, control::Error>,
) -> super::Result<Enrolled> {
    match answer {
        Ok(Response::Enrolled(enrolled)) if enrolled.app == input.enroll.app => Ok(enrolled),
        // A daemon from before web apps took the phone, but it would refuse
        // the app holding the key, so the pairing fails rather than look
        // paired and never connect.
        Ok(Response::Enrolled(enrolled)) => {
            if let Err(error) = revoke(state_dir, enrolled.device) {
                tracing::warn!(%error, "Could not remove a phone whose pairing failed");
            }
            Err(Error::Outdated.into())
        }
        Ok(_) => Err(Error::Unexpected.into()),
        // A daemon from before `Enroll` closes without answering.
        Err(control::Error::NoAnswer | control::Error::Frame(_)) => Err(Error::Outdated.into()),
        Err(error) => Err(error.into()),
    }
}

/// Turns on the settings at `path` for the phone just `enrolled`. A host
/// with no Sheprd files has no app to stop the daemon, so failing there
/// costs only a later install and the pairing stands. With Sheprd here the
/// daemon would not last: the pairing fails, and the phone, told so, is
/// removed again rather than left behind.
fn keep_settings(
    path: &Path,
    state: State,
    state_dir: &Path,
    enrolled: &Enrolled,
) -> super::Result<()> {
    let Err(error) = enable_settings_at(path, SETTINGS_LOCK_WAIT) else {
        return Ok(());
    };
    if state == State::Fresh {
        tracing::warn!(%error, "Could not turn on remote control in new Sheprd settings");
        return Ok(());
    }
    if let Err(error) = revoke(state_dir, enrolled.device.clone()) {
        tracing::warn!(%error, "Could not remove a phone whose pairing failed");
    }
    Err(error)
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

fn ensure_running(
    config: &RemoteConfig,
    state_dir: &Path,
    program: PathBuf,
    restart: bool,
) -> super::Result<()> {
    let manager = Manager::current()?;
    let home = sheprd_client::local_path::home_dir().ok_or(Error::NoHome)?;
    let config_home = sheprd_client::local_var("XDG_CONFIG_HOME")
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
        // the copy Sheprd just put here is the one serving.
        release: Some(
            option_env!("HERDR_RELEASE_VERSION")
                .unwrap_or(env!("CARGO_PKG_VERSION"))
                .to_owned(),
        ),
    };
    let address = control::address(state_dir);
    let send = |request: &Request| control::request(&address, request);
    // Sheprd on this host installed its own service: keep it, and only
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
        service::reconcile(config, &host, send, service::run)?
    };
    if restart && outcome == Outcome::Unchanged {
        for step in host.manager.load(&host.file) {
            service::run(&step)?;
        }
    }
    wait_for_daemon(&address)
}

/// Sheprd's settings on this host, `None` when it has no configuration
/// root.
fn settings_path() -> Option<PathBuf> {
    let root = sheprd_client::local_path::config_root().ok()?;
    Some(root.join("herdr/config-gpui.toml"))
}

/// What turning on the settings at a path takes, worked out before any of
/// it is done, so every file is checked before one is written.
#[derive(Debug)]
struct SettingsPlan {
    /// `[remote]` as those settings set it, turned on.
    config: RemoteConfig,
    state: State,
    changes: Vec<SettingsChange>,
}

#[derive(Debug)]
enum SettingsChange {
    Create(PathBuf, String),
    Remove(PathBuf),
    Replace(PathBuf, String),
}

impl SettingsChange {
    fn apply(&self) -> super::Result<()> {
        match self {
            Self::Create(path, text) => {
                settings_files::create(path, text).map_err(io_error("create", path))
            }
            Self::Remove(path) => fs::remove_file(path).map_err(io_error("remove", path)),
            Self::Replace(path, text) => {
                settings_files::replace(path, text).map_err(io_error("write", path))
            }
        }
    }
}

/// [`SettingsPlan`] for Sheprd's settings at `path`, as Sheprd reads them:
/// it rewrites a managed `path` and loads its overrides beside it. Those
/// take the change, seeded as Sheprd's first launch would seed them when
/// missing. A personal `path` from before overrides, which Sheprd moves
/// into them, takes it instead. Writes nothing. Blocking.
fn plan_settings(path: &Path) -> super::Result<SettingsPlan> {
    let local = settings_files::overrides_path(path);
    let original = settings_files::read(path).map_err(io_error("read", path))?;
    let personal = original.as_deref().and_then(settings_files::personal);
    let overrides = settings_files::read(&local).map_err(io_error("read", &local))?;
    let state = State::of(personal, overrides.as_deref());
    let personal_on = personal.map(|text| turned_on(path, text)).transpose()?;
    let overrides_on = overrides
        .as_deref()
        .map(|text| turned_on(&local, text))
        .transpose()?;
    let replace = |path: &Path, text| SettingsChange::Replace(path.to_owned(), text);
    let (config, changes) = match (personal_on, overrides_on) {
        (Some((config, changed)), None) => (
            config,
            changed
                .map(|text| replace(path, text))
                .into_iter()
                .collect(),
        ),
        // A copy of `path` Sheprd made and has yet to finish moving. It
        // matches byte for byte, so it is dropped and Sheprd moves `path`
        // again, with the change: stopping between the two steps leaves a
        // move Sheprd starts over, never two files that differ.
        (Some((config, changed)), Some(_)) if state == State::Moving => {
            let changes = match changed {
                Some(text) => vec![SettingsChange::Remove(local), replace(path, text)],
                None => Vec::new(),
            };
            (config, changes)
        }
        // Overrides, and maybe a personal file that differs from them,
        // which Sheprd leaves for a person to settle: both take the change.
        (personal_on, Some((config, changed))) => {
            let personal_changed = personal_on.and_then(|(_, changed)| changed);
            let changes = changed
                .map(|text| replace(&local, text))
                .into_iter()
                .chain(personal_changed.map(|text| replace(path, text)))
                .collect();
            (config, changes)
        }
        (None, None) => {
            let (config, changed) = turned_on(&local, settings_files::OVERRIDES_SEED)?;
            let text = changed.unwrap_or_else(|| settings_files::OVERRIDES_SEED.to_owned());
            (config, vec![SettingsChange::Create(local, text)])
        }
    };
    Ok(SettingsPlan {
        config,
        state,
        changes,
    })
}

/// Turns on the settings at `path`, so Sheprd keeps the daemon running
/// rather than stopping it, including when it is installed here later.
/// Sheprd's own lock, waited for up to `wait`, keeps its saves and moves
/// apart from this. Blocking.
fn enable_settings_at(path: &Path, wait: Duration) -> super::Result<RemoteConfig> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(io_error("create", parent))?;
    let _lock = lock_settings(path, wait)?;
    let plan = plan_settings(path)?;
    for change in &plan.changes {
        change.apply()?;
    }
    Ok(plan.config)
}

/// Takes Sheprd's lock on the settings at `path`, held until the file
/// returned is dropped. Sheprd may be stopped while holding it, so this
/// gives up after `wait` rather than outlasting the SSH job that ran it.
fn lock_settings(path: &Path, wait: Duration) -> super::Result<fs::File> {
    let lock_path = settings_files::lock_path(path);
    let lock = settings_files::open_lock(&lock_path).map_err(io_error("open", &lock_path))?;
    let deadline = Instant::now() + wait;
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(SETTINGS_LOCK_POLL);
            }
            Err(fs::TryLockError::WouldBlock) => {
                return Err(Error::SettingsLocked { path: lock_path }.into());
            }
            Err(fs::TryLockError::Error(source)) => {
                return Err(io_error("lock", &lock_path)(source));
            }
        }
    }
}

/// `[remote]` as `text` sets it, turned on, and `text` with `enabled =
/// true` when it does not say so already.
fn turned_on(path: &Path, text: &str) -> super::Result<(RemoteConfig, Option<String>)> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        remote: RemoteConfig,
    }
    let config_error = |source: Box<dyn std::error::Error + Send + Sync>| Error::Config {
        path: path.to_owned(),
        source,
    };
    let file: File = toml::from_str(text).map_err(|error| config_error(Box::new(error)))?;
    if file.remote.enabled {
        return Ok((file.remote, None));
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
    Ok((enabled(file.remote), Some(document.to_string())))
}

fn io_error(operation: &'static str, path: &Path) -> impl FnOnce(io::Error) -> super::Error {
    let path = path.to_owned();
    move |source| super::Error::Io {
        operation,
        path,
        source,
    }
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
pub(super) fn write_private(path: &Path, bytes: &[u8]) -> super::Result<()> {
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
