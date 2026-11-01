//! The thread that lists this computer's ports and signals their processes,
//! so neither a slow `lsof` nor a process taking its time to stop ever
//! blocks the window.

use super::{Command, Error, INTERVAL, Listener, Update, Worker, process, scan};
use crate::Result;
use herdr_client::{ScriptHost, ScriptLimits, run_script};
use std::{
    sync::{Arc, atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};
use sysinfo::System;

/// A failed scan is tried again after this long.
const RETRY: Duration = Duration::from_secs(30);
/// After a signal the next listing comes sooner, to show what it did.
const SETTLE: Duration = Duration::from_millis(300);
/// How long a stopped process has to exit before it is killed.
const GRACE: Duration = Duration::from_secs(3);
const LIMITS: ScriptLimits = ScriptLimits {
    output: 512 * 1024,
    idle: Duration::from_secs(15),
};

/// Starts the worker thread.
pub(super) fn start() -> Result<Worker> {
    let (commands, received) = mpsc::sync_channel(2);
    let (sender, updates) = mpsc::sync_channel(4);
    let cancelled = Arc::new(AtomicBool::new(false));
    let shared = cancelled.clone();
    thread::Builder::new()
        .name("herdr-local-ports".into())
        .spawn(move || run(&received, &sender, &shared))
        .map_err(Error::Worker)?;
    Ok(Worker {
        commands,
        updates,
        cancelled,
    })
}

fn run(
    commands: &mpsc::Receiver<Command>,
    updates: &mpsc::SyncSender<Update>,
    cancelled: &AtomicBool,
) {
    // Linux `sysinfo` otherwise keeps every process's stat file open, which
    // this app's terminals need. Elsewhere a no-op.
    sysinfo::set_open_files_limit(0);
    let mut system = System::new();
    let user = process::user();
    let mut due = Instant::now();
    loop {
        match commands.recv_timeout(due.saturating_duration_since(Instant::now())) {
            Ok(Command::Signal(action, victims)) => {
                let outcomes = victims
                    .into_iter()
                    .map(|victim| {
                        let outcome = process::act(&mut system, action, victim, user, GRACE);
                        (victim, outcome)
                    })
                    .collect();
                // Blocks while the UI is behind; a closed section ends it.
                if updates.send(Update::Signalled(action, outcomes)).is_err() {
                    return;
                }
                due = Instant::now() + SETTLE;
                continue;
            }
            Ok(Command::Refresh) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        let result = scan(&mut system, user, cancelled);
        due = Instant::now() + if result.is_ok() { INTERVAL } else { RETRY };
        if updates.send(Update::Listing(result)).is_err() {
            return;
        }
    }
}

fn scan(system: &mut System, user: u32, cancelled: &AtomicBool) -> Result<Vec<Listener>> {
    let mut output = Vec::new();
    run_script(
        ScriptHost::Local,
        scan::SCRIPT,
        std::io::empty(),
        &mut output,
        LIMITS,
        cancelled,
    )
    .map_err(Error::Scan)?;
    let sockets = scan::parse(&String::from_utf8_lossy(&output)).ok_or(Error::Tool)?;
    Ok(process::describe(system, sockets, user))
}
