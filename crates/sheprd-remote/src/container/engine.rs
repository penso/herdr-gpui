//! The container engine's CLI: finding it, and running one bounded command.
//! A GUI app on macOS starts with a minimal `PATH`, so the usual install
//! places are searched as well, and commands get the login environment.

use super::{Error, Result};
use std::{
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const SEARCH: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"];
/// Output kept from one command; inspections and listings fit easily.
const OUTPUT_LIMIT: usize = 64 * 1024;

/// A container engine with a Docker-compatible CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Docker,
    Podman,
}

impl Engine {
    pub const ALL: [Self; 2] = [Self::Docker, Self::Podman];

    /// The key saved as a device's account and written as `[container] engine`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|engine| engine.key() == key)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Docker => "Docker",
            Self::Podman => "Podman",
        }
    }

    /// The engine's executable, if installed.
    pub fn find(self) -> Option<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        // OrbStack and Rancher Desktop install their CLI under the home directory.
        let user = home
            .iter()
            .flat_map(|home| [home.join(".orbstack/bin"), home.join(".rd/bin")]);
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .chain(SEARCH.iter().map(PathBuf::from))
            .chain(user)
            .map(|dir| dir.join(self.key()))
            .find(|candidate| candidate.is_file())
    }

    /// `preferred`, or else the first engine installed.
    pub fn pick(preferred: Option<Self>) -> Result<Self> {
        match preferred {
            Some(engine) => Ok(engine),
            None => Self::ALL
                .into_iter()
                .find(|engine| engine.find().is_some())
                .ok_or(Error::NoEngine),
        }
    }

    /// A command running the engine with `args`.
    pub fn command<I, S>(self, args: I) -> Result<Command>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let binary = self.find().ok_or(Error::NoEngine)?;
        let mut command = sheprd_client::new_command(binary);
        sheprd_client::login_env::apply(&mut command);
        command.args(args);
        Ok(command)
    }

    /// Run the engine with `args` and return its trimmed standard output.
    /// Fails with the end of its error output when it exits unsuccessfully.
    pub fn run<I, S>(
        self,
        action: &'static str,
        args: I,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let (status, stdout, stderr) = self.output(action, args, timeout, cancelled)?;
        if status != Some(0) {
            return Err(Error::Failed {
                engine: self.key(),
                action,
                detail: detail(&stderr),
            });
        }
        Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
    }

    /// Run the engine with `args` and report its exit code and both outputs,
    /// for a caller that reads a failure itself, such as "no such container".
    pub fn output<I, S>(
        self,
        action: &'static str,
        args: I,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(Option<i32>, Vec<u8>, Vec<u8>)>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut child = self
            .command(args)?
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Error::Spawn {
                engine: self.key(),
                source,
            })?;
        let stdout = Arc::new(Mutex::new(Vec::new()));
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let readers = [
            child.stdout.take().map(|pipe| drain(pipe, stdout.clone())),
            child.stderr.take().map(|pipe| drain(pipe, stderr.clone())),
        ];
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(source) => {
                    break Err(Error::Spawn {
                        engine: self.key(),
                        source,
                    });
                }
            }
            if cancelled() {
                break Err(Error::Cancelled);
            }
            if Instant::now() >= deadline {
                break Err(Error::Timeout {
                    engine: self.key(),
                    action,
                });
            }
            thread::sleep(Duration::from_millis(50));
        };
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        for reader in readers.into_iter().flatten() {
            let _ = reader.join();
        }
        let take = |buffer: &Mutex<Vec<u8>>| {
            std::mem::take(&mut *buffer.lock().unwrap_or_else(|error| error.into_inner()))
        };
        Ok((status.code(), take(&stdout), take(&stderr)))
    }
}

impl Engine {
    /// Run the engine with `args`, passing each line it prints, from either
    /// stream, to `line` as it comes, such as a build's progress. Fails with
    /// its last line when it exits unsuccessfully.
    pub fn stream<I, S>(
        self,
        action: &'static str,
        args: I,
        timeout: Duration,
        cancelled: &dyn Fn() -> bool,
        line: &dyn Fn(&str),
    ) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut child = self
            .command(args)?
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Error::Spawn {
                engine: self.key(),
                source,
            })?;
        let (sender, lines) = std::sync::mpsc::channel::<String>();
        let readers = [
            child
                .stdout
                .take()
                .map(|pipe| lines_of(pipe, sender.clone())),
            child.stderr.take().map(|pipe| lines_of(pipe, sender)),
        ];
        let deadline = Instant::now() + timeout;
        let mut last = String::new();
        let status = loop {
            while let Ok(text) = lines.try_recv() {
                line(&text);
                if !text.trim().is_empty() {
                    last = text;
                }
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(source) => {
                    break Err(Error::Spawn {
                        engine: self.key(),
                        source,
                    });
                }
            }
            if cancelled() {
                break Err(Error::Cancelled);
            }
            if Instant::now() >= deadline {
                break Err(Error::Timeout {
                    engine: self.key(),
                    action,
                });
            }
            thread::sleep(Duration::from_millis(50));
        };
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        for reader in readers.into_iter().flatten() {
            let _ = reader.join();
        }
        for text in lines.try_iter() {
            line(&text);
            if !text.trim().is_empty() {
                last = text;
            }
        }
        if status.code() != Some(0) {
            return Err(Error::Failed {
                engine: self.key(),
                action,
                detail: detail(last.as_bytes()),
            });
        }
        Ok(())
    }
}

/// Send each line of a pipe, bounded, until it ends.
fn lines_of(
    pipe: impl Read + Send + 'static,
    sender: std::sync::mpsc::Sender<String>,
) -> thread::JoinHandle<()> {
    const LINE_LIMIT: u64 = 16 * 1024;
    thread::spawn(move || {
        let mut reader = std::io::BufReader::new(pipe);
        loop {
            let mut bytes = Vec::new();
            match std::io::BufRead::read_until(
                &mut (&mut reader).take(LINE_LIMIT),
                b'\n',
                &mut bytes,
            ) {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&bytes).trim_end().to_owned();
                    if sender.send(text).is_err() {
                        return;
                    }
                }
            }
        }
    })
}

/// Keep the first `OUTPUT_LIMIT` bytes of a pipe, draining it to the end so
/// the child never blocks on a full pipe.
fn drain(
    mut pipe: impl Read + Send + 'static,
    into: Arc<Mutex<Vec<u8>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0; 4096];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let mut kept = into.lock().unwrap_or_else(|error| error.into_inner());
            let room = OUTPUT_LIMIT.saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..read.min(room)]);
        }
    })
}

/// The last line of an engine's error output, without controls, bounded.
pub(super) fn detail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("no error output");
    line.chars().filter(|c| !c.is_control()).take(300).collect()
}
