//! Keeping the daemon running: a user service that starts it at login and
//! after a crash (a launchd agent, a systemd user unit, or a logon task on
//! Windows), and `start` and `shutdown` for when there is none. No admin
//! rights, nothing outside the user's home. A clean shutdown stays down.

use crate::{
    daemon::Lock,
    paths::{CONFIG_DIR, Paths, STATE_DIR},
    store::{RequestKind, Store},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

const LABEL: &str = "dev.herdr.automate";
const UNIT: &str = "herdr-automate.service";
const TASK: &str = "herdr-automate";
const START_WAIT: Duration = Duration::from_secs(15);
const STOP_WAIT: Duration = Duration::from_secs(40);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{command} failed: {stderr}")]
    Command { command: String, stderr: String },
    #[error("no home directory")]
    NoHome,
    #[error("the daemon did not stop within {} seconds", STOP_WAIT.as_secs())]
    StopTimeout,
}

/// What `install-service` writes and runs, printed before it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Service {
    pub file: Option<PathBuf>,
    pub content: Option<String>,
    pub commands: Vec<Vec<String>>,
}

fn home() -> Result<PathBuf, Error> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or(Error::NoHome)
}

fn file() -> Option<PathBuf> {
    let home = home().ok()?;
    if cfg!(target_os = "macos") {
        Some(
            home.join("Library/LaunchAgents")
                .join(format!("{LABEL}.plist")),
        )
    } else if cfg!(windows) {
        None
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Some(config.join("systemd/user").join(UNIT))
    }
}

/// Whether a service is set up; on Windows, a marker the install leaves.
pub fn installed() -> bool {
    match file() {
        Some(file) => file.is_file(),
        None => Paths::from_env().is_ok_and(|paths| paths.state.join("service").is_file()),
    }
}

/// What the daemon's environment needs from the installing shell's: its
/// `PATH`, and the directories this command uses, so a `HERDR_AUTOMATE_HOME`
/// or XDG directory set only in the shell still applies.
fn environment(paths: &Paths) -> Vec<(&'static str, String)> {
    vec![
        ("PATH", std::env::var("PATH").unwrap_or_default()),
        (STATE_DIR, paths.state.to_string_lossy().into_owned()),
        (CONFIG_DIR, paths.config.to_string_lossy().into_owned()),
    ]
}

/// The service for `exe`, which runs `exe serve`.
pub fn describe(exe: &Path, paths: &Paths) -> Result<Service, Error> {
    let exe_text = exe.to_string_lossy().into_owned();
    let environment = environment(paths);
    if cfg!(target_os = "macos") {
        let uid = user_id();
        let file = file().ok_or(Error::NoHome)?;
        let out = paths
            .state
            .join("daemon.out")
            .to_string_lossy()
            .into_owned();
        let content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{}</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>EnvironmentVariables</key><dict>{}</dict>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
</dict>
</plist>
"#,
            xml(&exe_text),
            environment
                .iter()
                .map(|(name, value)| format!("<key>{name}</key><string>{}</string>", xml(value)))
                .collect::<String>(),
            xml(&out),
            xml(&out)
        );
        let target = format!("gui/{uid}");
        return Ok(Service {
            commands: vec![
                vec![
                    "launchctl".into(),
                    "bootout".into(),
                    format!("{target}/{LABEL}"),
                ],
                vec![
                    "launchctl".into(),
                    "bootstrap".into(),
                    target,
                    file.to_string_lossy().into_owned(),
                ],
            ],
            file: Some(file),
            content: Some(content),
        });
    }
    if cfg!(windows) {
        // A logon task cannot set variables, so it runs a script that does.
        let launcher = paths.state.join("serve.cmd");
        let mut content = String::from("@echo off\r\n");
        for (name, value) in environment.iter().filter(|(name, _)| *name != "PATH") {
            // `%` is the only character a batch file expands inside quotes.
            content.push_str(&format!("set \"{name}={}\"\r\n", value.replace('%', "%%")));
        }
        content.push_str(&format!("\"{}\" serve\r\n", exe_text.replace('%', "%%")));
        return Ok(Service {
            file: Some(launcher.clone()),
            content: Some(content),
            commands: vec![
                vec![
                    "schtasks".into(),
                    "/Create".into(),
                    "/TN".into(),
                    TASK.into(),
                    "/SC".into(),
                    "ONLOGON".into(),
                    "/RL".into(),
                    "LIMITED".into(),
                    "/F".into(),
                    "/TR".into(),
                    format!("\"{}\"", launcher.to_string_lossy()),
                ],
                vec!["schtasks".into(), "/Run".into(), "/TN".into(), TASK.into()],
            ],
        });
    }
    let file = file().ok_or(Error::NoHome)?;
    let content = format!(
        "[Unit]\nDescription=herdr-automate: Herdr automations\n\n[Service]\nExecStart=\"{}\" serve\n\
         Restart=on-failure\nRestartSec=10\n{}\n[Install]\nWantedBy=default.target\n",
        // `$` would name a variable in a command line, though not in
        // `Environment=`.
        systemd_quote(&exe_text).replace('$', "$$"),
        environment
            .iter()
            .map(|(name, value)| format!("Environment=\"{name}={}\"\n", systemd_quote(value)))
            .collect::<String>(),
    );
    Ok(Service {
        commands: vec![
            vec!["systemctl".into(), "--user".into(), "daemon-reload".into()],
            vec![
                "systemctl".into(),
                "--user".into(),
                "enable".into(),
                "--now".into(),
                UNIT.into(),
            ],
        ],
        file: Some(file),
        content: Some(content),
    })
}

/// Text inside a systemd double-quoted word: backslashes and quotes are
/// escaped, and `%` starts a specifier, so it is doubled.
fn systemd_quote(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(unix)]
fn user_id() -> u32 {
    rustix::process::getuid().as_raw()
}

#[cfg(not(unix))]
fn user_id() -> u32 {
    0
}

pub fn install(service: &Service, paths: &Paths) -> Result<(), Error> {
    if let (Some(file), Some(content)) = (&service.file, &service.content) {
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_owned(),
                source,
            })?;
        }
        fs::write(file, content).map_err(|source| Error::Io {
            path: file.clone(),
            source,
        })?;
    }
    for (index, command) in service.commands.iter().enumerate() {
        // Unloading a launchd agent that is not loaded fails; that is fine.
        let tolerate = cfg!(target_os = "macos") && index == 0;
        match run(command) {
            Err(_) if tolerate => {}
            other => other?,
        }
    }
    if cfg!(windows) {
        let marker = paths.state.join("service");
        let _ = fs::create_dir_all(&paths.state);
        fs::write(&marker, TASK).map_err(|source| Error::Io {
            path: marker,
            source,
        })?;
    }
    Ok(())
}

pub fn uninstall(paths: &Paths) -> Result<(), Error> {
    let commands: Vec<Vec<String>> = if cfg!(target_os = "macos") {
        vec![vec![
            "launchctl".into(),
            "bootout".into(),
            format!("gui/{}/{LABEL}", user_id()),
        ]]
    } else if cfg!(windows) {
        vec![vec![
            "schtasks".into(),
            "/Delete".into(),
            "/TN".into(),
            TASK.into(),
            "/F".into(),
        ]]
    } else {
        vec![vec![
            "systemctl".into(),
            "--user".into(),
            "disable".into(),
            "--now".into(),
            UNIT.into(),
        ]]
    };
    for command in &commands {
        let _ = run(command);
    }
    if let Some(file) = file()
        && file.exists()
    {
        fs::remove_file(&file).map_err(|source| Error::Io { path: file, source })?;
    }
    let _ = fs::remove_file(paths.state.join("service"));
    let _ = fs::remove_file(paths.state.join("serve.cmd"));
    Ok(())
}

fn run(command: &[String]) -> Result<(), Error> {
    let Some((program, args)) = command.split_first() else {
        return Ok(());
    };
    let output = sheprd_client::new_command(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|source| Error::Io {
            path: PathBuf::from(program),
            source,
        })?;
    if output.status.success() {
        return Ok(());
    }
    Err(Error::Command {
        command: command.join(" "),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

/// Starts the daemon if it is not running: through the service when there
/// is one, else detached. Returns whether it was already running.
pub fn start(exe: &Path, paths: &Paths) -> crate::Result<bool> {
    if Lock::held(paths) {
        return Ok(true);
    }
    let kicked = installed()
        && if cfg!(target_os = "macos") {
            run(&[
                "launchctl".into(),
                "kickstart".into(),
                format!("gui/{}/{LABEL}", user_id()),
            ])
            .is_ok()
        } else if cfg!(windows) {
            run(&["schtasks".into(), "/Run".into(), "/TN".into(), TASK.into()]).is_ok()
        } else {
            run(&[
                "systemctl".into(),
                "--user".into(),
                "start".into(),
                UNIT.into(),
            ])
            .is_ok()
        };
    if !kicked {
        crate::paths::private_dir(&paths.state).map_err(crate::Error::io(&paths.state))?;
        let out = paths.state.join("daemon.out");
        let file = crate::paths::private_file()
            .create(true)
            .append(true)
            .open(&out)
            .map_err(crate::Error::io(&out))?;
        let mut command = sheprd_client::new_command(exe);
        command
            .arg("serve")
            .stdin(Stdio::null())
            .stdout(file.try_clone().map_err(crate::Error::io(&out))?)
            .stderr(file);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn().map_err(crate::Error::io(exe))?;
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
    let deadline = Instant::now() + START_WAIT;
    while Instant::now() < deadline {
        if Lock::held(paths) {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(crate::Error::StartTimeout {
        seconds: START_WAIT.as_secs(),
    })
}

/// Asks the daemon to stop after its runs, and waits. Returns whether one
/// was running.
pub fn shutdown(paths: &Paths, store: &Store) -> crate::Result<bool> {
    if !Lock::held(paths) {
        return Ok(false);
    }
    store.push_request(&RequestKind::Shutdown, crate::runner::now())?;
    let deadline = Instant::now() + STOP_WAIT;
    while Instant::now() < deadline {
        if !Lock::held(paths) {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(Error::StopTimeout.into())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
