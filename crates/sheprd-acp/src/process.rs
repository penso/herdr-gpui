//! The agent's process: started in its own process group on Unix so that
//! stopping it also stops what it started, such as `npx`'s Node child.

use std::{
    collections::VecDeque,
    ffi::OsString,
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, ExitStatus, Stdio},
    sync::{Arc, Mutex, PoisonError},
    thread,
    time::Duration,
};

use crate::{Error, Result};

/// Lines of the agent's standard error kept to explain a failure.
const STDERR_LINES: usize = 20;
const STDERR_LINE: usize = 400;
const POLL: Duration = Duration::from_millis(50);
#[cfg(unix)]
const GRACE: Duration = Duration::from_secs(2);

pub(crate) struct Spawned {
    pub input: ChildStdin,
    pub output: ChildStdout,
    pub process: AgentProcess,
}

/// A handle that can stop the agent from any thread.
#[derive(Clone)]
pub(crate) struct AgentProcess {
    child: Arc<Mutex<Child>>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

pub(crate) fn spawn(program: &OsString, args: &[OsString], cwd: &Path) -> Result<Spawned> {
    let mut command = sheprd_client::new_command(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(|source| Error::AgentSpawn {
        program: program.to_string_lossy().into_owned(),
        source,
    })?;
    let input = child.stdin.take().ok_or(Error::AgentPipe("input"))?;
    let output = child.stdout.take().ok_or(Error::AgentPipe("output"))?;
    let stderr = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(errors) = child.stderr.take() {
        let tail = Arc::clone(&stderr);
        // Keeps the last lines only; the rest of standard error is discarded so
        // it never draws over the chat view.
        thread::Builder::new()
            .name("acp-stderr".to_owned())
            .spawn(move || {
                for line in BufReader::new(errors).lines() {
                    let Ok(line) = line else { return };
                    let mut tail = tail.lock().unwrap_or_else(PoisonError::into_inner);
                    if tail.len() == STDERR_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(line.chars().take(STDERR_LINE).collect());
                }
            })?;
    }
    Ok(Spawned {
        input,
        output,
        process: AgentProcess {
            child: Arc::new(Mutex::new(child)),
            stderr,
        },
    })
}

impl AgentProcess {
    fn lock(&self) -> std::sync::MutexGuard<'_, Child> {
        self.child.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits for the agent to exit, polling so that `stop` can still reach it.
    pub(crate) fn wait(&self) -> std::io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.lock().try_wait()? {
                return Ok(status);
            }
            thread::sleep(POLL);
        }
    }

    /// The last lines the agent wrote to standard error.
    pub(crate) fn stderr_tail(&self) -> Vec<String> {
        self.stderr
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }

    /// Asks the agent's process group to end, then kills what is left of it
    /// after a grace period. The agent exiting is not enough: a tool it
    /// started may ignore the request, so the wait lasts until the whole
    /// group is gone.
    pub(crate) fn stop(&self) {
        #[cfg(unix)]
        {
            use rustix::process::{Signal, kill_process_group, test_kill_process_group};
            let id = self.lock().id();
            let group = i32::try_from(id)
                .ok()
                .and_then(rustix::process::Pid::from_raw);
            if let Some(group) = group {
                let _ = kill_process_group(group, Signal::TERM);
            }
            let deadline = std::time::Instant::now() + GRACE;
            while std::time::Instant::now() < deadline {
                let agent_gone = matches!(self.lock().try_wait(), Ok(Some(_)) | Err(_));
                let group_gone = group.is_none_or(|group| test_kill_process_group(group).is_err());
                if agent_gone && group_gone {
                    return;
                }
                thread::sleep(POLL);
            }
            if let Some(group) = group {
                let _ = kill_process_group(group, Signal::KILL);
            }
        }
        let mut child = self.lock();
        let _ = child.kill();
        let _ = child.wait();
    }
}

// Process groups are a Unix notion; Windows has no test here.
#[cfg(all(test, unix))]
mod tests;
