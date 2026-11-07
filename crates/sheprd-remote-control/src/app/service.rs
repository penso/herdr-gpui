//! The daemon as a user service: what it runs with, the service manager of
//! each platform, and bringing the installed service in line with the
//! `[remote]` settings. Everything here blocks; the supervisor runs it on
//! the background executor.
//!
//! - macOS: a LaunchAgent in `~/Library/LaunchAgents`, loaded with
//!   `launchctl bootstrap gui/UID` and removed with `bootout`.
//! - Linux: a systemd user unit, `systemctl --user enable` and `restart`.
//! - Windows: a Task Scheduler task that runs at sign-in.

pub mod files;

use crate::app::{
    Error, Result,
    config::{Certificate, RemoteConfig},
};
use crate::control::{Request, Response};
use crate::core_target::CoreTarget;
use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
};

/// The LaunchAgent's label, under the app's bundle identifier.
pub const LABEL: &str = "so.pen.herdr-gpui.remote";
/// The systemd unit's name.
pub const UNIT: &str = "herdr-remote-control.service";
/// The Task Scheduler task's name.
pub const TASK: &str = "Herdr UI Remote Control";
/// The daemon's file name on this platform.
pub const PROGRAM: &str = if cfg!(windows) {
    "herdr-remote-control.exe"
} else {
    "herdr-remote-control"
};
/// How much of a failed command's own message is kept.
const STDERR_LIMIT: usize = 512;

/// What the service runs: the daemon, its state, and the settings it serves
/// with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub program: PathBuf,
    pub state_dir: PathBuf,
    pub config: RemoteConfig,
    /// The Herdr daemon's client socket, the chat socket folder, and on
    /// Windows the saved WSL distributions as this app resolved them, passed
    /// on because a service manager's environment may lack the shell's
    /// `XDG_CONFIG_HOME` or `XDG_STATE_HOME`.
    pub herdr_socket: Option<PathBuf>,
    pub chat_dir: Option<PathBuf>,
    pub wsl_catalog: Option<PathBuf>,
    /// The core endpoint or setup failure resolved in the desktop process.
    pub core_endpoint: CoreTarget,
}

impl Plan {
    /// `serve --state-dir DIR --port N --bind ADDR [--cert F --key F]
    /// --privacy generic|excerpt --device-idle-days N [--live-activities
    /// generic] [--herdr-socket P] [--chat-dir D] [--wsl-catalog F]
    /// [--core-socket P --core-session S]`.
    pub fn args(&self) -> Vec<OsString> {
        let config = &self.config;
        let mut args: Vec<OsString> = vec![
            "serve".into(),
            "--state-dir".into(),
            self.state_dir.clone().into(),
            "--port".into(),
            config.port.to_string().into(),
            "--bind".into(),
            config.bind.to_string().into(),
        ];
        if let Certificate::Files { cert, key } = &config.certificate {
            args.extend([
                "--cert".into(),
                cert.clone().into(),
                "--key".into(),
                key.clone().into(),
            ]);
        }
        args.extend([
            "--privacy".into(),
            config.notifications.name().into(),
            "--device-idle-days".into(),
            config.device_idle_days.to_string().into(),
        ]);
        // The default is left out, so the service definition, and with it
        // the running daemon, stays as it was for everyone who keeps it.
        if !config.live_activities.is_detailed() {
            args.extend([
                "--live-activities".into(),
                config.live_activities.name().into(),
            ]);
        }
        if let Some(socket) = &self.herdr_socket {
            args.extend(["--herdr-socket".into(), socket.clone().into()]);
        }
        if let Some(dir) = &self.chat_dir {
            args.extend(["--chat-dir".into(), dir.clone().into()]);
        }
        if let Some(catalog) = &self.wsl_catalog {
            args.extend(["--wsl-catalog".into(), catalog.clone().into()]);
        }
        match &self.core_endpoint {
            CoreTarget::Endpoint(endpoint) => args.extend([
                "--core-socket".into(),
                endpoint.socket.clone().into(),
                "--core-session".into(),
                endpoint.session.clone().into(),
            ]),
            CoreTarget::Discover => {}
        }
        args
    }
}

/// One command a service manager is asked to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub program: &'static str,
    pub args: Vec<OsString>,
    /// Whether its failure fails the whole change. Unloading something not
    /// loaded fails, and is fine.
    pub required: bool,
}

fn step<const N: usize>(program: &'static str, args: [&str; N], required: bool) -> Step {
    Step {
        program,
        args: args.into_iter().map(OsString::from).collect(),
        required,
    }
}

/// The platform's service manager, with what it needs to address the service.
/// A build makes only its own platform's; the tests check every platform's
/// files on every platform.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Manager {
    LaunchAgent {
        uid: u32,
    },
    Systemd,
    /// `user` is `DOMAIN\name`, whose sign-in starts the task.
    ScheduledTask {
        user: String,
    },
}

impl Manager {
    /// This platform's manager. Reads the user's identity; no I/O beyond.
    pub fn current() -> Result<Self> {
        #[cfg(target_os = "macos")]
        return Ok(Self::LaunchAgent {
            uid: rustix::process::getuid().as_raw(),
        });
        #[cfg(all(unix, not(target_os = "macos")))]
        return Ok(Self::Systemd);
        #[cfg(windows)]
        {
            let var = |name| std::env::var(name).ok().filter(|value| !value.is_empty());
            let name = var("USERNAME").ok_or(Error::UnknownUser)?;
            return Ok(Self::ScheduledTask {
                user: match var("USERDOMAIN") {
                    Some(domain) => format!("{domain}\\{name}"),
                    None => name,
                },
            });
        }
        #[allow(unreachable_code)]
        Err(Error::Unsupported)
    }

    /// Where the service definition goes. Task Scheduler keeps its own copy;
    /// ours stays in the state directory to compare the next change with.
    pub fn file(&self, home: &Path, config_home: &Path, state_dir: &Path) -> PathBuf {
        match self {
            Self::LaunchAgent { .. } => home
                .join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
            Self::Systemd => config_home.join("systemd/user").join(UNIT),
            Self::ScheduledTask { .. } => state_dir.join("task.xml"),
        }
    }

    /// The definition's text for `plan`.
    pub fn text(&self, plan: &Plan) -> Result<String> {
        match self {
            Self::LaunchAgent { .. } => files::launch_agent(plan),
            Self::Systemd => files::systemd_unit(plan),
            Self::ScheduledTask { user } => files::scheduled_task(plan, user),
        }
    }

    /// The definition's bytes on disk.
    fn bytes(&self, text: &str) -> Vec<u8> {
        match self {
            Self::ScheduledTask { .. } => files::utf16(text),
            Self::LaunchAgent { .. } | Self::Systemd => text.as_bytes().to_vec(),
        }
    }

    /// Loads a written or rewritten definition, restarting a daemon that runs
    /// with the old one.
    pub fn load(&self, file: &Path) -> Vec<Step> {
        match self {
            Self::LaunchAgent { uid } => {
                let domain = format!("gui/{uid}");
                vec![
                    step(
                        "launchctl",
                        ["bootout", &format!("{domain}/{LABEL}")],
                        false,
                    ),
                    Step {
                        program: "launchctl",
                        args: vec!["bootstrap".into(), domain.into(), file.into()],
                        required: true,
                    },
                ]
            }
            Self::Systemd => vec![
                step("systemctl", ["--user", "daemon-reload"], true),
                step("systemctl", ["--user", "enable", UNIT], true),
                step("systemctl", ["--user", "restart", UNIT], true),
            ],
            Self::ScheduledTask { .. } => vec![
                step("schtasks", ["/End", "/TN", TASK], false),
                Step {
                    program: "schtasks",
                    args: vec![
                        "/Create".into(),
                        "/TN".into(),
                        TASK.into(),
                        "/XML".into(),
                        file.into(),
                        "/F".into(),
                    ],
                    required: true,
                },
                step("schtasks", ["/Run", "/TN", TASK], true),
            ],
        }
    }

    /// Starts the installed daemon that is not running, as after it was
    /// asked to stop when Sheprd quit.
    pub fn start(&self, file: &Path) -> Vec<Step> {
        match self {
            Self::LaunchAgent { uid } => {
                let domain = format!("gui/{uid}");
                vec![
                    // Fails when already loaded, which is what kickstart wants.
                    Step {
                        program: "launchctl",
                        args: vec!["bootstrap".into(), domain.clone().into(), file.into()],
                        required: false,
                    },
                    step(
                        "launchctl",
                        ["kickstart", &format!("{domain}/{LABEL}")],
                        true,
                    ),
                ]
            }
            Self::Systemd => vec![step("systemctl", ["--user", "start", UNIT], true)],
            Self::ScheduledTask { .. } => vec![step("schtasks", ["/Run", "/TN", TASK], true)],
        }
    }

    /// Stops and unregisters the service; the definition is removed after.
    pub fn unload(&self) -> Vec<Step> {
        match self {
            Self::LaunchAgent { uid } => vec![step(
                "launchctl",
                ["bootout", &format!("gui/{uid}/{LABEL}")],
                false,
            )],
            Self::Systemd => vec![step(
                "systemctl",
                ["--user", "disable", "--now", UNIT],
                false,
            )],
            Self::ScheduledTask { .. } => vec![
                step("schtasks", ["/End", "/TN", TASK], false),
                step("schtasks", ["/Delete", "/TN", TASK, "/F"], false),
            ],
        }
    }

    /// What runs once the definition is gone.
    pub fn unloaded(&self) -> Vec<Step> {
        match self {
            Self::Systemd => vec![step("systemctl", ["--user", "daemon-reload"], false)],
            Self::LaunchAgent { .. } | Self::ScheduledTask { .. } => Vec::new(),
        }
    }
}

/// Where this user's service goes and what it runs.
#[derive(Clone, Debug)]
pub struct Host {
    pub manager: Manager,
    /// The service definition's path.
    pub file: PathBuf,
    pub state_dir: PathBuf,
    /// The daemon, once found.
    pub program: Option<PathBuf>,
    /// A release app's version: a daemon of another version is restarted,
    /// so the binary beside the app is the one serving. `None` in
    /// development builds, which leave a running daemon alone.
    pub release: Option<String>,
}

/// What a change did, for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do: off and not installed, or on and running as configured.
    Unchanged,
    Installed,
    Started,
    Removed,
}

/// Brings the installed service in line with `config`. `send` asks the
/// daemon over its control endpoint; `run` runs a manager's command.
pub fn reconcile(
    config: &RemoteConfig,
    host: &Host,
    send: impl Fn(&Request) -> Result<Response, crate::control::Error>,
    run: impl FnMut(&Step) -> Result<()>,
) -> Result<Outcome> {
    reconcile_with_core(config, host, send, run, || {
        sheprd_core_client::Endpoint::resolve(sheprd_client::login_env::var)
    })
}

fn reconcile_with_core(
    config: &RemoteConfig,
    host: &Host,
    send: impl Fn(&Request) -> Result<Response, crate::control::Error>,
    mut run: impl FnMut(&Step) -> Result<()>,
    core_endpoint: impl FnOnce() -> sheprd_core_client::Result<sheprd_core_client::Endpoint>,
) -> Result<Outcome> {
    let installed = host.file.exists();
    if !config.enabled {
        if !installed {
            return Ok(Outcome::Unchanged);
        }
        // Asked first, so it exits cleanly and the manager leaves it down.
        if let Err(error) = send(&Request::Shutdown) {
            tracing::debug!(%error, "Remote control daemon did not take the shutdown");
        }
        for step in host.manager.unload() {
            run(&step)?;
        }
        remove(&host.file)?;
        for step in host.manager.unloaded() {
            run(&step)?;
        }
        return Ok(Outcome::Removed);
    }
    if write_definition(config, host, core_endpoint())? {
        for step in host.manager.load(&host.file) {
            run(&step)?;
        }
        return Ok(Outcome::Installed);
    }
    match send(&Request::Status) {
        // A release's daemon from another release than this app's is
        // restarted, so the binary beside the app is the one serving.
        Ok(Response::Status(status))
            if host
                .release
                .as_ref()
                .is_some_and(|version| *version != status.version) =>
        {
            for step in host.manager.load(&host.file) {
                run(&step)?;
            }
            Ok(Outcome::Started)
        }
        Ok(_) => Ok(Outcome::Unchanged),
        Err(_) => {
            for step in host.manager.start(&host.file) {
                run(&step)?;
            }
            Ok(Outcome::Started)
        }
    }
}

/// Restarts the daemon from the program `host` names, as Settings' Restart
/// does after the binary beside the app was rebuilt or updated. The
/// definition is rewritten first when it changed. Off, nothing runs.
pub fn restart(
    config: &RemoteConfig,
    host: &Host,
    mut run: impl FnMut(&Step) -> Result<()>,
) -> Result<Outcome> {
    if !config.enabled {
        return Ok(Outcome::Unchanged);
    }
    write_definition(
        config,
        host,
        sheprd_core_client::Endpoint::resolve(sheprd_client::login_env::var),
    )?;
    for step in host.manager.load(&host.file) {
        run(&step)?;
    }
    Ok(Outcome::Started)
}

/// Writes the service definition for `config` unless the file already
/// holds it. Returns whether it wrote.
fn write_definition(
    config: &RemoteConfig,
    host: &Host,
    core_endpoint: sheprd_core_client::Result<sheprd_core_client::Endpoint>,
) -> Result<bool> {
    let program = host.program.clone().ok_or(Error::ProgramMissing)?;
    let plan = Plan {
        program,
        state_dir: host.state_dir.clone(),
        config: config.clone(),
        herdr_socket: sheprd_client::ConnectTarget::Local.socket_path().ok(),
        chat_dir: sheprd_chat_protocol::endpoint_dir(),
        wsl_catalog: cfg!(windows).then(|| sheprd_client::wsl_hosts_path(false)),
        core_endpoint: CoreTarget::resolved(core_endpoint),
    };
    let text = host.manager.text(&plan)?;
    let bytes = host.manager.bytes(&text);
    if fs::read(&host.file).ok().as_deref() == Some(bytes.as_slice()) {
        return Ok(false);
    }
    private_dir(&host.state_dir)?;
    write(&host.file, &bytes)?;
    Ok(true)
}

/// Runs one manager command through `sheprd_client::new_command`.
pub fn run(step: &Step) -> Result<()> {
    let output = sheprd_client::new_command(step.program)
        .args(&step.args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|source| Error::Spawn {
            program: step.program,
            source,
        })?;
    if output.status.success() || !step.required {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(Error::Service {
        program: step.program,
        status: output.status,
        stderr: stderr.trim().chars().take(STDERR_LIMIT).collect(),
    })
}

/// The daemon's state directory, private to this user.
fn private_dir(path: &Path) -> Result<()> {
    let failed = |source| Error::Io {
        operation: "create",
        path: path.to_owned(),
        source,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    #[cfg(unix)]
    let created = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    };
    #[cfg(not(unix))]
    let created = fs::create_dir(path);
    match created {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(failed(error)),
        _ => Ok(()),
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let failed = |source| Error::Io {
        operation: "write",
        path: path.to_owned(),
        source,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    fs::write(path, bytes).map_err(failed)
}

fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(Error::Io {
            operation: "remove",
            path: path.to_owned(),
            source: error,
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
