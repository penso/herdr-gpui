//! One command an agent runs on a saved cloud machine, through the same
//! authenticated command the device connection uses (`coder ssh`, a
//! provider's SSH gateway, and so on), so no credential ever leaves the app.
//! The command gets no input. Its stdout and stderr are kept apart, each to a
//! bounded tail with terminal controls removed, as the output is untrusted
//! and goes back over the control socket. It is stopped, with the remote
//! session, on timeout or cancellation.

use super::CloudProvider;
use sheprd_client::shell_quote;
use std::{
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// The stdout tail kept. With stderr's, and JSON escaping at most doubling
/// what is left after controls go, an answer stays inside the socket bound.
pub const STDOUT_LIMIT: usize = 64 * 1024;
pub const STDERR_LIMIT: usize = 32 * 1024;
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The command must name a program.")]
    Empty,
    #[error("Could not reach the cloud machine: {0}")]
    Reach(#[source] io::Error),
    #[error("Could not start the command: {0}")]
    Spawn(#[source] io::Error),
    #[error("Could not wait for the command: {0}")]
    Wait(#[source] io::Error),
    #[error("The command did not finish within {0} seconds, so it was stopped.")]
    Timeout(u64),
    #[error("The command was stopped.")]
    Cancelled,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The saved machine a command runs on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Machine {
    pub provider: CloudProvider,
    pub account: String,
    pub id: String,
}

/// How a command ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Finished {
    /// Its exit code; `None` when a signal ended it.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Whether either stream was longer than its kept tail.
    pub truncated: bool,
}

/// The shell line for `argv`, each word quoted, run in `cwd` when given. A
/// `cwd` of `~` or under `~/` is under the machine's home directory.
pub fn script(argv: &[String], cwd: Option<&str>) -> Result<String> {
    if argv.first().is_none_or(String::is_empty) {
        return Err(Error::Empty);
    }
    let command = argv
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" ");
    let directory = |cwd: &str| match cwd.strip_prefix('~') {
        Some("") => "\"$HOME\"".to_owned(),
        Some(rest) if rest.starts_with('/') => format!("\"$HOME\"{}", shell_quote(rest)),
        _ => shell_quote(cwd),
    };
    Ok(match cwd {
        Some(cwd) => format!("cd {} && exec {command}", directory(cwd)),
        None => format!("exec {command}"),
    })
}

/// Run `argv` on `machine` for at most `timeout`. Blocks: run it on a worker.
/// Reaching the machine may first wait for it to start, which `cancelled`
/// stops; the timeout counts from now, so that wait comes out of the
/// command's time, and a command is not started once it has run out.
pub fn run(
    machine: &Machine,
    argv: &[String],
    cwd: Option<&str>,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<Finished> {
    let deadline = Instant::now() + timeout;
    let script = script(argv, cwd)?;
    super::with_command(
        machine.provider,
        &machine.account,
        &machine.id,
        cancelled,
        |mut command| {
            command.arg(format!("/bin/sh -c {}", shell_quote(&script)));
            until(deadline, timeout, |left| capture(command, left, cancelled))
        },
    )
    .map_err(Error::Reach)?
}

/// Run `start` with the time left before `deadline`, unless none is; either
/// way a timeout reports the whole `timeout`.
fn until(
    deadline: Instant,
    timeout: Duration,
    start: impl FnOnce(Duration) -> Result<Finished>,
) -> Result<Finished> {
    let left = deadline.saturating_duration_since(Instant::now());
    let result = if left.is_zero() {
        Err(Error::Timeout(0))
    } else {
        start(left)
    };
    result.map_err(|error| match error {
        Error::Timeout(_) => Error::Timeout(timeout.as_secs()),
        error => error,
    })
}

/// Run `command` with no input, keeping the tail of each stream.
pub fn capture(
    mut command: Command,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<Finished> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::Spawn)?;
    let stdout = Tail::new(STDOUT_LIMIT);
    let stderr = Tail::new(STDERR_LIMIT);
    let readers = [
        child.stdout.take().map(|pipe| stdout.drain(pipe)),
        child.stderr.take().map(|pipe| stderr.drain(pipe)),
    ];
    let code = wait(&mut child, timeout, cancelled)?;
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    let (stdout, cut_out) = stdout.text();
    let (stderr, cut_err) = stderr.text();
    Ok(Finished {
        code,
        stdout,
        stderr,
        truncated: cut_out || cut_err,
    })
}

/// Wait for `child`, killing it on timeout or cancellation. A killed child's
/// readers are left alone: a process it started may still hold the pipes.
fn wait(child: &mut Child, timeout: Duration, cancelled: &AtomicBool) -> Result<Option<i32>> {
    let deadline = Instant::now() + timeout;
    let stop = loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) => {}
            Err(error) => break Error::Wait(error),
        }
        if cancelled.load(Ordering::Acquire) {
            break Error::Cancelled;
        }
        if Instant::now() >= deadline {
            break Error::Timeout(timeout.as_secs());
        }
        thread::sleep(POLL);
    };
    let _ = child.kill();
    let _ = child.wait();
    Err(stop)
}

/// The last `limit` bytes a pipe produced, read to its end so a chatty
/// command never blocks on a full pipe.
#[derive(Clone)]
struct Tail {
    limit: usize,
    kept: Arc<Mutex<(Vec<u8>, bool)>>,
}

impl Tail {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            kept: Arc::default(),
        }
    }

    fn drain(&self, mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<()> {
        let tail = self.clone();
        thread::spawn(move || {
            let mut chunk = [0; 8192];
            while let Ok(read) = pipe.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                let mut kept = tail.kept.lock().unwrap_or_else(PoisonError::into_inner);
                kept.0.extend_from_slice(&chunk[..read]);
                let excess = kept.0.len().saturating_sub(tail.limit);
                if excess > 0 {
                    kept.0.drain(..excess);
                    kept.1 = true;
                }
            }
        })
    }

    /// The text kept, without terminal controls, and whether any was cut.
    /// Decoding can grow it, as each invalid byte becomes a three-byte
    /// replacement character, so the text is bounded again after.
    fn text(&self) -> (String, bool) {
        let kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        let mut text = String::with_capacity(kept.0.len());
        let mut escape = false;
        for c in String::from_utf8_lossy(&kept.0).chars() {
            match c {
                '\u{1b}' => escape = true,
                c if escape => escape = !c.is_ascii_alphabetic(),
                '\n' | '\t' => text.push(c),
                c if c.is_control() => {}
                c => text.push(c),
            }
        }
        if text.len() <= self.limit {
            return (text, kept.1);
        }
        let start = (text.len() - self.limit..text.len())
            .find(|&index| text.is_char_boundary(index))
            .unwrap_or(text.len());
        text.drain(..start);
        (text, true)
    }
}

#[cfg(all(test, unix))]
mod tests;
