//! What the user did with Overview rows: pinned them, snoozed them until a
//! time, or settled them as finished. The marks belong to this client
//! alone, like worktree notes: never sent to the daemon, kept by a key that
//! outlives a daemon restart, and shared by every window.
//!
//! A hold lasts until the row needs a look again: a snooze until its time,
//! then the row comes back marked as woken; either hold as soon as its row
//! turns blocked, failed, or working from the state it was held in.

use super::{
    Error,
    model::{Key, State},
};
use crate::state_file;
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Marks kept; the least recently changed go first beyond this.
const MAX_MARKS: usize = 512;
/// Bytes in each part of a row's key. A longer key gets no mark, so every
/// mark the store accepts fits in the file it reads back.
const MAX_FIELD_BYTES: usize = 1024;
/// Room for the most marks the store keeps, each as large as it accepts:
/// JSON escapes at most double a string's bytes, plus names and punctuation.
const MAX_FILE_BYTES: u64 = (MAX_MARKS * (2 * 3 * MAX_FIELD_BYTES + 256) + 64) as u64;
const FILE: &str = "overview-marks.json";

/// How a row is held out of the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "hold", rename_all = "snake_case")]
pub(crate) enum Hold {
    Snoozed {
        #[serde(with = "seconds")]
        until: SystemTime,
    },
    Settled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Mark {
    pub(crate) key: Key,
    #[serde(default)]
    pub(crate) pinned: bool,
    #[serde(default)]
    held: Option<(Hold, State)>,
    /// A snooze ran out and the row has not been looked at since.
    #[serde(default)]
    pub(crate) woke: bool,
}

impl Mark {
    fn new(key: Key) -> Self {
        Self {
            key,
            pinned: false,
            held: None,
            woke: false,
        }
    }

    /// The hold still in force on a row now in `state`.
    pub(crate) fn hold(&self, state: State, now: SystemTime) -> Option<Hold> {
        let (hold, held) = self.held?;
        if state != held && (state.needs_you() || state == State::Working) {
            return None;
        }
        match hold {
            Hold::Snoozed { until } if until <= now => None,
            hold => Some(hold),
        }
    }

    fn valid(&self) -> bool {
        self.key
            .fields()
            .all(|field| field.len() <= MAX_FIELD_BYTES)
            && self
                .key
                .fields()
                .next()
                .is_some_and(|field| !field.is_empty())
    }

    /// Whether the mark still says anything.
    fn kept(&self) -> bool {
        self.pinned || self.held.is_some() || self.woke
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    marks: Vec<Mark>,
}

fn parse(bytes: &[u8]) -> Result<Vec<Mark>, Error> {
    let saved: Saved = serde_json::from_slice(bytes)?;
    if saved.marks.len() > MAX_MARKS || !saved.marks.iter().all(Mark::valid) {
        return Err(Error::InvalidMarks);
    }
    Ok(saved.marks)
}

/// The app's marks, least recently changed first.
#[derive(Default)]
pub(crate) struct Marks {
    marks: Vec<Mark>,
    writer: Option<state_file::Writer<Saved>>,
    quitting: bool,
    /// Counts changes, so a window can tell another one changed a mark.
    revision: u64,
}

impl Global for Marks {}

impl Marks {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Called before starting GPUI. A missing or damaged file starts empty.
    pub(crate) fn load() -> Self {
        Self::at(Self::path())
    }

    pub(super) fn at(path: Option<PathBuf>) -> Self {
        let marks = path
            .as_deref()
            .map(|path| {
                state_file::read(path, MAX_FILE_BYTES).and_then(|bytes| {
                    bytes
                        .as_deref()
                        .map_or(Ok(Vec::new()), |bytes| Ok(parse(bytes)?))
                })
            })
            .transpose()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Cannot restore overview marks");
                None
            })
            .unwrap_or_default();
        let writer =
            path.and_then(
                |path| match state_file::Writer::start("overview-marks", path) {
                    Ok(writer) => Some(writer),
                    Err(error) => {
                        tracing::warn!(%error, "Cannot start the overview marks worker");
                        None
                    }
                },
            );
        Self {
            marks,
            writer,
            quitting: false,
            revision: 0,
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.on_app_quit(|cx| {
            let marks = cx.global_mut::<Self>();
            marks.quitting = true;
            let writer = marks.writer.take();
            cx.background_executor().spawn(async move {
                if let Some(writer) = writer {
                    writer.finish();
                }
            })
        })
        .detach();
    }

    /// Runs `f` against the app's marks, creating an unsaved set for
    /// fixtures that never installed them.
    pub(crate) fn update<R>(cx: &mut App, f: impl FnOnce(&mut Self) -> R) -> R {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        f(cx.global_mut::<Self>())
    }

    /// The app's marks, or none for fixtures that never installed them.
    pub(crate) fn of(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn get(&self, key: &Key) -> Option<&Mark> {
        self.marks.iter().find(|mark| mark.key == *key)
    }

    /// Changes `key`'s mark with `f`, moving it to the most recent end, and
    /// saves. A key too long to keep is refused.
    fn change(&mut self, key: &Key, f: impl FnOnce(&mut Mark)) -> bool {
        let position = self.marks.iter().position(|mark| mark.key == *key);
        let mut mark = match position {
            Some(index) => self.marks.remove(index),
            None => Mark::new(key.clone()),
        };
        if !mark.valid() {
            return false;
        }
        let before = mark.clone();
        f(&mut mark);
        let changed = position.is_none() && mark.kept() || mark != before;
        if mark.kept() {
            self.marks.push(mark);
            if self.marks.len() > MAX_MARKS {
                let excess = self.marks.len() - MAX_MARKS;
                self.marks.drain(..excess);
            }
        }
        if changed {
            self.revision += 1;
            self.save();
        }
        changed
    }

    pub(crate) fn toggle_pin(&mut self, key: &Key) -> bool {
        self.change(key, |mark| {
            mark.pinned = !mark.pinned;
            mark.woke = false;
        })
    }

    /// Holds the row, now in `state`, out of the way until `until`.
    pub(crate) fn snooze(&mut self, key: &Key, state: State, until: SystemTime) -> bool {
        self.change(key, |mark| {
            mark.held = Some((Hold::Snoozed { until }, state));
            mark.woke = false;
        })
    }

    /// Holds the row, now in `state`, as finished.
    pub(crate) fn settle(&mut self, key: &Key, state: State) -> bool {
        self.change(key, |mark| {
            mark.held = Some((Hold::Settled, state));
            mark.pinned = false;
            mark.woke = false;
        })
    }

    /// Ends the row's hold now: wakes a snoozed row, brings back a settled one.
    pub(crate) fn release(&mut self, key: &Key) -> bool {
        self.change(key, |mark| mark.held = None)
    }

    /// The row was opened, so it is no longer news that it woke.
    pub(crate) fn seen(&mut self, key: &Key) -> bool {
        if !self.get(key).is_some_and(|mark| mark.woke) {
            return false;
        }
        self.change(key, |mark| mark.woke = false)
    }

    /// Ends the holds of rows that need a look again, given each shown
    /// row's state: snoozes whose time came, marked as woken, and holds on
    /// rows that turned blocked, failed, or working. A row not shown keeps
    /// its mark, since its host may only be away.
    pub(crate) fn wake<'a>(
        &mut self,
        rows: impl IntoIterator<Item = (&'a Key, State)>,
        now: SystemTime,
    ) -> bool {
        let mut changed = false;
        for (key, state) in rows {
            let Some(mark) = self.marks.iter_mut().find(|mark| mark.key == *key) else {
                continue;
            };
            let Some((hold, _)) = mark.held else {
                continue;
            };
            if mark.hold(state, now).is_none() {
                mark.held = None;
                mark.woke = matches!(hold, Hold::Snoozed { .. });
                changed = true;
            }
        }
        let before = self.marks.len();
        self.marks.retain(Mark::kept);
        if changed || self.marks.len() != before {
            self.revision += 1;
            self.save();
            return true;
        }
        false
    }

    /// Writes what is pending and stops the worker, for tests to read the
    /// file back.
    #[cfg(test)]
    pub(super) fn finish(&mut self) {
        if let Some(writer) = self.writer.take() {
            writer.finish();
        }
    }

    fn save(&self) {
        if self.quitting {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.save(Saved {
                marks: self.marks.clone(),
            });
        }
    }
}

/// Times as whole seconds since 1970: a snooze needs no finer.
mod seconds {
    use super::{Duration, SystemTime, UNIX_EPOCH};
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        time: &SystemTime,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let seconds = time
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        serializer.serialize_u64(seconds)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<SystemTime, D::Error> {
        let seconds = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_secs(seconds))
    }
}
