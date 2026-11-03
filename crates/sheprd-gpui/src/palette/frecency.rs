//! How often and how recently each Go To destination and project was chosen,
//! so a typed query ranks familiar rows higher, as snacks.picker's frecency
//! does. Kept per user on this machine, never sent to the daemon.
//!
//! Workspaces are remembered by host, launch path, and branch, which survive
//! a daemon restart. Tab and pane IDs do not, so those are remembered by the
//! daemon boot they were chosen in: a reused ID after a restart never
//! inherits another pane's history, and the old entry simply ages out.

use crate::state_file;
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Destinations remembered; the least frecent go first beyond this.
const MAX_KEYS: usize = 1024;
/// Bytes in any one field of a key. A longer key is not remembered, so every
/// key the store accepts fits in the file it reads back.
const MAX_FIELD_BYTES: usize = 1024;
/// Room for the most keys the store keeps, each as large as it accepts: JSON
/// escapes at most double a string's bytes, plus field names and numbers.
const MAX_FILE_BYTES: u64 = (MAX_KEYS * (2 * 3 * MAX_FIELD_BYTES + 256) + 64) as u64;
const FILE: &str = "palette-frecency.json";
/// A choice counts half as much after this long.
const HALF_LIFE_SECS: f64 = 7. * 24. * 60. * 60.;
/// Turns a decayed visit count into ranking points. One recent visit is
/// worth about one matched character; the boost grows logarithmically.
const BOOST_SCALE: f64 = 16.;
/// A familiar row can outrank a slightly better match, never a far better one.
pub(super) const MAX_BOOST: u32 = 64;

/// A destination the palette remembers choosing.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Key {
    Workspace {
        endpoint: String,
        root: String,
        branch: Option<String>,
    },
    Tab {
        endpoint: String,
        boot: String,
        id: String,
    },
    Pane {
        endpoint: String,
        boot: String,
        id: String,
    },
    Project {
        path: PathBuf,
    },
}

impl Key {
    fn valid(&self) -> bool {
        let fits = |text: &str| !text.is_empty() && text.len() <= MAX_FIELD_BYTES;
        match self {
            Self::Workspace {
                endpoint,
                root,
                branch,
            } => fits(endpoint) && fits(root) && branch.as_deref().is_none_or(fits),
            Self::Tab { endpoint, boot, id } | Self::Pane { endpoint, boot, id } => {
                fits(endpoint) && fits(boot) && fits(id)
            }
            Self::Project { path } => path.to_str().is_some_and(fits),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Visit {
    key: Key,
    /// Decayed visit count as of `at`.
    score: f64,
    /// Unix seconds of the latest visit.
    at: u64,
}

impl Visit {
    fn valid(&self) -> bool {
        self.key.valid() && self.score.is_finite() && self.score >= 0.
    }

    /// The score decayed to `now`. A visit stamped in the future, from a
    /// clock that moved back, counts as made now.
    fn score_at(&self, now: u64) -> f64 {
        let elapsed = now.saturating_sub(self.at) as f64;
        self.score * 0.5_f64.powf(elapsed / HALF_LIFE_SECS)
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    visits: Vec<Visit>,
}

fn parse(bytes: &[u8]) -> crate::Result<Vec<Visit>> {
    let saved: Saved = serde_json::from_slice(bytes)?;
    if saved.visits.len() > MAX_KEYS || !saved.visits.iter().all(Visit::valid) {
        return Err(super::Error::InvalidFrecency.into());
    }
    Ok(saved.visits)
}

/// Unix seconds now; a clock before 1970 reads as the epoch.
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// Ranking points by key, taken when a palette opens.
#[derive(Clone, Debug, Default)]
pub(super) struct Boosts(std::collections::HashMap<Key, u32>);

impl Boosts {
    pub(super) fn of(&self, key: Option<&Key>) -> u32 {
        key.and_then(|key| self.0.get(key)).copied().unwrap_or(0)
    }
}

/// The app's palette history.
#[derive(Default)]
pub(crate) struct Frecency {
    visits: Vec<Visit>,
    writer: Option<state_file::Writer<Saved>>,
    quitting: bool,
}

impl Global for Frecency {}

impl Frecency {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Called before starting GPUI, like the worktree notes. A missing or
    /// damaged file starts with no history.
    pub(crate) fn load() -> Self {
        Self::at(Self::path())
    }

    fn at(path: Option<PathBuf>) -> Self {
        let visits = path
            .as_deref()
            .map(|path| {
                state_file::read(path, MAX_FILE_BYTES)
                    .and_then(|bytes| bytes.as_deref().map_or(Ok(Vec::new()), parse))
            })
            .transpose()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Cannot restore palette history");
                None
            })
            .unwrap_or_default();
        let writer = path.and_then(|path| {
            state_file::Writer::start("palette-frecency", path)
                .inspect_err(
                    |error| tracing::warn!(%error, "Cannot start the palette history worker"),
                )
                .ok()
        });
        Self {
            visits,
            writer,
            quitting: false,
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.on_app_quit(|cx| {
            let frecency = cx.global_mut::<Self>();
            frecency.quitting = true;
            let writer = frecency.writer.take();
            cx.background_executor().spawn(async move {
                if let Some(writer) = writer {
                    writer.finish();
                }
            })
        })
        .detach();
    }

    /// The app's history, or none for fixtures that never installed it.
    pub(crate) fn of(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }

    /// Runs `f` against the app's history, creating an unsaved one for
    /// fixtures that never installed it.
    pub(crate) fn update<R>(cx: &mut App, f: impl FnOnce(&mut Self) -> R) -> R {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        f(cx.global_mut::<Self>())
    }

    /// Ranking points for `key` at `now`, from zero to [`MAX_BOOST`].
    #[cfg(test)]
    fn boost(&self, key: &Key, now: u64) -> u32 {
        self.boosts(now).of(Some(key))
    }

    /// Ranking points at `now` for every remembered key, from zero to
    /// [`MAX_BOOST`]. History only changes when a row is chosen, which closes
    /// the palette, so one table serves an open palette's every rebuild.
    pub(super) fn boosts(&self, now: u64) -> Boosts {
        Boosts(
            self.visits
                .iter()
                .map(|visit| {
                    let points = (visit.score_at(now).ln_1p() * BOOST_SCALE).round();
                    // Bounded above and below, so the cast cannot truncate.
                    (
                        visit.key.clone(),
                        points.clamp(0., f64::from(MAX_BOOST)) as u32,
                    )
                })
                .collect(),
        )
    }

    /// Records choosing `key` at `now`. Beyond the limit, the least frecent
    /// key is forgotten.
    pub(super) fn visit(&mut self, key: Key, now: u64) {
        if !key.valid() {
            return;
        }
        match self.visits.iter_mut().find(|visit| visit.key == key) {
            Some(visit) => {
                visit.score = visit.score_at(now) + 1.;
                visit.at = visit.at.max(now);
            }
            None => self.visits.push(Visit {
                key,
                score: 1.,
                at: now,
            }),
        }
        if self.visits.len() > MAX_KEYS
            && let Some(weakest) = self
                .visits
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.score_at(now).total_cmp(&b.score_at(now)))
                .map(|(index, _)| index)
        {
            self.visits.swap_remove(weakest);
        }
        self.save();
    }

    fn save(&self) {
        if self.quitting {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.save(Saved {
                visits: self.visits.clone(),
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
