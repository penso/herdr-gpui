//! The taskbar and Start menu jump list on Windows: the checkouts of recently
//! focused workspaces on this computer, and the devices as tasks. Windows
//! shows the recent ones only while "Show recently opened items in Start,
//! Jump Lists, and File Explorer" is on, as for every app. Each entry starts
//! `herdr-gpui.exe` with a `herdr-gpui://` link as its only argument, which
//! the running app follows (see `link`), so an entry never names anything
//! but what to show.
//!
//! Recent checkouts are remembered in `jump-list.json` in the state
//! directory, so the list survives a restart; the shell keeps the list it was
//! last given even while the app is closed. An entry the user removes from
//! the list is forgotten. Building the list takes COM calls and disk writes,
//! so it happens on a worker, and only when the entries change.

mod shell;
#[cfg(test)]
mod tests;

use crate::{SheprdWindow, endpoint::LOCAL, link::Link, state_file};
use gpui::{App, Global, WindowId};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

/// Recent checkouts listed; Windows shows about ten entries by default.
const MAX_RECENT: usize = 8;
const MAX_DEVICES: usize = 8;
const FILE: &str = "jump-list.json";
const MAX_FILE_BYTES: u64 = 64 * 1024;
/// Labels are shown in a narrow menu.
const MAX_LABEL: usize = 80;

/// A checkout a workspace was opened in, as the jump list names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Recent {
    pub path: PathBuf,
    pub label: String,
}

/// One jump list row: what it is called, and the link it opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    pub title: String,
    pub link: Link,
}

/// Everything the jump list shows, compared before the shell is called.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Entries {
    pub recent: Vec<Item>,
    pub devices: Vec<Item>,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    recent: Vec<Recent>,
}

/// Moves the checkout to the front, keeping at most [`MAX_RECENT`].
pub(crate) fn remember(recent: &mut Vec<Recent>, entry: Recent) {
    recent.retain(|old| old.path != entry.path);
    recent.insert(0, entry);
    recent.truncate(MAX_RECENT);
}

/// The jump list for these checkouts and devices. Devices are listed Local
/// first, then by name: each window keeps its own device order, and the
/// list must not change with whichever window looked last.
pub(crate) fn entries(recent: &[Recent], devices: &[(String, String)]) -> Entries {
    let label = |text: &str| crate::notifications::safe_text(text, MAX_LABEL);
    let mut devices: Vec<&(String, String)> = devices.iter().collect();
    devices.sort_by_cached_key(|(id, name)| (id != LOCAL, name.to_lowercase(), id.clone()));
    Entries {
        recent: recent
            .iter()
            .map(|entry| Item {
                title: label(&entry.label),
                link: Link::Open {
                    path: entry.path.clone(),
                },
            })
            .collect(),
        devices: devices
            .iter()
            .take(MAX_DEVICES)
            .map(|(id, name)| Item {
                title: label(&format!("Show {name}")),
                link: Link::Device { id: id.clone() },
            })
            .collect(),
    }
}

/// The workspace a window focuses on this computer's Herdr, when it is a Git
/// checkout a link can open: its launch directory and label.
fn focused_checkout(view: &SheprdWindow) -> Option<Recent> {
    let endpoint = view.endpoints.get(view.selected_endpoint)?;
    if endpoint.id != LOCAL {
        return None;
    }
    let snapshot = view.live.snapshot.as_ref()?;
    let id = snapshot.focused_workspace_id.as_ref()?;
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| &workspace.workspace_id == id)?;
    if workspace.branch.is_none() && workspace.worktree.is_none() {
        return None;
    }
    let root = crate::palette::launch_root(snapshot, id)?;
    let path = PathBuf::from(root);
    // Only what a link accepts: an absolute local path.
    Link::parse(&Link::Open { path: path.clone() }.to_url()).ok()?;
    Some(Recent {
        path,
        label: workspace.label.clone(),
    })
}

/// What the worker shares with the UI thread.
#[derive(Default)]
pub(crate) struct Mailbox {
    /// The latest entries and checkouts to build and save, replacing any the
    /// worker has not taken.
    pub pending: Option<(Entries, Vec<Recent>)>,
    /// Links the user removed from the list, for the app to forget.
    pub removed: Vec<Link>,
    /// Set when the shell refused the last entries, which are then sent
    /// again after [`RETRY`].
    pub failed: bool,
}

/// How long a refused jump list waits before it is sent again.
const RETRY: Duration = Duration::from_secs(30);

/// Whether `entries` go to the worker: when they differ from those last
/// sent, or once a retry after a refusal is due.
fn due(
    published: Option<&Entries>,
    entries: &Entries,
    retry: Option<Instant>,
    now: Instant,
) -> bool {
    published != Some(entries) || retry.is_some_and(|at| now >= at)
}

/// The app's jump list: what it shows and the worker that builds it.
pub(crate) struct JumpList {
    recent: Vec<Recent>,
    /// Each window's focused checkout as last seen, so a checkout is
    /// remembered when focus moves to it, not on every poll.
    focused: HashMap<WindowId, Option<PathBuf>>,
    published: Option<Entries>,
    /// When to send the entries again after the shell refused them.
    retry: Option<Instant>,
    mailbox: Arc<Mutex<Mailbox>>,
    wake: Option<mpsc::SyncSender<()>>,
}

impl Global for JumpList {}

impl JumpList {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Reads the saved checkouts. Called before GPUI starts, like the other
    /// stores; a missing or damaged file starts an empty list.
    pub(crate) fn load() -> Self {
        let recent = Self::path()
            .and_then(|path| {
                state_file::read(&path, MAX_FILE_BYTES)
                    .inspect_err(|error| tracing::warn!(%error, "Cannot read the jump list"))
                    .ok()
                    .flatten()
            })
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .map(|saved| {
                let mut recent = saved.recent;
                recent.truncate(MAX_RECENT);
                recent
            })
            .unwrap_or_default();
        Self {
            recent,
            focused: HashMap::new(),
            published: None,
            retry: None,
            mailbox: Arc::default(),
            wake: None,
        }
    }

    pub(crate) fn install(mut self, cx: &mut App) {
        let mailbox = self.mailbox.clone();
        let path = Self::path();
        // Entries start the windowed launcher when it is installed, so a
        // click opens no console window; a `cargo run` build has none.
        let exe = cx
            .try_global::<crate::taskbar::Taskbar>()
            .and_then(|taskbar| taskbar.launcher().map(std::path::Path::to_owned))
            .or_else(|| std::env::current_exe().ok());
        let (wake, receiver) = mpsc::sync_channel::<()>(1);
        let started = std::thread::Builder::new()
            .name("jump-list".into())
            .spawn(move || shell::run(&receiver, &mailbox, path.as_deref(), exe.as_deref()));
        match started {
            Ok(_) => self.wake = Some(wake),
            Err(error) => tracing::warn!(%error, "The jump list is off"),
        }
        cx.set_global(self);
        cx.on_window_closed(|cx, _| {
            let open = cx.windows();
            cx.global_mut::<Self>()
                .focused
                .retain(|id, _| open.iter().any(|window| window.window_id() == *id));
        })
        .detach();
    }

    /// Notes what a window shows; called from its poll, so it does no work
    /// unless the focused checkout, the devices, or the shell's list changed.
    pub(crate) fn observe(window: WindowId, view: &SheprdWindow, cx: &mut App) {
        let Some(list) = cx.try_global::<Self>() else {
            return;
        };
        let checkout = focused_checkout(view);
        let moved = list.focused.get(&window).cloned().flatten()
            != checkout.as_ref().map(|entry| entry.path.clone());
        let devices: Vec<(String, String)> = view
            .endpoints
            .iter()
            .map(|endpoint| (endpoint.id.clone(), endpoint.label.clone()))
            .collect();
        let list = cx.global_mut::<Self>();
        if moved {
            list.focused
                .insert(window, checkout.as_ref().map(|entry| entry.path.clone()));
            if let Some(checkout) = checkout {
                remember(&mut list.recent, checkout);
            }
        }
        let (removed, failed) = list
            .mailbox
            .lock()
            .map(|mut mailbox| {
                (
                    std::mem::take(&mut mailbox.removed),
                    std::mem::take(&mut mailbox.failed),
                )
            })
            .unwrap_or_default();
        let now = Instant::now();
        if failed {
            list.retry = Some(now + RETRY);
        }
        list.recent.retain(|entry| {
            !removed.contains(&Link::Open {
                path: entry.path.clone(),
            })
        });
        let entries = entries(&list.recent, &devices);
        if !due(list.published.as_ref(), &entries, list.retry, now) {
            return;
        }
        list.retry = None;
        list.published = Some(entries.clone());
        if let Ok(mut mailbox) = list.mailbox.lock() {
            mailbox.pending = Some((entries, list.recent.clone()));
        }
        if let Some(wake) = &list.wake {
            // Full means the worker is already due to look.
            let _ = wake.try_send(());
        }
    }
}
