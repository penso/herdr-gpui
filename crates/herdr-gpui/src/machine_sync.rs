//! Machine sync in the app: this computer's sync state and saved devices for
//! Settings to show, and the syncs themselves, one at a time, on the
//! background executor (`herdr_client::machine_sync` does the work).
//!
//! The user turns sync on once in Settings, after reading what it changes on
//! each machine. From then on it runs without asking: soon after launch,
//! every hour, when a device is added here, and from Settings' Sync now.
mod suggestion;
pub(crate) use suggestion::Suggestion;

use gpui::{App, Global};
use herdr_client::{
    SavedHost,
    machine_sync::{self, SyncReport, SyncState},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

/// How long after launch the first sync waits, so it never competes with
/// the window's own connections.
const STARTUP_DELAY: Duration = Duration::from_secs(20);
const INTERVAL: Duration = Duration::from_secs(60 * 60);

/// How the last sync ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    Done { report: SyncReport, at: SystemTime },
    Failed(crate::Error),
}

#[derive(Default)]
pub(crate) struct MachineSync {
    // keep-sorted start
    /// Set on quit, so a running sync stops its SSH children.
    cancelled: Arc<AtomicBool>,
    /// This computer's saved SSH devices, in catalog order.
    hosts: Vec<SavedHost>,
    last: Option<Outcome>,
    /// Why this computer's sync state or devices could not be read.
    load_failure: Option<crate::Error>,
    loaded: bool,
    /// Another sync was asked for while one ran.
    queued: bool,
    running: bool,
    /// Why the last change to the state could not be saved.
    save_failure: Option<crate::Error>,
    /// This computer's place in the list, or `None` while sync is off.
    state: Option<SyncState>,
    /// What Settings fills in while sync is off, once looked up.
    suggestion: Option<Suggestion>,
    // keep-sorted end
}

impl Global for MachineSync {}

#[cfg(test)]
impl MachineSync {
    /// A read state and device list, for Settings tests; nothing syncs.
    pub(crate) fn fixture(state: Option<SyncState>, hosts: Vec<SavedHost>) -> Self {
        Self {
            hosts,
            loaded: true,
            state,
            ..Self::default()
        }
    }
}

impl MachineSync {
    /// Reads the state, then keeps the list in step while sync is on. Only
    /// the user's own app does: test modes never touch other machines.
    pub(crate) fn install(cx: &mut App) {
        cx.set_global(Self::default());
        cx.on_app_quit(|cx| {
            cx.global::<Self>().cancelled.store(true, Ordering::Release);
            async {}
        })
        .detach();
        Self::reload(cx, false);
        Self::suggest(cx);
        cx.spawn(async move |cx| {
            cx.background_executor().timer(STARTUP_DELAY).await;
            loop {
                cx.update(Self::sync_if_on);
                cx.background_executor().timer(INTERVAL).await;
            }
        })
        .detach();
    }

    pub(crate) fn state(&self) -> Option<&SyncState> {
        self.state.as_ref()
    }

    pub(crate) fn hosts(&self) -> &[SavedHost] {
        &self.hosts
    }

    pub(crate) fn running(&self) -> bool {
        self.running
    }

    pub(crate) fn last(&self) -> Option<&Outcome> {
        self.last.as_ref()
    }

    /// Why the state could not be read or saved, the save first.
    pub(crate) fn failure(&self) -> Option<&crate::Error> {
        self.save_failure.as_ref().or(self.load_failure.as_ref())
    }

    pub(crate) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(crate) fn suggestion(&self) -> Option<&Suggestion> {
        self.suggestion.as_ref()
    }

    /// Rereads the state and devices, as Settings does when it shows them.
    pub(crate) fn refresh(cx: &mut App) {
        if cx.has_global::<Self>() {
            Self::reload(cx, false);
            // Tailscale may have connected since.
            Self::suggest(cx);
        }
    }

    /// Looks up this computer's suggested address and name.
    fn suggest(cx: &mut App) {
        let task = cx.background_executor().spawn(async { Suggestion::find() });
        cx.spawn(async move |cx| {
            let suggestion = task.await;
            cx.update(|cx| {
                if cx.global::<Self>().suggestion.as_ref() != Some(&suggestion) {
                    cx.global_mut::<Self>().suggestion = Some(suggestion);
                }
            });
        })
        .detach();
    }

    /// A window read this computer's catalog. A device it lists that the
    /// last read did not, saved by the Add Device dialog, a terminal setup,
    /// or Herdr's own CLI, is given to the other machines.
    pub(crate) fn hosts_seen(hosts: &[SavedHost], cx: &mut App) {
        let Some(this) = cx.try_global::<Self>() else {
            return;
        };
        if !this.loaded || this.state.is_none() {
            return;
        }
        if new_device(&this.hosts, hosts) {
            cx.global_mut::<Self>().hosts = hosts.to_vec();
            Self::sync(cx);
            cx.refresh_windows();
        }
    }

    fn reload(cx: &mut App, then_sync: bool) {
        let task = cx.background_executor().spawn(async move {
            let state = machine_sync::load_sync_state(false)?;
            let hosts = herdr_client::load_saved_hosts(false)?;
            Ok::<_, crate::Error>((state, hosts))
        });
        cx.spawn(async move |cx| {
            let result = task.await;
            cx.update(|cx| {
                let this = cx.global_mut::<Self>();
                this.loaded = true;
                match result {
                    Ok((state, hosts)) => {
                        this.state = state;
                        this.hosts = hosts;
                        this.load_failure = None;
                    }
                    Err(error) => {
                        tracing::warn!(category = "machine_sync", %error, "Cannot read machine sync");
                        this.load_failure = Some(error);
                    }
                }
                if then_sync {
                    Self::sync_if_on(cx);
                }
                cx.refresh_windows();
            });
        })
        .detach();
    }

    fn sync_if_on(cx: &mut App) {
        if cx.global::<Self>().state.is_some() {
            Self::sync(cx);
        }
    }

    /// Syncs now, or right after the sync that is running.
    pub(crate) fn sync(cx: &mut App) {
        let this = cx.global_mut::<Self>();
        let Some(state) = this.state.clone() else {
            return;
        };
        if this.running {
            this.queued = true;
            return;
        }
        this.running = true;
        let cancelled = this.cancelled.clone();
        let task = cx.background_executor().spawn(async move {
            machine_sync::sync_machines(false, &state, &cancelled).map_err(crate::Error::from)
        });
        cx.spawn(async move |cx| {
            let result = task.await;
            cx.update(|cx| {
                let this = cx.global_mut::<Self>();
                this.running = false;
                this.last = Some(match result {
                    Ok(report) => {
                        log(&report);
                        Outcome::Done {
                            report,
                            at: SystemTime::now(),
                        }
                    }
                    Err(error) => {
                        tracing::warn!(category = "machine_sync", %error, "Machine sync failed");
                        Outcome::Failed(error)
                    }
                });
                let again = std::mem::take(&mut this.queued);
                // Devices learned from the other machines show at once.
                Self::reload(cx, again);
                cx.refresh_windows();
            });
        })
        .detach();
    }

    /// Saves this computer's address and name, turning sync on when it was
    /// off (the user confirmed what that changes), then syncs.
    pub(crate) fn save(cx: &mut App, label: String, target: String) -> crate::Result<()> {
        let this = cx.global::<Self>();
        let (id, excluded) = match &this.state {
            Some(state) => (state.id.clone(), state.excluded.clone()),
            None => (
                uuid::Uuid::new_v4().simple().to_string(),
                Default::default(),
            ),
        };
        let mut state = SyncState::new(id, label, target).map_err(herdr_client::Error::from)?;
        state.excluded = excluded;
        Self::store(cx, Some(state), true);
        Ok(())
    }

    /// Turns sync off here. The other machines keep what syncs gave them.
    pub(crate) fn turn_off(cx: &mut App) {
        Self::store(cx, None, false);
    }

    /// Takes the saved device `id` into the list, or leaves it out.
    pub(crate) fn set_included(cx: &mut App, id: &str, included: bool) {
        let Some(mut state) = cx.global::<Self>().state.clone() else {
            return;
        };
        if included {
            state.excluded.remove(id);
        } else {
            state.excluded.insert(id.to_owned());
        }
        Self::store(cx, Some(state), false);
    }

    /// Shows `state` at once and writes it in the background; a failed
    /// write rereads the file, so Settings shows what is really saved.
    fn store(cx: &mut App, state: Option<SyncState>, then_sync: bool) {
        cx.global_mut::<Self>().state = state.clone();
        cx.refresh_windows();
        let task = cx.background_executor().spawn(async move {
            machine_sync::store_sync_state(false, state.as_ref()).map_err(crate::Error::from)
        });
        cx.spawn(async move |cx| {
            let result = task.await;
            cx.update(|cx| match result {
                Ok(()) => {
                    cx.global_mut::<Self>().save_failure = None;
                    if then_sync {
                        Self::sync(cx);
                    }
                    cx.refresh_windows();
                }
                Err(error) => {
                    tracing::warn!(category = "machine_sync", %error, "Cannot save machine sync");
                    cx.global_mut::<Self>().save_failure = Some(error);
                    Self::reload(cx, false);
                }
            });
        })
        .detach();
    }
}

/// Whether `seen` lists an enabled device `known` does not.
fn new_device(known: &[SavedHost], seen: &[SavedHost]) -> bool {
    seen.iter()
        .any(|host| host.enabled && !known.iter().any(|known| known.id == host.id))
}

fn log(report: &SyncReport) {
    let failed = report
        .machines
        .iter()
        .filter(|machine| machine.result.is_err())
        .count();
    tracing::info!(
        category = "machine_sync",
        machines = report.machines.len(),
        failed,
        learned = report.learned.len(),
        authorized = report.authorized,
        "Machine sync finished"
    );
}

#[cfg(test)]
mod tests;
