//! Tags on devices, such as `personal` and `work`, which group the sidebar.
//!
//! Each device's own daemon holds its tags, for every phone and desktop that
//! reaches it (`sheprd_remote_control::control::tags`). [`Tags`] is this app's
//! copy, kept by endpoint ID so a device that is away still shows them and a
//! device with no daemon can be tagged at all. [`DeviceTags`] saves the copy
//! and meets each reachable daemon now and then: it always offers what it has
//! and takes the answer, so one exchange settles either direction, and an edit
//! made while a device was away goes once it is back.

mod error;
mod sync;

pub use error::Error;
pub(crate) use sync::Daemon;

use gpui::{App, BorrowAppContext, Global};
use serde::{Deserialize, Serialize};
use sheprd_remote_control::control::tags::{ComputerTags, same};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError, atomic::AtomicBool},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FILE: &str = "device-tags.json";
/// How often a reachable machine is asked over SSH. A phone's edit reaches
/// this app within it; this app's own edits go at once.
const REMOTE_INTERVAL: Duration = Duration::from_secs(120);
/// This computer's daemon is a socket away.
const LOCAL_INTERVAL: Duration = Duration::from_secs(15);
/// Offered while no device has a tag yet, as the phone offers them.
pub(crate) const STARTERS: [&str; 2] = ["personal", "work"];

/// This app's copy of every device's tags, by endpoint ID.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Tags {
    records: BTreeMap<String, ComputerTags>,
}

impl Tags {
    pub(crate) fn of(&self, id: &str) -> &[String] {
        self.records
            .get(id)
            .map_or(&[], |record| record.tags.as_slice())
    }

    /// Whether any device has a tag: until one does, the sidebar is as it
    /// always was.
    pub(crate) fn any(&self) -> bool {
        self.records.values().any(|record| !record.tags.is_empty())
    }

    /// Every tag in use, alphabetically, each once.
    pub(crate) fn all(&self) -> Vec<&str> {
        let mut tags: Vec<&str> = Vec::new();
        for tag in self.records.values().flat_map(|record| &record.tags) {
            if !tags.iter().any(|known| same(known, tag)) {
                tags.push(tag);
            }
        }
        tags.sort_by_cached_key(|tag| tag.to_lowercase());
        tags
    }

    /// Whether the device carries `tag`.
    pub(crate) fn has(&self, id: &str, tag: &str) -> bool {
        self.of(id).iter().any(|own| same(own, tag))
    }

    /// The section the device lists under: `filter` while it has that tag,
    /// else its first tag, so a device is listed once. `None` is Untagged.
    pub(crate) fn section<'a>(&'a self, id: &str, filter: Option<&'a str>) -> Option<&'a str> {
        filter
            .filter(|tag| self.has(id, tag))
            .or_else(|| self.of(id).first().map(String::as_str))
    }

    /// Where the device's section sorts: alphabetically, Untagged last.
    pub(crate) fn rank(&self, id: &str, filter: Option<&str>, all: &[&str]) -> usize {
        self.section(id, filter)
            .and_then(|section| all.iter().position(|tag| same(tag, section)))
            .unwrap_or(all.len())
    }

    /// The device's tags once moved to `tag`, which becomes its section; the
    /// section it left is dropped and any others stay. `None` clears them.
    pub(crate) fn moved_to(&self, id: &str, tag: Option<&str>) -> Vec<String> {
        let Some(tag) = tag else {
            return Vec::new();
        };
        let tag = self.known_spelling(tag);
        let others = self.of(id).iter().skip(1).filter(|own| !same(own, &tag));
        std::iter::once(tag.clone())
            .chain(others.cloned())
            .collect()
    }

    /// The device's tags with `tag` added or taken away.
    pub(crate) fn toggled(&self, id: &str, tag: &str) -> Vec<String> {
        if self.has(id, tag) {
            return self
                .of(id)
                .iter()
                .filter(|own| !same(own, tag))
                .cloned()
                .collect();
        }
        let mut tags = self.of(id).to_vec();
        tags.push(self.known_spelling(tag));
        tags
    }

    /// `tag` as a device already spells it, as two spellings are one tag.
    fn known_spelling(&self, tag: &str) -> String {
        self.all()
            .into_iter()
            .find(|known| same(known, tag))
            .unwrap_or(tag)
            .to_owned()
    }

    /// `records` as edited long ago, for tests of what draws them.
    #[cfg(test)]
    pub(crate) fn tagged(records: &[(&str, &[&str])]) -> Self {
        let mut tags = Self::default();
        for (id, own) in records {
            tags.edit(id, own.iter().map(|tag| (*tag).to_owned()).collect(), 10);
        }
        tags
    }

    /// Records an edit made here at `now_ms`. Returns whether it changed
    /// anything.
    fn edit(&mut self, id: &str, tags: Vec<String>, now_ms: u64) -> bool {
        let edited = ComputerTags::new(tags, now_ms);
        if self.of(id) == edited.tags {
            return false;
        }
        self.records.insert(id.to_owned(), edited);
        true
    }

    /// Takes what the daemon says stands, unless the copy offered
    /// (`offered_ms`) was edited meanwhile: that edit is newer, and goes next.
    /// Returns whether the copy changed.
    fn settle(&mut self, id: &str, offered_ms: u64, stood: ComputerTags) -> bool {
        let current = self.records.get(id);
        if current.map_or(0, |record| record.updated_ms) != offered_ms
            || current == Some(&stood)
            || (current.is_none() && stood == ComputerTags::default())
        {
            return false;
        }
        self.records.insert(id.to_owned(), stood);
        true
    }

    fn offer(&self, id: &str) -> ComputerTags {
        self.records.get(id).cloned().unwrap_or_default()
    }
}

/// Where a device's tags stand with its daemon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Standing {
    #[default]
    Unknown,
    /// Its daemon has the same tags.
    Synced,
    /// An edit has not reached its daemon, or the last try failed.
    Waiting,
    /// It has no daemon that keeps tags, so no one else reads them.
    Alone,
}

#[derive(Debug, Default)]
struct Link {
    last: Option<Instant>,
    /// An edit asks for an exchange without waiting for the interval.
    due: bool,
    running: bool,
    standing: Standing,
}

/// What the status bar says about sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Synced,
    /// This many devices have tags their daemon has not heard of.
    Waiting(usize),
}

/// This app's tags and each device's link to its daemon.
#[derive(Default)]
pub(crate) struct DeviceTags {
    tags: Arc<Tags>,
    file: Option<PathBuf>,
    /// Whether the saved copy was read: until then an exchange would offer
    /// nothing and could lose an edit made while the app was closed.
    loaded: bool,
    /// The devices windows can reach now, by endpoint ID.
    daemons: BTreeMap<String, Daemon>,
    links: HashMap<String, Link>,
    /// The newest copy to write, shared with the writer so a slow write
    /// never puts an older copy back.
    saving: Arc<Mutex<Option<Arc<Tags>>>>,
}

impl Global for DeviceTags {}

impl DeviceTags {
    /// Reads the saved copy off the UI thread.
    pub(crate) fn install(cx: &mut App) {
        let file = crate::preferences::state_dir().map(|dir| dir.join(FILE));
        cx.set_global(Self {
            file: file.clone(),
            ..Self::default()
        });
        let read = cx
            .background_executor()
            .spawn(async move { file.as_deref().map(read).unwrap_or_default() });
        cx.spawn(async move |cx| {
            let tags = read.await;
            cx.update(|cx| {
                cx.update_global::<Self, _>(|this, _| {
                    this.tags = Arc::new(tags);
                    this.loaded = true;
                });
            });
        })
        .detach();
    }

    /// The copy to draw from; empty before [`Self::install`], as in tests.
    pub(crate) fn current(cx: &App) -> Arc<Tags> {
        cx.try_global::<Self>()
            .map(|this| this.tags.clone())
            .unwrap_or_default()
    }

    /// Replaces a device's tags and sends them on at once.
    pub(crate) fn set(cx: &mut App, id: &str, tags: Vec<String>) {
        if !cx.has_global::<Self>() {
            return;
        }
        let changed = cx.update_global::<Self, _>(|this, _| {
            let mut copy = (*this.tags).clone();
            if !copy.edit(id, tags, now_ms()) {
                return false;
            }
            this.tags = Arc::new(copy);
            let link = this.links.entry(id.to_owned()).or_default();
            link.due = true;
            link.standing = Standing::Waiting;
            true
        });
        if changed {
            Self::save(cx);
            Self::tick(cx);
        }
    }

    /// Tells which devices a window can reach. Windows report every poll, so
    /// this only touches the global when the set changed.
    pub(crate) fn reach(cx: &mut App, reachable: BTreeMap<String, Daemon>) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        if reachable
            .iter()
            .any(|(id, daemon)| this.daemons.get(id) != Some(daemon))
            || this.daemons.keys().any(|id| !reachable.contains_key(id))
        {
            cx.global_mut::<Self>().daemons = reachable;
        }
        Self::tick(cx);
    }

    /// Starts the exchanges that are due.
    fn tick(cx: &mut App) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        if !this.loaded {
            return;
        }
        let due: Vec<(String, Daemon, ComputerTags)> = this
            .daemons
            .iter()
            .filter(|(id, daemon)| {
                let link = this.links.get(*id);
                let interval = match daemon {
                    Daemon::Local => LOCAL_INTERVAL,
                    Daemon::Remote(_) => REMOTE_INTERVAL,
                };
                !link.is_some_and(|link| link.running)
                    && link.is_none_or(|link| {
                        link.due || link.last.is_none_or(|last| last.elapsed() >= interval)
                    })
            })
            .map(|(id, daemon)| (id.clone(), daemon.clone(), this.tags.offer(id)))
            .collect();
        for (id, daemon, offer) in due {
            let link = cx.global_mut::<Self>().links.entry(id.clone()).or_default();
            link.running = true;
            link.due = false;
            link.last = Some(Instant::now());
            let offered_ms = offer.updated_ms;
            let task = cx
                .background_executor()
                .spawn(async move { sync::exchange(&daemon, offer, &AtomicBool::new(false)) });
            cx.spawn(async move |cx| {
                let answer = task.await;
                cx.update(|cx| Self::answered(cx, &id, offered_ms, answer));
            })
            .detach();
        }
    }

    fn answered(
        cx: &mut App,
        id: &str,
        offered_ms: u64,
        answer: Result<Option<ComputerTags>, Error>,
    ) {
        if let Err(error) = &answer {
            tracing::debug!(id, %error, "Could not exchange a device's tags");
        }
        let changed = cx.update_global::<Self, _>(|this, _| {
            let link = this.links.entry(id.to_owned()).or_default();
            link.running = false;
            let (standing, stood) = match answer {
                Ok(Some(stood)) => (Standing::Synced, Some(stood)),
                Ok(None) => (Standing::Alone, None),
                Err(_) => (Standing::Waiting, None),
            };
            // An edit made during the exchange is still to go.
            if !link.due {
                link.standing = standing;
            }
            let Some(stood) = stood else {
                return false;
            };
            let mut copy = (*this.tags).clone();
            let changed = copy.settle(id, offered_ms, stood);
            if changed {
                this.tags = Arc::new(copy);
            }
            changed
        });
        if changed {
            Self::save(cx);
        }
        cx.refresh_windows();
    }

    /// How sync stands, or `None` while nothing is tagged.
    pub(crate) fn status(cx: &App) -> Option<Status> {
        let this = cx.try_global::<Self>()?;
        if !this.tags.any() {
            return None;
        }
        let waiting = this
            .links
            .values()
            .filter(|link| link.standing == Standing::Waiting)
            .count();
        Some(if waiting == 0 {
            Status::Synced
        } else {
            Status::Waiting(waiting)
        })
    }

    fn save(cx: &mut App) {
        let this = cx.global::<Self>();
        let Some(file) = this.file.clone() else {
            return;
        };
        let saving = this.saving.clone();
        *saving.lock().unwrap_or_else(PoisonError::into_inner) = Some(this.tags.clone());
        cx.background_executor()
            .spawn(async move {
                // Held across the write, so writes land one at a time and the
                // last one holds the newest copy.
                let mut newest = saving.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(tags) = newest.take()
                    && let Err(error) = write(&file, &tags)
                {
                    tracing::warn!(%error, "Could not save device tags");
                }
            })
            .detach();
    }
}

/// The copy saved in `file`; none when missing or damaged, which costs only
/// labels each daemon still holds. Blocking.
fn read(file: &Path) -> Tags {
    std::fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Tags>(&bytes).ok())
        .map(|saved| Tags {
            records: saved
                .records
                .into_iter()
                .map(|(id, record)| (id, ComputerTags::new(record.tags, record.updated_ms)))
                .collect(),
        })
        .unwrap_or_default()
}

/// Replaces `file` whole, so a crash leaves one copy or the other. Blocking.
fn write(file: &Path, tags: &Tags) -> Result<(), Error> {
    let failed = |source| Error::Save {
        path: file.to_owned(),
        source,
    };
    let bytes = serde_json::to_vec_pretty(tags).map_err(|error| failed(error.into()))?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    let partial = file.with_extension("json.partial");
    std::fs::write(&partial, bytes).map_err(failed)?;
    std::fs::rename(&partial, file).map_err(failed)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// The tags in `tags` that are not `used`, for a picker to offer.
pub(crate) fn offered(tags: &Tags) -> Vec<String> {
    let all = tags.all();
    let known: BTreeSet<&str> = if all.is_empty() {
        STARTERS.into_iter().collect()
    } else {
        all.into_iter().collect()
    };
    let mut known: Vec<String> = known.into_iter().map(str::to_owned).collect();
    known.sort_by_cached_key(|tag| tag.to_lowercase());
    known
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
