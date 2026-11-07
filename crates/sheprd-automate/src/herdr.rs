//! Driving Herdr through its own command line, as Sheprd's fan-out and
//! orchestrator do: worktrees, agents, notifications and pane details. The
//! daemon starts Herdr's server when a step needs it and it is not running;
//! it never stops, upgrades or reconfigures Herdr.

use serde::Deserialize;
use serde_json::Value;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// How long `agent start` may wait for the agent to be ready, as fan-out.
const AGENT_START_TIMEOUT_MS: &str = "90000";
/// How long to retry `agent start` while the new pane is still busy.
const BUSY_RETRY: Duration = Duration::from_secs(10);
/// How long to wait for a server the daemon started.
const SERVER_START: Duration = Duration::from_secs(120);
/// The most of a cancellable call's output kept, per stream.
const MAX_OUTPUT: u64 = 1024 * 1024;
/// How often a cancellable call looks at its stop request.
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Herdr did not start within {} seconds", SERVER_START.as_secs())]
    DidNotStart,
    #[error("could not run {}: {source}", program.display())]
    Launch {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("herdr {command} failed: {message}")]
    Failed { command: String, message: String },
    #[error("herdr {command}: the pane is gone")]
    PaneNotFound { command: String },
    #[error("herdr {command} was stopped on request")]
    Stopped { command: String },
    #[error("herdr {command} ran past the step's timeout")]
    TimedOut { command: String },
    #[error("herdr {command} answered something unexpected: {source}")]
    Answer {
        command: String,
        #[source]
        source: serde_json::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

/// The `herdr` binary and the session to drive.
#[derive(Clone, Debug)]
pub struct Herdr {
    program: PathBuf,
    session: Option<String>,
    /// Set for one step: every call ends at its deadline or stop request.
    limit: Option<Limit>,
}

#[derive(Clone, Debug)]
struct Limit {
    stop: Arc<AtomicBool>,
    deadline: Instant,
}

/// A new worktree and the shell pane Herdr opened in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub branch: Option<String>,
    pub pane: String,
}

/// What `pane process-info` says about a pane.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ProcessInfo {
    pub shell_pid: Option<u32>,
    pub foreground_process_group_id: Option<u32>,
}

impl ProcessInfo {
    /// The shell is in the foreground again: whatever ran there has ended.
    pub fn at_prompt(&self) -> bool {
        self.shell_pid.is_some() && self.shell_pid == self.foreground_process_group_id
    }
}

/// What `pane get` says about the agent in a pane.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct PaneAgent {
    pub agent: Option<String>,
    pub agent_status: Option<String>,
    pub agent_session: Option<Session>,
}

/// An agent's session as Herdr knows it: an id or a file path.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Session {
    pub kind: String,
    pub value: String,
}

impl Herdr {
    /// `program`, or the first `herdr` on the usual paths.
    pub fn new(program: Option<PathBuf>, session: Option<String>) -> Self {
        Self {
            program: program.unwrap_or_else(find),
            session,
            limit: None,
        }
    }

    /// This Herdr for one step: each call, and waiting for a server it
    /// starts, ends when `stop` is set or `deadline` passes.
    pub fn bounded(&self, stop: Arc<AtomicBool>, deadline: Instant) -> Self {
        Self {
            limit: Some(Limit { stop, deadline }),
            ..self.clone()
        }
    }

    /// Why a bounded step must give up now, if it must.
    fn limit_reached(&self, command: &str) -> Result<()> {
        let Some(limit) = &self.limit else {
            return Ok(());
        };
        if limit.stop.load(Ordering::Relaxed) {
            return Err(Error::Stopped {
                command: command.to_owned(),
            });
        }
        if Instant::now() >= limit.deadline {
            return Err(Error::TimedOut {
                command: command.to_owned(),
            });
        }
        Ok(())
    }

    fn command(&self, args: &[&str]) -> std::process::Command {
        let mut command = sheprd_client::new_command(&self.program);
        if let Some(session) = &self.session {
            command.args(["--session", session]);
        }
        command.args(args).stdin(Stdio::null());
        command
    }

    /// Runs `herdr ARGS` and returns its `result` object.
    fn call(&self, args: &[&str]) -> Result<Value> {
        let name = command_name(args);
        answer(&name, &self.output(args)?)
    }

    /// Runs `herdr ARGS` to its end, or, bounded, until the step's stop
    /// request or deadline, which kill it.
    fn output(&self, args: &[&str]) -> Result<Output> {
        let launch = |source| Error::Launch {
            program: self.program.clone(),
            source,
        };
        if self.limit.is_none() {
            return self.command(args).output().map_err(launch);
        }
        let name = command_name(args);
        let mut child = self
            .command(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(launch)?;
        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let status = loop {
            if let Some(status) = child.try_wait().map_err(launch)? {
                break status;
            }
            if let Err(error) = self.limit_reached(&name) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            thread::sleep(POLL);
        };
        Ok(Output {
            status,
            stdout: stdout.join().unwrap_or_default(),
            stderr: stderr.join().unwrap_or_default(),
        })
    }

    /// Whether Herdr's server answers as running. Only a bounded step's
    /// stop or deadline is an error; any other failure reads as not running.
    fn server_running(&self) -> Result<bool> {
        let output = match self.output(&["status", "server", "--json"]) {
            Ok(output) => output,
            Err(error @ (Error::Stopped { .. } | Error::TimedOut { .. })) => return Err(error),
            Err(_) => return Ok(false),
        };
        Ok(serde_json::from_slice::<Value>(&output.stdout)
            .ok()
            .and_then(|status| status.get("running").and_then(Value::as_bool))
            .unwrap_or(false))
    }

    /// Starts Herdr's headless server if it is not running, and waits for
    /// it. Detached in its own process group, as Herdr starts itself.
    pub fn ensure_server(&self) -> Result<()> {
        if self.server_running()? {
            return Ok(());
        }
        let mut command = self.command(&["server"]);
        command.stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn().map_err(|source| Error::Launch {
            program: self.program.clone(),
            source,
        })?;
        // Reap it whenever it exits, so it never lingers as a zombie.
        thread::spawn(move || {
            let _ = child.wait();
        });
        let deadline = Instant::now() + SERVER_START;
        while Instant::now() < deadline {
            if self.server_running()? {
                return Ok(());
            }
            self.limit_reached("server")?;
            thread::sleep(Duration::from_millis(500));
        }
        Err(Error::DidNotStart)
    }

    pub fn create_worktree(
        &self,
        repo: &Path,
        branch: &str,
        base: Option<&str>,
    ) -> Result<Worktree> {
        let cwd = repo.to_string_lossy();
        let mut args = vec![
            "worktree",
            "create",
            "--cwd",
            &cwd,
            "--branch",
            branch,
            "--no-focus",
        ];
        if let Some(base) = base {
            args.extend(["--base", base]);
        }
        let result = self.call(&args)?;
        let text = |pointer: &str| {
            result
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        match (text("/worktree/path"), text("/root_pane/pane_id")) {
            (Some(path), Some(pane)) => Ok(Worktree {
                path,
                branch: text("/worktree/branch"),
                pane,
            }),
            _ => Err(unexpected("worktree create", "no worktree path or pane")),
        }
    }

    /// Starts `harness` in `pane`, retrying while the new shell is busy.
    pub fn start_agent(&self, name: &str, harness: &str, pane: &str) -> Result<()> {
        let deadline = Instant::now() + BUSY_RETRY;
        loop {
            let started = self.call(&[
                "agent",
                "start",
                name,
                "--kind",
                harness,
                "--pane",
                pane,
                "--timeout",
                AGENT_START_TIMEOUT_MS,
            ]);
            match started {
                Err(Error::Failed { message, .. })
                    if message.contains("agent_pane_busy") && Instant::now() < deadline =>
                {
                    self.limit_reached("agent start")?;
                    thread::sleep(Duration::from_millis(100));
                }
                other => return other.map(drop),
            }
        }
    }

    /// Sends `prompt`; with `wait`, returns once the agent is done or
    /// `timeout` passes, or, bounded, at the step's stop or deadline.
    /// Stopping ends the wait, not the agent, which keeps its pane as any
    /// agent the daemon started does.
    pub fn prompt(&self, pane: &str, prompt: &str, wait: Option<Duration>) -> Result<()> {
        let millis;
        let mut args = vec!["agent", "prompt", pane, prompt];
        if let Some(timeout) = wait {
            millis = timeout.as_millis().clamp(1_000, 86_400_000).to_string();
            args.extend(["--wait", "--until", "done", "--timeout", &millis]);
        }
        self.call(&args).map(drop)
    }

    pub fn notify(&self, title: &str, body: &str) -> Result<()> {
        self.call(&[
            "notification",
            "show",
            title,
            "--body",
            body,
            "--sound",
            "done",
        ])
        .map(drop)
    }

    pub fn process_info(&self, pane: &str) -> Result<ProcessInfo> {
        let result = self.call(&["pane", "process-info", "--pane", pane])?;
        let info = result.get("process_info").cloned().unwrap_or(Value::Null);
        serde_json::from_value(info).map_err(|source| Error::Answer {
            command: "pane process-info".to_owned(),
            source,
        })
    }

    pub fn pane_agent(&self, pane: &str) -> Result<PaneAgent> {
        let result = self.call(&["pane", "get", pane])?;
        let pane = result.get("pane").cloned().unwrap_or(Value::Null);
        serde_json::from_value(pane).map_err(|source| Error::Answer {
            command: "pane get".to_owned(),
            source,
        })
    }
}

/// A command's name for messages: its first two words.
fn command_name(args: &[&str]) -> String {
    args.iter().take(2).copied().collect::<Vec<_>>().join(" ")
}

/// Reads a child's stream while it runs, so a full pipe never blocks it.
fn drain(stream: Option<impl Read + Send + 'static>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(stream) = stream {
            let _ = stream.take(MAX_OUTPUT).read_to_end(&mut bytes);
        }
        bytes
    })
}

/// The `result` of a Herdr answer, or its error. Herdr prints results on
/// stdout and `{"error": {"code", "message"}}` on stderr.
fn answer(command: &str, output: &Output) -> Result<Value> {
    if output.status.success() {
        let envelope =
            serde_json::from_slice::<Value>(&output.stdout).map_err(|source| Error::Answer {
                command: command.to_owned(),
                source,
            })?;
        return Ok(envelope.get("result").cloned().unwrap_or(envelope));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let error = serde_json::from_str::<Value>(stderr.trim()).ok();
    if error
        .as_ref()
        .and_then(|error| error.pointer("/error/code"))
        .and_then(Value::as_str)
        == Some("pane_not_found")
    {
        return Err(Error::PaneNotFound {
            command: command.to_owned(),
        });
    }
    let message = error
        .and_then(|error| {
            let code = error
                .pointer("/error/code")
                .and_then(Value::as_str)?
                .to_owned();
            let text = error
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Some(format!("{code}: {text}"))
        })
        .unwrap_or_else(|| last_line(&stderr));
    Err(Error::Failed {
        command: command.to_owned(),
        message,
    })
}

fn last_line(text: &str) -> String {
    let line = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no output");
    line.chars().take(500).collect()
}

fn unexpected(command: &str, what: &str) -> Error {
    Error::Failed {
        command: command.to_owned(),
        message: what.to_owned(),
    }
}

/// The first `herdr` on `PATH`, then where installers put it, as Sheprd
/// looks; the bare name otherwise.
pub fn find() -> PathBuf {
    let name = if cfg!(windows) { "herdr.exe" } else { "herdr" };
    let mut candidates = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join(".cargo/bin").join(name));
    }
    for dir in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/home/linuxbrew/.linuxbrew/bin",
    ] {
        candidates.push(Path::new(dir).join(name));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
