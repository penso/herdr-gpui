//! The fan-out journal: every launched fan-out's lanes and the last step each
//! one reached, so a fan-out cut short by a quit or crash comes back on the
//! next launch instead of leaving worktrees and half-started agents nobody
//! tracks.
//!
//! A step is recorded twice: before its host script runs, as a step whose
//! outcome is unknown, and after it succeeds. A lane interrupted mid-step is
//! therefore never mistaken for one that finished, nor for one that never
//! started. Failures are not recorded, so a failed lane keeps the step it
//! failed in and is offered the same choices as an interrupted one.
//!
//! The journal only remembers; it never acts. Restoring a fan-out sends no
//! prompt and removes no worktree: the user does either from the comparison.
//!
//! Every change is made in memory first, under a short lock that never waits
//! for the disk, and returns a [`Pending`] write that the caller runs on a
//! worker. Host work writes before running the step it records, so the step
//! is on disk before its script starts.

use super::{
    error::{Error, Result},
    job::{Checkout, Progress, Report},
    plan::{Lane, MAX_LANES},
    state::Origin,
};
use crate::{state_file, teleport::AgentKind};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

const FILE: &str = "fan-outs.json";
/// Held for the life of the process that saves fan-outs; see [`Journal::load`].
const LOCK: &str = "fan-outs.lock";
/// Where a file that cannot be read is moved, so saving never replaces it.
const UNREADABLE: &str = "fan-outs.json.unreadable";
/// Fan-outs kept. None is ever dropped to make room: a launch beyond this is
/// not journaled, and says so.
const MAX_RECORDS: usize = 4;
/// Fan-outs that could not be journaled, followed in memory only so their
/// windows can still offer retries; the oldest is let go first.
const MAX_UNSAVED: usize = 16;
/// Bytes in one identifier, label, branch, or path. A fan-out with a longer
/// one is not journaled, and a longer checkout or commit is not recorded.
const MAX_FIELD_BYTES: usize = 1024;
/// Bytes of a prompt kept for resuming; a longer prompt is not kept.
pub(super) const MAX_PROMPT_BYTES: usize = 32 * 1024;
/// Text fields of one record: the origin's seven and the commit, then each
/// lane's three and its checkout's five.
const MAX_FIELDS: usize = 8 + MAX_LANES * 8;
/// JSON escapes a byte to at most six, plus names and punctuation.
const MAX_FILE_BYTES: u64 =
    (MAX_RECORDS * (6 * (MAX_PROMPT_BYTES + MAX_FIELDS * MAX_FIELD_BYTES) + 4096) + 64) as u64;

/// How far a lane got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Checkpoint {
    /// Nothing was done for the lane yet.
    Planned,
    /// The worktree was being created: its branch may exist.
    CreatingWorktree,
    /// The worktree exists, without an agent.
    Created,
    /// The agent was being started: one may be running in the pane.
    StartingAgent,
    /// The agent started; the prompt may or may not have reached it.
    Prompting,
    /// The agent took the prompt.
    Prompted,
}

impl Checkpoint {
    /// Whether the lane has a worktree, known by its checkout.
    fn has_checkout(self) -> bool {
        self >= Self::Created
    }

    /// What finishing an interrupted lane takes, if it can be finished.
    pub(crate) fn resume(self) -> Option<Resume> {
        match self {
            Self::Created => Some(Resume::Start),
            Self::StartingAgent | Self::Prompting => Some(Resume::Prompt),
            Self::Planned | Self::CreatingWorktree | Self::Prompted => None,
        }
    }
}

/// How an interrupted lane is finished, only ever at the user's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resume {
    /// Start the agent, then send the prompt.
    Start,
    /// Send the prompt only: an agent may already run in the pane, and
    /// starting it again would type into that agent.
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedOrigin {
    pub(crate) endpoint_id: String,
    /// The daemon behind the endpoint; see [`crate::teleport::Host::identity`].
    pub(crate) host: String,
    pub(crate) endpoint_label: String,
    pub(crate) workspace_id: String,
    pub(crate) repo_key: String,
    pub(crate) repo_label: String,
    pub(crate) base: String,
}

impl From<&Origin> for SavedOrigin {
    fn from(origin: &Origin) -> Self {
        Self {
            endpoint_id: origin.endpoint_id.clone(),
            host: origin.host.identity(),
            endpoint_label: origin.endpoint_label.clone(),
            workspace_id: origin.workspace_id.clone(),
            repo_key: origin.repo_key.clone(),
            repo_label: origin.repo_label.clone(),
            base: origin.base.clone(),
        }
    }
}

/// A lane's checkout without its host, which is found again from the
/// endpoint when the fan-out is restored, and only when it is the same
/// daemon: workspace and pane IDs repeat from one daemon to the next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedCheckout {
    pub(crate) endpoint_id: String,
    /// The daemon behind the endpoint; see [`crate::teleport::Host::identity`].
    pub(crate) host: String,
    pub(crate) workspace_id: String,
    pub(crate) path: String,
    pub(crate) pane_id: String,
}

impl SavedCheckout {
    fn fits(&self) -> bool {
        [
            &self.endpoint_id,
            &self.host,
            &self.workspace_id,
            &self.path,
            &self.pane_id,
        ]
        .into_iter()
        .all(|field| fits(field))
    }
}

impl From<&Checkout> for SavedCheckout {
    fn from(checkout: &Checkout) -> Self {
        Self {
            endpoint_id: checkout.endpoint_id.clone(),
            host: checkout.host.identity(),
            workspace_id: checkout.workspace_id.clone(),
            path: checkout.path.clone(),
            pane_id: checkout.pane_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedLane {
    /// The agent kind's name; see [`AgentKind::parse`].
    pub(crate) kind: String,
    pub(crate) branch: String,
    pub(crate) agent: String,
    pub(crate) step: Checkpoint,
    /// Present exactly when the step has a worktree.
    pub(crate) checkout: Option<SavedCheckout>,
}

impl SavedLane {
    pub(crate) fn lane(&self) -> Option<Lane> {
        Some(Lane {
            kind: AgentKind::parse(&self.kind)?,
            branch: self.branch.clone(),
            agent: self.agent.clone(),
        })
    }
}

/// One fan-out as journaled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Record {
    pub(crate) id: u64,
    pub(crate) origin: SavedOrigin,
    /// The commit every lane branches from, once resolved.
    pub(crate) commit: Option<String>,
    /// `None` when the prompt was too long to keep; it cannot be resent.
    pub(crate) prompt: Option<String>,
    pub(crate) lanes: Vec<SavedLane>,
}

impl Record {
    pub(crate) fn new(id: u64, origin: &Origin, prompt: &str, lanes: &[Lane]) -> Self {
        Self {
            id,
            origin: origin.into(),
            commit: None,
            prompt: (prompt.len() <= MAX_PROMPT_BYTES).then(|| prompt.to_owned()),
            lanes: lanes
                .iter()
                .map(|lane| SavedLane {
                    kind: lane.kind.name().to_owned(),
                    branch: lane.branch.clone(),
                    agent: lane.agent.clone(),
                    step: Checkpoint::Planned,
                    checkout: None,
                })
                .collect(),
        }
    }

    /// Record what host work reported. Returns whether anything changed.
    pub(crate) fn advance(&mut self, report: &Report) -> bool {
        let (index, progress) = match report {
            Report::Base(commit) => {
                if !fits(commit) {
                    return false;
                }
                let changed = self.commit.as_ref() != Some(commit);
                self.commit = Some(commit.clone());
                return changed;
            }
            Report::Lane(index, progress) => (*index, progress),
        };
        let Some(lane) = self.lanes.get_mut(index) else {
            return false;
        };
        let step = match progress {
            Progress::CreatingWorktree => Checkpoint::CreatingWorktree,
            Progress::Created(checkout) => {
                // Too long to keep: the lane stays at creating its worktree,
                // which says as much as can be kept, that one may exist.
                let saved = SavedCheckout::from(checkout);
                if !saved.fits() || lane.checkout.is_some() {
                    return false;
                }
                lane.checkout = Some(saved);
                Checkpoint::Created
            }
            Progress::StartingAgent => Checkpoint::StartingAgent,
            Progress::Prompting => Checkpoint::Prompting,
            Progress::Running => Checkpoint::Prompted,
            // Setting up a host changes nothing on it a lane owns, and a
            // failure leaves the lane where it stopped.
            Progress::SettingUp | Progress::Failed(_) => return false,
        };
        // A lane with a worktree never goes back to not knowing it, and one
        // without never claims an agent.
        if step.has_checkout() != lane.checkout.is_some() {
            return false;
        }
        let changed = lane.step != step;
        lane.step = step;
        changed
    }

    /// Whether anything is left to come back to: a worktree, or one that
    /// may exist.
    pub(crate) fn worth_keeping(&self) -> bool {
        self.lanes
            .iter()
            .any(|lane| lane.step >= Checkpoint::CreatingWorktree)
    }

    fn valid(&self) -> bool {
        let short = |field: &String| fits(field);
        let origin = &self.origin;
        [
            &origin.endpoint_id,
            &origin.host,
            &origin.endpoint_label,
            &origin.workspace_id,
            &origin.repo_key,
            &origin.repo_label,
            &origin.base,
        ]
        .into_iter()
        .all(short)
            && self.commit.as_ref().is_none_or(short)
            && self
                .prompt
                .as_ref()
                .is_none_or(|prompt| prompt.len() <= MAX_PROMPT_BYTES)
            && !self.lanes.is_empty()
            && self.lanes.len() <= MAX_LANES
            && self.lanes.iter().all(|lane| {
                AgentKind::parse(&lane.kind).is_some()
                    && short(&lane.branch)
                    && short(&lane.agent)
                    && lane.step.has_checkout() == lane.checkout.is_some()
                    && lane.checkout.as_ref().is_none_or(SavedCheckout::fits)
            })
    }
}

fn fits(field: &str) -> bool {
    field.len() <= MAX_FIELD_BYTES
}

#[derive(Serialize)]
struct Saved {
    records: Vec<Record>,
}

/// The file as read, each record still undecoded so that one damaged record
/// is dropped alone.
#[derive(Deserialize)]
struct Loaded {
    records: Vec<serde_json::Value>,
}

/// The saved records. One that is damaged, repeats an ID, or is beyond the
/// newest [`MAX_RECORDS`] is dropped on its own, so it never costs the others.
fn parse(bytes: &[u8]) -> Result<Vec<Record>> {
    let loaded: Loaded = serde_json::from_slice(bytes).map_err(Error::JournalJson)?;
    let mut records: Vec<Record> = Vec::new();
    for value in loaded.records {
        let record = match serde_json::from_value::<Record>(value) {
            Ok(record) => record,
            Err(error) => {
                tracing::warn!(%error, "Dropping an unreadable saved fan-out");
                continue;
            }
        };
        if !record.valid() || records.iter().any(|known| known.id == record.id) {
            tracing::warn!(
                lanes = record.lanes.len(),
                "Dropping a damaged saved fan-out"
            );
            continue;
        }
        records.push(record);
    }
    let excess = records.len().saturating_sub(MAX_RECORDS);
    if excess > 0 {
        tracing::warn!(excess, "Too many saved fan-outs; dropping the oldest");
        records.drain(..excess);
    }
    Ok(records)
}

/// Drop the lanes at `indices` of fan-out `id` among `records`, then the
/// fan-out itself once nothing is left to come back to. Whether it was there.
fn settle_in(records: &mut Vec<Record>, id: u64, indices: &[usize]) -> bool {
    let Some(at) = records.iter().position(|record| record.id == id) else {
        return false;
    };
    let record = &mut records[at];
    let mut index = 0;
    record.lanes.retain(|_| {
        let keep = !indices.contains(&index);
        index += 1;
        keep
    });
    if !record.worth_keeping() {
        records.remove(at);
    }
    true
}

/// The records saved at `path`, none when there is no file yet.
fn read(path: &Path) -> Result<Vec<Record>> {
    let bytes = state_file::read(path, MAX_FILE_BYTES)
        .map_err(|error| Error::JournalRead(Box::new(error)))?;
    bytes.as_deref().map_or(Ok(Vec::new()), parse)
}

#[derive(Default)]
struct State {
    records: Vec<Record>,
    /// Records a window of this process shows, which no other may claim.
    claimed: Vec<u64>,
    /// Fan-outs launched without being journaled. Never written: they only
    /// keep each lane's step, so a failed lane is retried from where it
    /// stopped while its window is open.
    unsaved: Vec<Record>,
    /// Bumped by every change, so an older snapshot never overwrites a newer
    /// one already on disk.
    revision: u64,
    /// The latest revision taken back by a failed save. A snapshot at or
    /// below it may hold the step taken back, so it is retaken before
    /// writing.
    rewound: u64,
}

#[derive(Default)]
struct Shared {
    path: Option<PathBuf>,
    /// Keeps other app processes from saving over this one's fan-outs.
    _lock: Option<fs::File>,
    state: Mutex<State>,
    /// The revision on disk, held while writing so writes do not interleave.
    written: Mutex<u64>,
}

/// The app's journal, shared by every window and their workers.
#[derive(Clone, Default)]
pub(crate) struct Journal(Arc<Shared>);

impl Global for Journal {}

/// A change made in memory, still to be written.
#[must_use = "a pending journal change is only saved by `write`"]
pub(crate) struct Pending {
    journal: Journal,
    revision: u64,
    records: Vec<Record>,
}

impl Pending {
    /// Save the change unless a newer one, which includes it, is already
    /// saved. Blocks on the disk, so callers run it off the UI thread.
    pub(crate) fn write(self) -> Result<()> {
        let journal = self.journal.clone();
        let mut written = journal.written();
        self.write_locked(&mut written)
    }

    fn write_locked(mut self, written: &mut u64) -> Result<()> {
        let Some(path) = &self.journal.0.path else {
            return Ok(());
        };
        if let Ok(mut state) = self.journal.0.state.lock()
            && self.revision <= state.rewound
        {
            state.revision += 1;
            self.revision = state.revision;
            self.records = state.records.clone();
        }
        if *written >= self.revision {
            return Ok(());
        }
        let saved = Saved {
            records: self.records,
        };
        state_file::write(path, &saved).map_err(|error| {
            tracing::warn!(%error, "Cannot save the fan-out journal");
            Error::JournalWrite(Box::new(error))
        })?;
        *written = self.revision;
        Ok(())
    }
}

impl Journal {
    /// Called before starting GPUI. A missing file starts empty.
    pub(crate) fn load() -> Self {
        crate::preferences::state_dir().map_or_else(Self::default, |dir| Self::locked(&dir))
    }

    /// The journal in `dir`, saved only by the first app process to open
    /// it. Another one, which would save over the first's fan-outs and
    /// could restore the same one, journals in memory only.
    pub(super) fn locked(dir: &Path) -> Self {
        let lock = fs::create_dir_all(dir)
            .and_then(|()| {
                fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(dir.join(LOCK))
            })
            .map_err(fs::TryLockError::Error)
            .and_then(|file| file.try_lock().map(|()| file));
        match lock {
            Ok(file) => Self::open(Some(dir.join(FILE)), Some(file)),
            Err(error) => {
                tracing::warn!(%error, "Fan-outs are saved by another app process, not this one");
                Self::default()
            }
        }
    }

    #[cfg(test)]
    pub(super) fn at(path: Option<PathBuf>) -> Self {
        Self::open(path, None)
    }

    /// A file that cannot be read is moved aside rather than saved over;
    /// when even that fails, nothing is saved.
    fn open(mut path: Option<PathBuf>, lock: Option<fs::File>) -> Self {
        let records = match path.as_deref().map(read).transpose() {
            Ok(records) => records.unwrap_or_default(),
            Err(error) => {
                tracing::warn!(%error, "Cannot restore the fan-out journal; setting it aside");
                let aside = path
                    .as_deref()
                    .is_some_and(|path| fs::rename(path, path.with_file_name(UNREADABLE)).is_ok());
                if !aside {
                    path = None;
                }
                Vec::new()
            }
        };
        Self(Arc::new(Shared {
            path,
            _lock: lock,
            state: Mutex::new(State {
                records,
                ..State::default()
            }),
            written: Mutex::new(0),
        }))
    }

    /// The revision on disk, held while writing. A writer that panicked
    /// left the file as whole as any other write.
    fn written(&self) -> MutexGuard<'_, u64> {
        self.0
            .written
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
    }

    /// The app's journal, or an unsaved one for fixtures that never
    /// installed it.
    pub(crate) fn get(cx: &App) -> Self {
        cx.try_global::<Self>().cloned().unwrap_or_default()
    }

    /// Apply `change` in memory. Returns the write to run when it changed
    /// anything and the journal is saved.
    fn update(&self, change: impl FnOnce(&mut State) -> bool) -> Option<Pending> {
        let mut state = self.0.state.lock().ok()?;
        if !change(&mut state) || self.0.path.is_none() {
            return None;
        }
        state.revision += 1;
        Some(Pending {
            journal: self.clone(),
            revision: state.revision,
            records: state.records.clone(),
        })
    }

    /// Start journaling a launched fan-out, claimed by its window. One with
    /// a field too long to keep, or beyond [`MAX_RECORDS`], is not
    /// journaled: no saved fan-out is dropped for it, and it is followed in
    /// memory instead.
    pub(crate) fn begin(&self, record: Record) -> Result<Option<Pending>> {
        if !record.valid() {
            self.follow(record);
            return Err(Error::JournalFieldTooLong);
        }
        let mut refused = None;
        let pending = self.update(|state| {
            state.records.retain(|known| known.id != record.id);
            if state.records.len() >= MAX_RECORDS {
                refused = Some(record);
                return false;
            }
            state.claimed.push(record.id);
            state.records.push(record);
            true
        });
        if let Some(record) = refused {
            self.follow(record);
            return Err(Error::JournalFull);
        }
        Ok(pending)
    }

    /// Follow a fan-out that cannot be journaled in memory only.
    fn follow(&self, record: Record) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        state.unsaved.retain(|known| known.id != record.id);
        if state.unsaved.len() >= MAX_UNSAVED {
            state.unsaved.remove(0);
        }
        state.unsaved.push(record);
    }

    pub(crate) fn advance(&self, id: u64, report: &Report) -> Option<Pending> {
        if let Ok(mut state) = self.0.state.lock()
            && let Some(record) = state.unsaved.iter_mut().find(|record| record.id == id)
        {
            record.advance(report);
            return None;
        }
        self.update(|state| {
            state
                .records
                .iter_mut()
                .find(|record| record.id == id)
                .is_some_and(|record| record.advance(report))
        })
    }

    /// Record `report` and save it. On a failed save the lane's step goes
    /// back to the last one saved, so nothing offers to finish a step whose
    /// script never ran. Blocks on the disk, so callers run it off the UI
    /// thread.
    pub(crate) fn record(&self, id: u64, report: &Report) -> Result<()> {
        // Held from the change to the rewind, so no other write can save
        // the step between a failed save and its rewind.
        let mut written = self.written();
        let before = match report {
            Report::Lane(index, _) => self.step(id, *index).map(|step| (*index, step)),
            Report::Base(_) => None,
        };
        let Some(pending) = self.advance(id, report) else {
            return Ok(());
        };
        let saved = pending.write_locked(&mut written);
        if saved.is_err()
            && let Some((index, step)) = before
        {
            self.rewind(id, index, step);
        }
        saved
    }

    /// Put lane `index` back at `step`, in memory, unless that would break
    /// its checkout invariant. Every snapshot taken until now may hold the
    /// step, so each is retaken before it is written.
    fn rewind(&self, id: u64, index: usize, step: Checkpoint) {
        let Ok(mut state) = self.0.state.lock() else {
            return;
        };
        state.rewound = state.revision;
        let lane = state
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .and_then(|record| record.lanes.get_mut(index));
        if let Some(lane) = lane
            && step.has_checkout() == lane.checkout.is_some()
        {
            lane.step = step;
        }
    }

    /// The step lane `index` of fan-out `id` last reached.
    pub(crate) fn step(&self, id: u64, index: usize) -> Option<Checkpoint> {
        let state = self.0.state.lock().ok()?;
        let record = state
            .records
            .iter()
            .chain(&state.unsaved)
            .find(|record| record.id == id)?;
        Some(record.lanes.get(index)?.step)
    }

    /// Drop the lanes at `indices`, which nothing needs to come back to, then
    /// the whole fan-out once no lane is left that has or may have a
    /// worktree.
    pub(crate) fn settle(&self, id: u64, indices: &[usize]) -> Option<Pending> {
        self.update(|state| {
            settle_in(&mut state.unsaved, id, indices);
            settle_in(&mut state.records, id, indices)
        })
    }

    /// Stop journaling a fan-out that is finished or that the user let go.
    pub(crate) fn forget(&self, id: u64) -> Option<Pending> {
        self.update(|state| {
            state.unsaved.retain(|record| record.id != id);
            let before = state.records.len();
            state.records.retain(|record| record.id != id);
            state.records.len() != before
        })
    }

    /// The oldest fan-out no window shows yet that `usable` accepts, now
    /// claimed by the caller. Touches no disk, so the UI thread may call it.
    pub(crate) fn claim(&self, usable: impl Fn(&Record) -> bool) -> Option<Record> {
        let mut state = self.0.state.lock().ok()?;
        let record = state
            .records
            .iter()
            .find(|record| !state.claimed.contains(&record.id) && usable(record))?
            .clone();
        state.claimed.push(record.id);
        Some(record)
    }

    #[cfg(test)]
    pub(super) fn records(&self) -> Vec<Record> {
        self.0
            .state
            .lock()
            .map(|state| state.records.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
