//! What the OS says about the processes behind listening sockets, and
//! signalling exactly one of them.
//!
//! A pid is reused once its process exits, so every signal first looks the
//! pid up again and goes ahead only while it still names the process the
//! user chose: the same start time, owned by this user. The signal goes to
//! that pid alone, never to a group or a name.

use super::{Listener, Outcome, scan::Socket};
use crate::processes::Identity;
use herdr_control_protocol::protocol::hosts::PortAction;
use std::{
    thread,
    time::{Duration, Instant},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, Signal, System, UpdateKind};

/// A process name is display text; longer ones are cut.
const NAME_LIMIT: usize = 64;
/// How often a stopping process is looked at while it has time to exit.
const POLL: Duration = Duration::from_millis(100);

/// This process's real user, whose processes alone are listed and signalled.
pub(super) fn user() -> u32 {
    rustix::process::getuid().as_raw()
}

/// The process `pid` names now, as far as signalling cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub started: u64,
    /// `None` when the OS would not say.
    pub user: Option<u32>,
    /// Exited, and only waiting for its parent to reap it.
    pub zombie: bool,
}

/// Whether `found` is still `victim`, a process of `user` that is running.
pub(crate) fn check(found: Option<Found>, victim: Identity, user: u32) -> Result<(), Outcome> {
    let found = found.ok_or(Outcome::Exited)?;
    if found.zombie || found.started != victim.started {
        return Err(Outcome::Exited);
    }
    if found.user.is_some_and(|owner| owner != user) {
        return Err(Outcome::Foreign);
    }
    Ok(())
}

fn refresh(system: &mut System, pids: &[Pid]) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(pids),
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_user(UpdateKind::OnlyIfNotSet),
    );
}

fn found(system: &System, pid: u32) -> Option<Found> {
    let process = system.process(Pid::from_u32(pid))?;
    Some(Found {
        started: process.start_time(),
        user: process.user_id().map(|uid| **uid),
        zombie: matches!(process.status(), ProcessStatus::Zombie | ProcessStatus::Dead),
    })
}

/// The rows `sockets` make: each with its process's identity, name, working
/// directory, and state, leaving out sockets whose process exited since or
/// belongs to another user.
pub(super) fn describe(system: &mut System, sockets: Vec<Socket>, user: u32) -> Vec<Listener> {
    let mut pids: Vec<Pid> = sockets
        .iter()
        .map(|socket| Pid::from_u32(socket.pid))
        .collect();
    pids.sort_unstable();
    pids.dedup();
    refresh(system, &pids);
    sockets
        .into_iter()
        .filter_map(|socket| {
            let process = system.process(Pid::from_u32(socket.pid))?;
            if process
                .user_id()
                .is_some_and(|uid| **uid != user)
            {
                return None;
            }
            // `lsof` cuts names at nine characters; the OS's own is longer.
            let name: String = process
                .name()
                .to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(NAME_LIMIT)
                .collect();
            Some(Listener {
                identity: Identity {
                    pid: socket.pid,
                    started: process.start_time(),
                },
                number: socket.number,
                bind: socket.bind,
                process: if name.trim().is_empty() {
                    socket.process
                } else {
                    name
                },
                directory: process.cwd().map(std::path::Path::to_path_buf),
                paused: process.status() == ProcessStatus::Stop,
            })
        })
        .collect()
}

/// Looks `pid` up afresh.
fn look(system: &mut System, pid: u32) -> Option<Found> {
    refresh(system, &[Pid::from_u32(pid)]);
    found(system, pid)
}

fn send(system: &System, pid: u32, signal: Signal) -> bool {
    system
        .process(Pid::from_u32(pid))
        .and_then(|process| process.kill_with(signal))
        .unwrap_or(false)
}

/// Does `action` to `victim` if it is still that process. Stopping sends
/// SIGTERM, and SIGCONT so a paused process can act on it, then SIGKILL if
/// the same process still runs after `grace`. Blocking: a worker's call.
pub(super) fn act(
    system: &mut System,
    action: PortAction,
    victim: Identity,
    user: u32,
    grace: Duration,
) -> Outcome {
    if let Err(outcome) = check(look(system, victim.pid), victim, user) {
        return outcome;
    }
    let signal = match action {
        PortAction::Stop => Signal::Term,
        PortAction::Pause => Signal::Stop,
        PortAction::Resume => Signal::Continue,
    };
    if !send(system, victim.pid, signal) {
        return Outcome::Failed;
    }
    if action != PortAction::Stop {
        return Outcome::Signalled;
    }
    send(system, victim.pid, Signal::Continue);
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        thread::sleep(POLL);
        if check(look(system, victim.pid), victim, user).is_err() {
            return Outcome::Signalled;
        }
    }
    if check(look(system, victim.pid), victim, user).is_err() {
        return Outcome::Signalled;
    }
    if send(system, victim.pid, Signal::Kill) {
        Outcome::Killed
    } else {
        Outcome::Failed
    }
}
