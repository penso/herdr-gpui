//! Running one step's command: output to its log file, a timeout, a stop
//! request, and CPU and memory sampled from the whole process tree. On Unix
//! the command gets its own process group, so stopping it stops everything
//! it started.

use crate::{Error, Result};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Child, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// The end of a log kept for the script's token and the run's record.
pub const TAIL_BYTES: u64 = 16 * 1024;
/// The most lines a step's tail keeps.
pub const TAIL_LINES: usize = 200;
const POLL: Duration = Duration::from_millis(100);
const SAMPLE: Duration = Duration::from_secs(1);
/// How long a stopped command gets to exit before it is killed.
#[cfg(unix)]
const GRACE: Duration = Duration::from_secs(5);

pub struct Job<'a> {
    pub program: &'a OsStr,
    pub args: &'a [&'a OsStr],
    pub cwd: Option<&'a Path>,
    pub env: &'a [(String, String)],
    pub log: &'a Path,
    pub timeout: Duration,
    pub stop: &'a AtomicBool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Finished {
    pub exit: Option<i32>,
    pub timed_out: bool,
    pub stopped: bool,
    pub log_tail: String,
    pub cpu_ms: Option<u64>,
    pub peak_memory: Option<u64>,
}

impl Finished {
    pub fn ok(&self) -> bool {
        self.exit == Some(0) && !self.timed_out && !self.stopped
    }
}

pub fn run(job: &Job<'_>) -> Result<Finished> {
    if let Some(parent) = job.log.parent() {
        crate::paths::private_dir(parent).map_err(Error::io(parent))?;
    }
    let log = crate::paths::private_file()
        .create(true)
        .append(true)
        .open(job.log)
        .map_err(Error::io(job.log))?;
    let mut command = sheprd_client::new_command(job.program);
    command
        .args(job.args)
        .envs(job.env.iter().map(|(name, value)| (name, value)))
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(Error::io(job.log))?)
        .stderr(log);
    if let Some(cwd) = job.cwd {
        command.current_dir(cwd);
    }
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(Error::io(job.program))?;
    let mut meter = Meter::new(child.id());
    let started = Instant::now();
    let mut sampled = Instant::now();
    let mut timed_out = false;
    let mut stopped = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(Error::io(job.program))? {
            break status;
        }
        if sampled.elapsed() >= SAMPLE {
            meter.sample();
            sampled = Instant::now();
        }
        if started.elapsed() >= job.timeout {
            timed_out = true;
        } else if job.stop.load(Ordering::Relaxed) {
            stopped = true;
        }
        if timed_out || stopped {
            break end(&mut child);
        }
        thread::sleep(POLL);
    };
    meter.sample();
    Ok(Finished {
        exit: status.code(),
        timed_out,
        stopped,
        log_tail: tail(job.log, TAIL_BYTES)
            .map(|(text, _)| text)
            .unwrap_or_default(),
        cpu_ms: meter.cpu_ms(),
        peak_memory: meter.peak_memory(),
    })
}

/// Asks the command's process group to stop, then kills what is left of
/// it. The command exiting is not enough: a worker it started that ignores
/// the request would carry on after the run is reported stopped, so the
/// grace period lasts until the whole group is gone.
fn end(child: &mut Child) -> std::process::ExitStatus {
    #[cfg(unix)]
    if let Some(group) = rustix::process::Pid::from_raw(i32::try_from(child.id()).unwrap_or(0)) {
        use rustix::process::{Signal, kill_process_group, test_kill_process_group};
        let _ = kill_process_group(group, Signal::TERM);
        let deadline = Instant::now() + GRACE;
        let mut exited = None;
        while Instant::now() < deadline {
            if exited.is_none()
                && let Ok(Some(status)) = child.try_wait()
            {
                exited = Some(status);
            }
            if exited.is_some() && test_kill_process_group(group).is_err() {
                break;
            }
            thread::sleep(POLL);
        }
        let _ = kill_process_group(group, Signal::KILL);
        if let Some(status) = exited {
            return status;
        }
    }
    let _ = child.kill();
    match child.wait() {
        Ok(status) => status,
        // The child is gone either way; report it as killed.
        Err(_) => killed(),
    }
}

#[cfg(unix)]
fn killed() -> std::process::ExitStatus {
    std::os::unix::process::ExitStatusExt::from_raw(9)
}

#[cfg(windows)]
fn killed() -> std::process::ExitStatus {
    std::os::windows::process::ExitStatusExt::from_raw(1)
}

/// The last `limit` bytes of a file, cut to whole lines and to at most
/// `max_lines` of them, and whether the file was longer.
pub fn tail_lines(path: &Path, limit: u64, max_lines: usize) -> std::io::Result<(String, bool)> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let start = size.saturating_sub(limit);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(usize::try_from(size - start).unwrap_or(0));
    file.take(limit).read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines().collect::<Vec<_>>();
    if start > 0 && !lines.is_empty() {
        // The first line is a fragment of a longer one.
        lines.remove(0);
    }
    let keep = lines.len().saturating_sub(max_lines);
    Ok((
        lines.get(keep..).unwrap_or_default().join("\n"),
        start > 0 || keep > 0,
    ))
}

/// A step's tail: the last [`TAIL_BYTES`], at most [`TAIL_LINES`] lines.
pub fn tail(path: &Path, limit: u64) -> std::io::Result<(String, bool)> {
    tail_lines(path, limit, TAIL_LINES)
}

/// CPU time and peak memory of a process and everything below it, from
/// samples: a child that starts and ends between two samples is missed.
pub struct Meter {
    root: Pid,
    system: System,
    cpu: HashMap<Pid, u64>,
    peak: u64,
    seen: bool,
}

impl Meter {
    pub fn new(root: u32) -> Self {
        Self {
            root: Pid::from_u32(root),
            system: System::new(),
            cpu: HashMap::new(),
            peak: 0,
            seen: false,
        }
    }

    pub fn sample(&mut self) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let processes = self.system.processes();
        let mut tree = vec![self.root];
        let mut index = 0;
        while let Some(pid) = tree.get(index).copied() {
            tree.extend(
                processes
                    .iter()
                    .filter(|(_, process)| {
                        process.parent() == Some(pid) && process.thread_kind().is_none()
                    })
                    .map(|(child, _)| *child),
            );
            index += 1;
            if tree.len() > 4_096 {
                break;
            }
        }
        let mut memory = 0;
        for pid in &tree {
            if let Some(process) = processes.get(pid) {
                self.seen = true;
                memory += process.memory();
                // Keep each process's highest reading: one that ended keeps
                // the time it had used by its last sample.
                let used = self.cpu.entry(*pid).or_default();
                *used = (*used).max(process.accumulated_cpu_time());
            }
        }
        self.peak = self.peak.max(memory);
    }

    pub fn cpu_ms(&self) -> Option<u64> {
        self.seen.then(|| self.cpu.values().sum())
    }

    pub fn peak_memory(&self) -> Option<u64> {
        self.seen.then_some(self.peak)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
