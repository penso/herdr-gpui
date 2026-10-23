//! `processes`, answered by this process: the tree under a pane with CPU and
//! memory, sampled here as the app's process list samples it. Herdr's own
//! `pane process-info` names the pane's processes; this adds their tree and
//! usage. Only a daemon on this machine is asked.
use super::Output;
use crate::control::client::hosts::bytes;
use crate::{
    control::{ErrorCode, client::env_id},
    processes::{Daemon, Listing, Update, Watch},
};
use serde::Serialize;
use std::{
    fmt::Write as _,
    time::{Duration, Instant},
};

/// How long to wait for a listing whose CPU is measured: the third sample
/// is, well under a second after the first once the CLI has answered.
const WAIT: Duration = Duration::from_secs(7);
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Process {
    pub pid: u32,
    pub parent: Option<u32>,
    pub depth: usize,
    pub name: String,
    pub command: String,
    /// Percent of one core; absent until measured.
    pub cpu: Option<f32>,
    /// Resident bytes.
    pub memory: u64,
    pub foreground: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Tree {
    pub pane: String,
    pub processes: Vec<Process>,
    /// Processes beyond the row limit, still counted in the totals.
    pub hidden: usize,
    pub cpu: f32,
    pub memory: u64,
}

impl Tree {
    pub(super) fn new(pane: &str, listing: &Listing) -> Self {
        Self {
            pane: pane.to_owned(),
            processes: listing
                .rows
                .iter()
                .map(|row| Process {
                    pid: row.entry.identity.pid,
                    parent: row.entry.parent,
                    depth: row.depth,
                    name: row.entry.name.clone(),
                    command: row.entry.command.clone(),
                    cpu: row.entry.cpu,
                    memory: row.entry.memory,
                    foreground: row.foreground,
                })
                .collect(),
            hidden: listing.hidden,
            cpu: listing.cpu,
            memory: listing.memory,
        }
    }
}

/// One line per process, indented under its parent; `*` marks the
/// terminal's foreground job.
pub(super) fn text(tree: &Tree) -> String {
    let mut text = format!("{:>8} {:>6} {:>10}  COMMAND\n", "PID", "CPU", "MEMORY");
    for process in &tree.processes {
        let cpu = process
            .cpu
            .map_or_else(|| "-".to_owned(), |cpu| format!("{cpu:.1}%"));
        let _ = write!(
            text,
            "{:>8} {cpu:>6} {:>10}  {}{}{}",
            process.pid,
            bytes(process.memory),
            "  ".repeat(process.depth),
            if process.foreground { "*" } else { "" },
            process.name,
        );
        if !process.command.is_empty() && process.command != process.name {
            let _ = write!(text, "  {}", process.command);
        }
        text.push('\n');
    }
    let count = tree.processes.len() + tree.hidden;
    let _ = write!(
        text,
        "{count} {} under pane {}, {:.1}% CPU, {}",
        if count == 1 { "process" } else { "processes" },
        tree.pane,
        tree.cpu,
        bytes(tree.memory)
    );
    if tree.hidden > 0 {
        let _ = write!(text, " ({} not listed)", tree.hidden);
    }
    text
}

/// Whether every listed process's CPU is measured yet.
fn measured(listing: &Listing) -> bool {
    listing.rows.iter().all(|row| row.entry.cpu.is_some())
}

/// Waits for the worker's first listing with CPU measured, or the latest
/// one when time runs out. A failed sample is the answer: the next is ten
/// seconds away.
fn sample(watch: &Watch, wait: Duration) -> crate::Result<Listing> {
    let until = Instant::now() + wait;
    let mut latest = None;
    loop {
        while let Some(update) = watch.try_update()? {
            if let Update::Listing(listing) = update {
                let listing = listing?;
                if measured(&listing) {
                    return Ok(listing);
                }
                latest = Some(listing);
            }
        }
        if Instant::now() >= until {
            return latest.ok_or_else(|| {
                crate::control::Error::Rejected {
                    code: ErrorCode::Timeout,
                    message: "Herdr did not describe the pane's processes in time".into(),
                }
                .into()
            });
        }
        std::thread::sleep(POLL);
    }
}

pub(crate) fn processes(pane: Option<&str>) -> crate::Result<Output> {
    let Some(pane) = pane.map(str::to_owned).or_else(|| env_id("HERDR_PANE_ID")) else {
        return Err(crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            message: "Name a pane with --pane ID, or run this in a Herdr pane".into(),
        }
        .into());
    };
    // Herdr's CLI, which this asks, finds the caller's own daemon from the
    // environment Herdr gave the pane, as it does when an agent runs it.
    let Some(daemon) = Daemon::for_target(&herdr_client::ConnectTarget::Local) else {
        return Err(crate::control::Error::Rejected {
            code: ErrorCode::Unsupported,
            message: "Listing a pane's processes needs Linux or macOS".into(),
        }
        .into());
    };
    let watch = Watch::start(daemon, pane.clone())?;
    let tree = Tree::new(&pane, &sample(&watch, WAIT)?);
    Output::data(text(&tree), &tree)
}

#[cfg(test)]
mod tests;
