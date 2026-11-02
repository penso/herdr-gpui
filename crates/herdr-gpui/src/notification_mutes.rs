//! Workspaces and tabs whose notifications the user muted. A muted workspace
//! silences every tab in it, including tabs opened later; a tab can also be
//! muted on its own. While muted, nothing about it asks for the user: no
//! toast, system notification, or sound, and its agents leave the Dock badge
//! and the Overview's count. Status dots still show what its agents do.
//!
//! The mutes belong to this client alone, like worktree notes: never sent to
//! the daemon, and shared by every window. Each is keyed by the device, the
//! daemon on it, and the workspace or tab ID, so it follows a rename, and it
//! remembers the daemon process it was last seen in and the label it had
//! there. Herdr keeps IDs when it restores a session after a restart, but a
//! daemon started afresh numbers its workspaces from the start again, and
//! names the ones nobody named after their folder and tabs after their
//! position, so an old ID and label can come back on new work. A mute carries
//! over to a new process only for a workspace or tab the user named, back
//! under the same ID and name. A mute whose workspace or tab closed is
//! forgotten.

mod error;

pub(crate) use error::Error;

use crate::state_file;
use gpui::{App, Global};
use herdr_client::{ConnectTarget, protocol::ClientShellSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// Mutes kept; the least recently changed go first beyond this.
const MAX_MUTES: usize = 512;
/// Bytes in each saved field. A longer ID is not muted, and a longer label
/// is kept cut to this, so every mute the store accepts fits in its file.
const MAX_FIELD_BYTES: usize = 1024;
/// Room for the most mutes the store keeps, each as large as it accepts. No
/// field holds a control character, so JSON escapes at most double a
/// string's bytes; plus the field names, the time, and punctuation.
const MAX_FILE_BYTES: u64 = (MAX_MUTES * (2 * 5 * MAX_FIELD_BYTES + 160) + 64) as u64;
const FILE: &str = "notification-mutes.json";
/// How long a new mute ignores a snapshot that would forget it: one that does
/// not show its target, or shows another daemon process. Another window's
/// connection may still show a snapshot from before the tab opened, or from
/// before the daemon restarted, when the tab is muted here.
const FORGET_AFTER_SECONDS: u64 = 60;

/// What a mute silences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    /// A workspace and every tab in it.
    Workspace,
    /// One tab.
    Tab,
}

/// The Herdr daemon a mute belongs to: the device's endpoint, and the
/// session on it, since a device can be pointed at another of its host's
/// sessions.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct Daemon {
    pub(crate) endpoint: String,
    pub(crate) session: String,
}

impl Daemon {
    pub(crate) fn of(endpoint: &crate::endpoint::Endpoint) -> Self {
        Self::new(&endpoint.id, &endpoint.connection.target)
    }

    /// A daemon on this machine is its client socket, however `target` names
    /// it: the root session, `--session default`, and a socket override to the
    /// same place are one daemon, and two local sessions are two. A remote one
    /// is the session it attaches to on the endpoint's host.
    pub(crate) fn new(endpoint: &str, target: &ConnectTarget) -> Self {
        let session = match target.remote_session() {
            Some(session) => session.to_owned(),
            None => target
                .socket_path()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };
        Self {
            endpoint: endpoint.to_owned(),
            session,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Mute {
    #[serde(flatten)]
    daemon: Daemon,
    scope: Scope,
    id: String,
    /// The daemon process the target was last seen in, and its label then.
    boot: String,
    label: String,
    /// When it was muted, in seconds since 1970.
    #[serde(default)]
    muted_at: u64,
}

impl Mute {
    fn valid(&self) -> bool {
        !self.daemon.endpoint.is_empty()
            && !self.id.is_empty()
            && !self.boot.is_empty()
            && [
                &self.daemon.endpoint,
                &self.daemon.session,
                &self.id,
                &self.boot,
                &self.label,
            ]
            .iter()
            .all(|field| field.len() <= MAX_FIELD_BYTES && !field.chars().any(char::is_control))
    }

    fn is(&self, daemon: &Daemon, scope: Scope, id: &str) -> bool {
        self.daemon == *daemon && self.scope == scope && self.id == id
    }

    /// Whether this mute still stands for `target` in `snapshot`: in the same
    /// daemon process, or in a new one that restored it.
    fn holds(&self, snapshot: &ClientShellSnapshot, target: Target<'_>) -> bool {
        self.boot == snapshot.boot_id || (target.named && self.label == fit(target.label))
    }
}

/// `label` as it is saved and compared: without control characters, and cut
/// to [`MAX_FIELD_BYTES`] at a character boundary.
fn fit(label: &str) -> Cow<'_, str> {
    if label.len() <= MAX_FIELD_BYTES && !label.chars().any(char::is_control) {
        return Cow::Borrowed(label);
    }
    let mut fitted = String::with_capacity(label.len().min(MAX_FIELD_BYTES));
    for c in label.chars().filter(|c| !c.is_control()) {
        if fitted.len() + c.len_utf8() > MAX_FIELD_BYTES {
            break;
        }
        fitted.push(c);
    }
    Cow::Owned(fitted)
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// A workspace or tab as a snapshot shows it.
#[derive(Clone, Copy)]
struct Target<'a> {
    label: &'a str,
    /// The user named it, rather than Herdr after its folder or position.
    named: bool,
}

/// The workspace or tab `snapshot` shows with `id`, if it shows one.
fn target<'a>(snapshot: &'a ClientShellSnapshot, scope: Scope, id: &str) -> Option<Target<'a>> {
    match scope {
        Scope::Workspace => snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == id)
            .map(|workspace| Target {
                label: &workspace.label,
                named: workspace.custom_label,
            }),
        Scope::Tab => snapshot
            .tabs
            .iter()
            .find(|tab| tab.tab_id == id)
            .map(|tab| Target {
                label: &tab.label,
                named: tab.custom_label,
            }),
    }
}

/// What a snapshot of a mute's daemon means for the mute.
#[derive(Debug, PartialEq, Eq)]
enum Verdict<'a> {
    Keep,
    /// Same process, and the target was renamed.
    Relabel(Cow<'a, str>),
    /// A new process restored the target under the same ID and the name the
    /// user gave it.
    Adopt,
    /// The target closed, or a new process did not bring it back.
    Forget,
}

fn verdict<'a>(mute: &Mute, snapshot: &'a ClientShellSnapshot, now: SystemTime) -> Verdict<'a> {
    let same_process = mute.boot == snapshot.boot_id;
    let young = seconds(now) < mute.muted_at.saturating_add(FORGET_AFTER_SECONDS);
    let Some(target) = target(snapshot, mute.scope, &mute.id) else {
        return if young {
            Verdict::Keep
        } else {
            Verdict::Forget
        };
    };
    let label = fit(target.label);
    match (same_process, mute.label == label) {
        (true, true) => Verdict::Keep,
        (true, false) => Verdict::Relabel(label),
        (false, true) if target.named && snapshot.boot_id.len() <= MAX_FIELD_BYTES => {
            Verdict::Adopt
        }
        (false, _) if young => Verdict::Keep,
        (false, _) => Verdict::Forget,
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    mutes: Vec<Mute>,
}

fn parse(bytes: &[u8]) -> Result<Vec<Mute>, Error> {
    let saved: Saved = serde_json::from_slice(bytes)?;
    if saved.mutes.len() > MAX_MUTES || !saved.mutes.iter().all(Mute::valid) {
        return Err(Error::Invalid);
    }
    Ok(saved.mutes)
}

/// What one daemon's mutes silence in one of its snapshots: the workspaces
/// muted, and every tab and pane silent through its own mute or its
/// workspace's. Borrows only the snapshot, so a caller can keep it while it
/// changes other state.
#[derive(Debug, Default)]
pub(crate) struct Silenced<'a> {
    workspaces: HashSet<&'a str>,
    /// Tabs muted on their own.
    own_tabs: HashSet<&'a str>,
    tabs: HashSet<&'a str>,
    panes: HashSet<&'a str>,
}

impl Silenced<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.workspaces.is_empty() && self.tabs.is_empty()
    }

    /// Whether any workspace is muted, rather than only tabs.
    pub(crate) fn any_workspace(&self) -> bool {
        !self.workspaces.is_empty()
    }

    /// Whether the workspace is muted.
    pub(crate) fn workspace(&self, id: &str) -> bool {
        self.workspaces.contains(id)
    }

    /// Whether the tab is muted on its own, whatever its workspace is.
    pub(crate) fn own_tab(&self, id: &str) -> bool {
        self.own_tabs.contains(id)
    }

    /// Whether the tab is silent: muted itself or with its workspace.
    pub(crate) fn tab(&self, id: &str) -> bool {
        self.tabs.contains(id)
    }

    /// Whether the pane is silent through its tab or workspace.
    pub(crate) fn pane(&self, id: &str) -> bool {
        self.panes.contains(id)
    }

    /// Whether a notification naming these places is silenced. Any of them
    /// being silent is enough, so a pane the snapshot does not list yet
    /// still follows the tab or workspace the notification names.
    pub(crate) fn notice(
        &self,
        workspace: Option<&str>,
        tab: Option<&str>,
        pane: Option<&str>,
    ) -> bool {
        workspace.is_some_and(|id| self.workspace(id))
            || tab.is_some_and(|id| self.tab(id))
            || pane.is_some_and(|id| self.pane(id))
    }
}

/// The app's mutes, least recently changed first.
#[derive(Default)]
pub(crate) struct Mutes {
    mutes: Vec<Mute>,
    writer: Option<state_file::Writer<Saved>>,
    quitting: bool,
    /// Counts changes, so a window can tell another one changed a mute.
    revision: u64,
}

impl Global for Mutes {}

impl Mutes {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Called before starting GPUI. A missing or damaged file starts with
    /// nothing muted.
    pub(crate) fn load() -> Self {
        Self::at(Self::path())
    }

    fn at(path: Option<PathBuf>) -> Self {
        let mutes = path
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
                tracing::warn!(%error, "Cannot restore notification mutes");
                None
            })
            .unwrap_or_default();
        let writer =
            path.and_then(
                |path| match state_file::Writer::start("notification-mutes", path) {
                    Ok(writer) => Some(writer),
                    Err(error) => {
                        tracing::warn!(%error, "Cannot start the notification mutes worker");
                        None
                    }
                },
            );
        Self {
            mutes,
            writer,
            quitting: false,
            revision: 0,
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.on_app_quit(|cx| {
            let mutes = cx.global_mut::<Self>();
            mutes.quitting = true;
            let writer = mutes.writer.take();
            cx.background_executor().spawn(async move {
                if let Some(writer) = writer {
                    writer.finish();
                }
            })
        })
        .detach();
    }

    /// Runs `f` against the app's mutes, creating an unsaved set for
    /// fixtures that never installed them.
    pub(crate) fn update<R>(cx: &mut App, f: impl FnOnce(&mut Self) -> R) -> R {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        f(cx.global_mut::<Self>())
    }

    /// The app's mutes, or none for fixtures that never installed them.
    pub(crate) fn of(cx: &App) -> Option<&Self> {
        cx.try_global::<Self>()
    }

    /// The app's mutes while there are any, so what draws or ticks every
    /// frame skips all its work otherwise.
    pub(crate) fn any(cx: &App) -> Option<&Self> {
        Self::of(cx).filter(|mutes| !mutes.is_empty())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.mutes.is_empty()
    }

    /// What the app's mutes silence in `endpoint`'s `snapshot`: nothing while
    /// none are kept.
    pub(crate) fn silenced_in<'a>(
        cx: &App,
        endpoint: &crate::endpoint::Endpoint,
        snapshot: &'a ClientShellSnapshot,
    ) -> Silenced<'a> {
        Self::any(cx)
            .map(|mutes| mutes.silenced(&Daemon::of(endpoint), snapshot))
            .unwrap_or_default()
    }

    /// Read by the Dock badge, which only macOS and Windows draw.
    #[cfg(any(target_os = "macos", windows, test))]
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// What `daemon`'s mutes silence in `snapshot`, in one pass over each of
    /// its lists.
    pub(crate) fn silenced<'a>(
        &self,
        daemon: &Daemon,
        snapshot: &'a ClientShellSnapshot,
    ) -> Silenced<'a> {
        let mine: HashMap<(Scope, &str), &Mute> = self
            .mutes
            .iter()
            .filter(|mute| mute.daemon == *daemon)
            .map(|mute| ((mute.scope, mute.id.as_str()), mute))
            .collect();
        let mut silenced = Silenced::default();
        if mine.is_empty() {
            return silenced;
        }
        let holds = |scope, id: &str, label: &str, named: bool| {
            mine.get(&(scope, id))
                .is_some_and(|mute| mute.holds(snapshot, Target { label, named }))
        };
        silenced.workspaces = snapshot
            .workspaces
            .iter()
            .filter(|workspace| {
                holds(
                    Scope::Workspace,
                    &workspace.workspace_id,
                    &workspace.label,
                    workspace.custom_label,
                )
            })
            .map(|workspace| workspace.workspace_id.as_str())
            .collect();
        silenced.own_tabs = snapshot
            .tabs
            .iter()
            .filter(|tab| holds(Scope::Tab, &tab.tab_id, &tab.label, tab.custom_label))
            .map(|tab| tab.tab_id.as_str())
            .collect();
        if silenced.workspaces.is_empty() && silenced.own_tabs.is_empty() {
            return silenced;
        }
        silenced.tabs = snapshot
            .tabs
            .iter()
            .filter(|tab| {
                silenced.own_tabs.contains(tab.tab_id.as_str())
                    || silenced.workspaces.contains(tab.workspace_id.as_str())
            })
            .map(|tab| tab.tab_id.as_str())
            .collect();
        silenced.panes = snapshot
            .panes
            .iter()
            .filter(|pane| {
                silenced.tabs.contains(pane.tab_id.as_str())
                    || silenced.workspaces.contains(pane.workspace_id.as_str())
            })
            .map(|pane| pane.pane_id.as_str())
            .collect();
        silenced
    }

    /// Mutes or unmutes the workspace or tab `snapshot` shows with `id`, at
    /// `now`. Returns whether anything changed. A target the snapshot does not
    /// show cannot be muted, and one whose ID is too long to keep is refused.
    pub(crate) fn set(
        &mut self,
        daemon: &Daemon,
        snapshot: &ClientShellSnapshot,
        scope: Scope,
        id: &str,
        muted: bool,
        now: SystemTime,
    ) -> bool {
        let position = self
            .mutes
            .iter()
            .position(|mute| mute.is(daemon, scope, id));
        if !muted {
            if position.is_none() {
                return false;
            }
            self.mutes.retain(|old| !old.is(daemon, scope, id));
            self.changed();
            return true;
        }
        let Some(mute) = target(snapshot, scope, id)
            .map(|target| Mute {
                daemon: daemon.clone(),
                scope,
                id: id.to_owned(),
                boot: snapshot.boot_id.clone(),
                label: fit(target.label).into_owned(),
                muted_at: seconds(now),
            })
            .filter(Mute::valid)
        else {
            return false;
        };
        // Muting what is already muted, as this daemon process shows it,
        // changes nothing; one carried over from another process is renewed.
        if position.is_some_and(|index| {
            let old = &self.mutes[index];
            old.boot == mute.boot && old.label == mute.label
        }) {
            return false;
        }
        self.mutes.retain(|old| !old.is(daemon, scope, id));
        self.mutes.push(mute);
        if self.mutes.len() > MAX_MUTES {
            let excess = self.mutes.len() - MAX_MUTES;
            self.mutes.drain(..excess);
        }
        self.changed();
        true
    }

    /// Whether [`Self::reconcile`] would leave `daemon`'s mutes as they are,
    /// so a window can check on every snapshot without touching the global,
    /// whose observers are every window.
    pub(crate) fn current(
        &self,
        daemon: &Daemon,
        snapshot: &ClientShellSnapshot,
        now: SystemTime,
    ) -> bool {
        self.mutes
            .iter()
            .filter(|mute| mute.daemon == *daemon)
            .all(|mute| verdict(mute, snapshot, now) == Verdict::Keep)
    }

    /// Brings `daemon`'s mutes up to date with its latest snapshot at `now`:
    /// a mute follows its target's rename, carries over to a new daemon
    /// process that restored its named target, and is forgotten once its
    /// target is gone. Returns whether anything changed.
    pub(crate) fn reconcile(
        &mut self,
        daemon: &Daemon,
        snapshot: &ClientShellSnapshot,
        now: SystemTime,
    ) -> bool {
        let mut changed = false;
        self.mutes.retain_mut(|mute| {
            if mute.daemon != *daemon {
                return true;
            }
            match verdict(mute, snapshot, now) {
                Verdict::Keep => return true,
                Verdict::Relabel(label) => mute.label = label.into_owned(),
                Verdict::Adopt => mute.boot = snapshot.boot_id.clone(),
                Verdict::Forget => {
                    changed = true;
                    return false;
                }
            }
            changed = true;
            true
        });
        if changed {
            self.changed();
        }
        changed
    }

    fn changed(&mut self) {
        self.revision += 1;
        if self.quitting {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.save(Saved {
                mutes: self.mutes.clone(),
            });
        }
    }

    /// These mutes as the next launch reads them back, at revision 0 like an
    /// empty set, for tests of what keys on the revision.
    #[cfg(test)]
    pub(crate) fn reloaded(&self) -> Self {
        Self {
            mutes: self.mutes.clone(),
            ..Self::default()
        }
    }

    /// Writes what is pending and stops the worker, for tests to read the
    /// file back.
    #[cfg(test)]
    fn finish(&mut self) {
        if let Some(writer) = self.writer.take() {
            writer.finish();
        }
    }
}

#[cfg(test)]
mod tests;
