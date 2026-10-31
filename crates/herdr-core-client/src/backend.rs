//! herdr-core as a `herdr_backend::Backend`: terminal records become
//! `TerminalInfo`, events become changes, and attached terminals are
//! emulated here so the app receives cells as it does from Herdr 1.

mod terminal;

use crate::{
    Core, Error, Result,
    event::{Notice, TerminalChange},
    wire::{Run, Status, StatusState, Terminal, TerminalState},
};
use herdr_backend::{
    AgentStatus, Change, Exit, Generation, Role, RunState, Size, Spawn, TerminalId, TerminalInfo,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

pub use terminal::CoreTerminal;

/// One core's terminals, shared by the workers that list, watch, and attach.
#[derive(Clone)]
pub struct CoreBackend {
    core: Arc<Core>,
    /// Scrollback lines each attached terminal keeps.
    scrollback: usize,
}

impl CoreBackend {
    pub fn new(core: Core, scrollback: usize) -> Self {
        Self {
            core: Arc::new(core),
            scrollback,
        }
    }

    pub fn core(&self) -> &Core {
        &self.core
    }
}

impl herdr_backend::Backend for CoreBackend {
    type Changes = CoreChanges;
    type Error = Error;
    type Terminal = CoreTerminal;

    fn generation(&self) -> Generation {
        Generation::Herdr2
    }

    fn terminals(&self) -> Result<Vec<TerminalInfo>> {
        self.core.list(false)?.terminals.iter().map(info).collect()
    }

    fn watch(&self) -> Result<(Vec<TerminalInfo>, CoreChanges)> {
        let subscription = self.core.subscribe()?;
        let records: BTreeMap<String, Terminal> = subscription
            .terminals
            .iter()
            .map(|terminal| (terminal.address.clone(), terminal.clone()))
            .collect();
        let snapshot = records.values().map(info).collect::<Result<_>>()?;
        let changes = CoreChanges {
            records,
            seq: subscription.seq,
            subscription,
        };
        Ok((snapshot, changes))
    }

    fn attach(&self, id: &TerminalId, size: Size, role: Role) -> Result<CoreTerminal> {
        CoreTerminal::attach(&self.core, id, size, role, self.scrollback)
    }

    fn spawn(&self, spawn: &Spawn) -> Result<TerminalInfo> {
        let cwd = spawn.cwd.as_deref().map(|cwd| cwd.to_string_lossy());
        let run = Run {
            argv: &spawn.argv,
            cwd: cwd.as_deref(),
            env: spawn
                .env
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect(),
            // A terminal the user opened outlives its program's loss.
            keep: true,
            name: spawn.name.as_deref(),
            ..Run::default()
        };
        info(&self.core.run(&run)?)
    }

    fn kill(&self, id: &TerminalId) -> Result<()> {
        self.core.kill(id.as_str()).map(drop)
    }
}

/// Events after a watch snapshot, applied to the records they change so
/// every change carries the terminal's whole current facts.
pub struct CoreChanges {
    records: BTreeMap<String, Terminal>,
    seq: u64,
    subscription: crate::Subscription,
}

impl herdr_backend::Changes for CoreChanges {
    type Error = Error;

    fn next(&mut self, timeout: Duration) -> Result<Vec<Change>> {
        let mut changes = Vec::new();
        for notice in self.subscription.next(timeout)? {
            match notice {
                Notice::Heartbeat { .. } => {}
                Notice::Missed { seq, terminals } => {
                    self.seq = seq;
                    self.records = terminals
                        .into_iter()
                        .map(|terminal| (terminal.address.clone(), terminal))
                        .collect();
                    changes.clear();
                    changes.push(Change::Resync(
                        self.records.values().map(info).collect::<Result<_>>()?,
                    ));
                }
                // Events from before a missed snapshot are already in it.
                Notice::Event(event) if event.seq <= self.seq => {}
                Notice::Event(event) => {
                    self.seq = event.seq;
                    if let Some(change) = self.apply(event.terminal, event.change)? {
                        changes.push(change);
                    }
                }
            }
        }
        Ok(changes)
    }
}

impl CoreChanges {
    fn apply(&mut self, address: String, change: TerminalChange) -> Result<Option<Change>> {
        if let TerminalChange::Removed = change {
            self.records.remove(&address);
            return TerminalId::new(address)
                .map(Change::Removed)
                .map(Some)
                .map_err(Error::from);
        }
        let record = match change {
            TerminalChange::Created(record) | TerminalChange::Restarted(record) => {
                self.records.insert(address.clone(), *record);
                self.records.get_mut(&address)
            }
            TerminalChange::Labels {
                record: Some(record),
                ..
            } => {
                self.records.insert(address.clone(), *record);
                self.records.get_mut(&address)
            }
            change => {
                let Some(record) = self.records.get_mut(&address) else {
                    return Ok(None);
                };
                if !update(record, change) {
                    return Ok(None);
                }
                Some(record)
            }
        };
        record
            .map(|record| info(record).map(Change::Upsert))
            .transpose()
    }
}

/// Applies one event to its record, as herdr-core's API documents it;
/// false when nothing the app shows changed.
fn update(record: &mut Terminal, change: TerminalChange) -> bool {
    match change {
        TerminalChange::Finished(ended) => {
            record.state = TerminalState::Finished;
            record.exit_code = ended.exit_code;
            record.reason = ended.reason;
            record.foreground = None;
            record.status = None;
        }
        TerminalChange::Foreground(foreground) => record.foreground = foreground,
        TerminalChange::Keep(keep) => record.keep = keep,
        TerminalChange::Labels { labels, .. } => record.labels = labels,
        TerminalChange::Lost(ended) => {
            record.exit_code = ended.exit_code;
            record.reason = ended.reason;
            record.pid = None;
            record.foreground = None;
        }
        TerminalChange::Size { cols, rows } => (record.cols, record.rows) = (cols, rows),
        TerminalChange::Status { status, .. } => record.status = status,
        TerminalChange::Title(title) => record.title = title,
        TerminalChange::Turn { .. } | TerminalChange::Unknown(_) => return false,
        TerminalChange::Created(_) | TerminalChange::Removed | TerminalChange::Restarted(_) => {
            return false;
        }
    }
    true
}

pub(crate) fn info(terminal: &Terminal) -> Result<TerminalInfo> {
    let cwd = terminal
        .foreground
        .as_ref()
        .map(|foreground| foreground.cwd.as_str())
        .filter(|cwd| !cwd.is_empty())
        .or(Some(terminal.cwd.as_str()).filter(|cwd| !cwd.is_empty()))
        .map(PathBuf::from);
    Ok(TerminalInfo {
        agent: terminal.status.as_ref().map_or(AgentStatus::Unknown, agent),
        cwd,
        id: TerminalId::new(terminal.address.clone())?,
        name: terminal.name.clone(),
        state: run_state(terminal),
        title: terminal.title.clone().filter(|title| !title.is_empty()),
    })
}

/// herdr-core's error state asks for the user's attention, as blocked does.
fn agent(status: &Status) -> AgentStatus {
    match status.state {
        StatusState::Blocked | StatusState::Error => AgentStatus::Blocked,
        StatusState::Done => AgentStatus::Done,
        StatusState::Idle => AgentStatus::Idle,
        StatusState::Working => AgentStatus::Working,
        StatusState::Other => AgentStatus::Unknown,
    }
}

fn run_state(terminal: &Terminal) -> RunState {
    match terminal.state {
        TerminalState::Finished => RunState::Exited(Exit {
            code: terminal.exit_code,
        }),
        // A kept terminal stays live after its program is lost.
        TerminalState::Live if terminal.pid.is_none() && terminal.reason.is_some() => {
            RunState::Lost
        }
        TerminalState::Live | TerminalState::Other => RunState::Running,
    }
}
