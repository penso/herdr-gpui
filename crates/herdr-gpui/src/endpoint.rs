//! GUI-owned endpoint catalog and connection lifetimes. Each attempt has its own
//! inbox, so retired workers can never publish into a replacement connection.
use super::{
    HerdrWindow, LiveState, NavigationTarget, WheelAccumulator, connection::ConnectionBridge,
    state::ConnectionStatus,
};
use crate::{Error, Result};
use gpui::Context;
use herdr_client::{ClientHandle, ConnectOptions, ConnectTarget, SavedHost};
use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub(super) const LOCAL: &str = "local";
/// Saved SSH endpoints are keyed `ssh:<profile-id>`, so no catalog ID can
/// collide with `LOCAL`.
const SAVED_PREFIX: &str = "ssh:";

/// The catalog profile ID behind a saved SSH endpoint's ID, which is what the
/// `herdr machine` commands and per-device credentials are keyed by.
pub(crate) fn saved_profile_id(endpoint_id: &str) -> Option<&str> {
    endpoint_id
        .strip_prefix(SAVED_PREFIX)
        .filter(|id| herdr_client::valid_profile_id(id))
}
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(5);
const STABLE_CONNECTION_PERIOD: Duration = Duration::from_secs(60);
/// Upstream rechecks failed SSH machines every 30 seconds, so authentication
/// repaired outside the app (a new master, a loaded key) is picked up promptly.
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

/// How much of the window an update changes. Ordered, so several updates
/// combine into the widest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Redraw {
    #[default]
    None,
    /// Only the terminal surface: the sidebar keeps its last layout.
    Terminal,
    Window,
}

pub(super) struct Release {
    inbox: Arc<Mutex<LiveState>>,
    drained: Arc<AtomicBool>,
    phase: ReleasePhase,
    boot: String,
}

enum ReleasePhase {
    Deferred(ClientHandle),
    Sent(String),
    Disconnecting,
}

impl Release {
    fn resolved(&mut self) -> bool {
        // Disconnect() requests shutdown; only the event receiver closing proves
        // that this generation's transport is gone. Catalog removal alone is not
        // sufficient evidence to let another surface take ownership.
        if self.drained.load(Ordering::Acquire) {
            return true;
        }
        let Ok(mut state) = self.inbox.try_lock() else {
            return false;
        };
        if let ReleasePhase::Deferred(handle) = &self.phase {
            // Queue and register under the same lock, but never wait for that lock
            // on the UI thread. The destination stays fenced until acknowledgement.
            if !state.status.is_connected()
                || state
                    .snapshot
                    .as_ref()
                    .is_none_or(|s| s.boot_id != self.boot)
            {
                handle.disconnect();
                self.phase = ReleasePhase::Disconnecting;
                return false;
            }
            state.set_outer_focus(false);
            state.surface = None;
            state.dirty = true;
            let result = handle
                .set_focus(&self.boot, false)
                .and_then(|()| handle.set_surface_active(&self.boot, false));
            match result {
                Ok(request) => {
                    state.activation = Some(super::state::SurfaceActivation {
                        request: request.clone(),
                        boot: self.boot.clone(),
                        revision: None,
                        failed: false,
                        focus: None,
                        active: false,
                    });
                    self.phase = ReleasePhase::Sent(request);
                }
                Err(_) => {
                    handle.disconnect();
                    self.phase = ReleasePhase::Disconnecting;
                }
            }
        }
        match &self.phase {
            ReleasePhase::Sent(request) => state.activation.as_ref().is_some_and(|a| {
                a.request == *request
                    && a.boot == self.boot
                    && !a.active
                    && !a.failed
                    && a.revision.is_some()
            }),
            _ => false,
        }
    }
}

pub(super) struct Endpoint {
    pub id: String,
    pub label: String,
    pub connection: ConnectionBridge,
    pub enabled: bool,
    /// The saved entry this endpoint was last reconciled against. Its own session
    /// may have been picked in the sessions list since, so a catalog change can
    /// only be told from such a pick by remembering what the catalog said.
    saved_host: Option<SavedHost>,
    pub collapsed: bool,
    pub collapsed_repos: HashSet<String>,
    pub live: LiveState,
    pub generation: u64,
    pub(crate) toasts: crate::notifications::Toasts,
    retry_at: Instant,
    attempts: u32,
    online_since: Option<Instant>,
    detached: bool,
    initial_surface: bool,
    sounds: crate::sound::Policy,
}

impl Endpoint {
    pub fn surface_requested(&self) -> bool {
        self.initial_surface
    }
    pub fn new(id: String, label: String, target: ConnectTarget, enabled: bool) -> Self {
        Self {
            id,
            label,
            connection: ConnectionBridge::new(target),
            enabled,
            saved_host: None,
            collapsed: false,
            collapsed_repos: HashSet::new(),
            live: LiveState::default(),
            generation: 0,
            toasts: Default::default(),
            retry_at: Instant::now(),
            attempts: 0,
            online_since: None,
            detached: false,
            initial_surface: false,
            sounds: Default::default(),
        }
    }

    fn stop(&mut self) {
        self.toasts.entries.clear();
        self.connection.detach(false);
        self.initial_surface = false;
        self.online_since = None;
        self.generation += 1;
        self.live = self.connection.take_update().unwrap_or_default();
    }

    /// The SSH target and session this device was saved with. The sessions list
    /// may have pointed the live connection at another of the host's sessions,
    /// so whatever speaks for the saved device (duplicate checks, the device's
    /// own menu) reads this instead. One never reconciled against the catalog
    /// has only its live target to go on.
    pub(crate) fn saved_ssh(&self) -> Option<(&str, &str)> {
        if let Some(host) = &self.saved_host {
            return Some((&host.target, &host.session));
        }
        match &self.connection.target {
            ConnectTarget::Ssh { target, session } => Some((target, session)),
            _ => None,
        }
    }

    /// Point this endpoint at another target, retiring the old transport. The
    /// endpoint keeps its identity, label, and sidebar state; nothing the old
    /// connection produced survives it.
    fn retarget(&mut self, target: ConnectTarget) {
        self.stop();
        self.connection = ConnectionBridge::new(target);
        self.detached = false;
        self.attempts = 0;
        // The replacement transport has produced no state of its own yet.
        self.live = LiveState::default();
    }

    fn connect(&mut self, options: ConnectOptions, active: bool) {
        self.stop();
        self.detached = false;
        self.initial_surface = active;
        self.attempts = self.attempts.saturating_add(1);
        self.connection.reconnect(options, false, active);
        self.live = self.connection.take_update().unwrap_or_default();
        self.toasts.receive(self.live.notifications.drain(..));
        self.retry_at = Instant::now() + self.retry_delay();
    }

    fn poll(&mut self, now: Instant) -> Redraw {
        let mut changed = Redraw::None;
        if let Some(mut state) = self.connection.take_update() {
            changed = if self.live.only_surface_changed(&state) {
                Redraw::Terminal
            } else {
                Redraw::Window
            };
            if state.notifications_lost
                || !state.status.is_connected()
                || self
                    .live
                    .snapshot
                    .as_ref()
                    .zip(state.snapshot.as_ref())
                    .is_some_and(|(old, new)| old.boot_id != new.boot_id)
            {
                self.toasts.entries.clear();
            }
            self.toasts.receive(state.notifications.drain(..));
            self.live = state;
        }
        if self
            .connection
            .handle
            .as_ref()
            .is_some_and(ClientHandle::is_disconnected)
        {
            self.connection.handle = None;
            self.retry_at = now + self.retry_delay();
            changed = Redraw::Window;
        }
        if self.connection.handle.is_some()
            && self.live.status.is_connected()
            && self.live.snapshot.is_some()
        {
            let since = self.online_since.get_or_insert(now);
            if now.duration_since(*since) >= STABLE_CONNECTION_PERIOD {
                self.attempts = 0;
            }
        } else {
            self.online_since = None;
        }
        changed
    }

    fn retry_delay(&self) -> Duration {
        Duration::from_millis(500u64 << self.attempts.min(8)).min(MAX_RETRY_DELAY)
    }

    pub fn status(&self) -> &'static str {
        if !self.enabled {
            "disabled"
        } else if self.detached {
            "detached"
        } else if self.live.status.is_connected() {
            "online"
        } else if self.live.error.is_some() {
            "reconnecting"
        } else {
            "connecting"
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(super) struct Catalog {
    development: Option<bool>,
    pending: Option<mpsc::Receiver<Result<CatalogUpdate>>>,
    next_poll: Instant,
    desired: Option<String>,
    initialized: bool,
    restore_pending: bool,
    queued_write: Option<Option<String>>,
    writing: Option<mpsc::Receiver<Result<()>>>,
}

struct CatalogUpdate {
    hosts: Vec<SavedHost>,
    selection: Option<Option<String>>,
}

impl Catalog {
    pub fn new(target: &ConnectTarget) -> Self {
        Self {
            development: match target {
                ConnectTarget::Socket(_) => None,
                ConnectTarget::Session { development, .. } => Some(*development),
                _ => Some(false),
            },
            pending: None,
            next_poll: Instant::now(),
            desired: None,
            initialized: false,
            restore_pending: false,
            queued_write: None,
            writing: None,
        }
    }

    fn poll(&mut self) -> Option<Result<CatalogUpdate>> {
        let development = self.development?;
        if let Some(result) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.pending = None;
            self.next_poll = Instant::now() + Duration::from_secs(2);
            return Some(result);
        }
        if self.pending.is_none() && Instant::now() >= self.next_poll {
            let (tx, rx) = mpsc::sync_channel(1);
            self.pending = Some(rx);
            let startup = !self.initialized;
            if let Err(error) = std::thread::Builder::new()
                .name("herdr-gui-catalog".into())
                .spawn(move || {
                    let result = if startup {
                        herdr_client::load_saved_host_selection(development).map(
                            |(hosts, selection)| CatalogUpdate {
                                hosts,
                                selection: Some(selection),
                            },
                        )
                    } else {
                        herdr_client::load_saved_hosts(development).map(|hosts| CatalogUpdate {
                            hosts,
                            selection: None,
                        })
                    };
                    let _ = tx.send(result.map_err(Error::from));
                })
            {
                self.pending = None;
                self.next_poll = Instant::now() + Duration::from_secs(2);
                return Some(Err(error.into()));
            }
        }
        None
    }

    fn accept(&mut self, update: &CatalogUpdate) {
        if !self.initialized {
            self.desired = update.selection.clone().flatten();
            self.restore_pending = self.desired.is_some();
            self.initialized = true;
        }
        if self.desired.as_ref().is_some_and(|id| {
            !update
                .hosts
                .iter()
                .any(|host| host.enabled && &host.id == id)
        }) {
            self.desired = None;
            self.restore_pending = false;
        }
    }

    fn choose(&mut self, id: &str) {
        // Also cancels an in-flight startup restore when Local is clicked.
        self.initialized = true;
        self.restore_pending = false;
        self.desired = id.strip_prefix(SAVED_PREFIX).map(str::to_owned);
        if self.development.is_some() {
            self.queued_write = Some(self.desired.clone());
        }
    }

    fn poll_write(&mut self) -> Option<Error> {
        let development = self.development?;
        let mut error = None;
        if let Some(result) = self.writing.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.writing = None;
            error = result.err();
        }
        // Serialize this client's writes so rapid choices cannot finish backwards.
        if self.writing.is_none()
            && let Some(selected) = self.queued_write.take()
        {
            let (tx, rx) = mpsc::sync_channel(1);
            match std::thread::Builder::new()
                .name("herdr-gui-selection".into())
                .spawn(move || {
                    let _ = tx.send(
                        herdr_client::store_saved_host_selection(development, selected.as_deref())
                            .map_err(Error::from),
                    );
                }) {
                Ok(_) => self.writing = Some(rx),
                Err(e) => error = Some(e.into()),
            }
        }
        error
    }
}

impl HerdrWindow {
    pub(super) fn reconnect(&mut self) {
        let index = self.selected_endpoint;
        if !self.endpoints[index].enabled {
            return;
        }
        self.install_warning_shown = false;
        self.endpoints[index].attempts = 0;
        self.endpoints[index].connect(self.options, index == 0);
        self.reset_selected();
    }

    pub(super) fn detach_endpoint(&mut self) {
        let endpoint = &mut self.endpoints[self.selected_endpoint];
        endpoint.stop();
        endpoint.detached = true;
        endpoint.live.status = ConnectionStatus::Detached;
        if let Ok(mut state) = endpoint.connection.inbox.lock() {
            *state = endpoint.live.clone();
        }
        self.reset_selected();
    }

    fn reset_selected(&mut self) {
        if let Some(transfer) = &self.file_transfer {
            transfer.cancel();
        }
        for image in &self.pending_images {
            image.cancel();
        }
        self.menu.reset();
        self.selection_epoch += 1;
        let endpoint = &self.endpoints[self.selected_endpoint];
        self.selected_generation = endpoint.generation;
        self.live = endpoint.live.clone();
        if !endpoint.initial_surface {
            self.live.surface = None;
        }
        // Another connection's picture is not this one's, so a reconnect, a
        // detach, or a switch of endpoint starts from an empty terminal area.
        self.presentation.clear();
        self.selection = None;
        self.terminal_mouse = None;
        self.pressed_terminal_link = None;
        self.flash = None;
        self.local_error = None;
        self.marked.clear();
        self.last_queued_options = None;
        self.sent_focus = None;
        self.wheel = WheelAccumulator::default();
        self.activation_deadline = (self.selected_endpoint != 0 && !endpoint.detached)
            .then(|| Instant::now() + ACTIVATION_TIMEOUT);
        self.pending_navigation = None;
        self.pending_toast = None;
    }

    pub(super) fn select_endpoint(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if !self.switch_endpoint(id, cx) {
            return false;
        }
        self.catalog.choose(id);
        if let Some(error) = self.catalog.poll_write() {
            self.local_error = Some(format!("Save host selection: {error}"));
            cx.notify();
        }
        true
    }

    /// The installation this window's local endpoint belongs to. A development
    /// target lists the development catalog's sessions, not the release ones.
    pub(super) fn local_development(&self) -> bool {
        matches!(
            self.endpoints[0].connection.target,
            ConnectTarget::Session {
                development: true,
                ..
            }
        )
    }

    /// Attach this window to another named local session. The local endpoint
    /// keeps its identity, so the session changes its target rather than adding
    /// an endpoint for every session on the machine.
    pub(super) fn select_local_session(&mut self, name: &str, cx: &mut Context<Self>) {
        let target = ConnectTarget::Session {
            name: name.to_owned(),
            development: self.local_development(),
        };
        if self.selected_endpoint == 0 && self.endpoints[0].connection.target == target {
            return;
        }
        // Retarget before selecting: the poll loop reconnects endpoint zero, so
        // it must never still name the session it is leaving.
        if self.endpoints[0].connection.target != target {
            self.endpoints[0].retarget(target);
        }
        if self.selected_endpoint != 0 && !self.switch_endpoint(LOCAL, cx) {
            return;
        }
        // This is a deliberate move off any remote selection, which must not be
        // restored over it on the next launch.
        self.catalog.choose(LOCAL);
        self.reconnect();
        // After the reconnect: resetting the connection clears the error slot.
        if let Some(error) = self.catalog.poll_write() {
            self.local_error = Some(format!("Save host selection: {error}"));
        }
        cx.notify();
    }

    /// Attach this window to another session of a saved device. The device keeps
    /// its identity, so the session changes that endpoint's target rather than
    /// adding an endpoint for every session the host has.
    pub(super) fn select_device_session(
        &mut self,
        id: &str,
        session: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .endpoints
            .iter()
            .position(|endpoint| endpoint.id == id && endpoint.enabled)
        else {
            return;
        };
        // Only an SSH device has a session to name; the local endpoint has its
        // own path through `select_local_session`.
        let ConnectTarget::Ssh { target, .. } = &self.endpoints[index].connection.target else {
            return;
        };
        let target = ConnectTarget::Ssh {
            target: target.clone(),
            session: session.to_owned(),
        };
        if self.selected_endpoint == index && self.endpoints[index].connection.target == target {
            return;
        }
        // Retarget before switching, exactly as attaching to a local session
        // does: the poll loop reconnects this endpoint, so it must never still
        // name the session the window is leaving.
        let retargeted = self.endpoints[index].connection.target != target;
        if retargeted {
            self.endpoints[index].retarget(target);
        }
        if self.selected_endpoint != index && !self.switch_endpoint(id, cx) {
            return;
        }
        self.catalog.choose(id);
        // Only a session this device was not already on needs a new connection:
        // choosing the device itself keeps the transport it has, the way choosing
        // it from the picker does.
        if retargeted {
            self.reconnect();
        }
        // After the reconnect: resetting the connection clears the error slot.
        if let Some(error) = self.catalog.poll_write() {
            self.local_error = Some(format!("Save host selection: {error}"));
        }
        cx.notify();
    }

    fn switch_endpoint(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.endpoints.iter().position(|e| e.id == id && e.enabled) else {
            return false;
        };
        if index == self.selected_endpoint {
            return true;
        }
        if index != 0
            && self.endpoints[self.selected_endpoint]
                .live
                .status
                .is_connected()
            && !self.endpoints[self.selected_endpoint].live.supports_surface
        {
            self.local_error = Some("Current endpoint does not support surface switching".into());
            cx.notify();
            return false;
        }
        self.release_selected();
        if index == 0 {
            // Local is the escape hatch and never waits on a remote release. An
            // unsent release must still retire its exact source transport.
            for release in &self.pending_releases {
                if let ReleasePhase::Deferred(handle) = &release.phase {
                    handle.disconnect();
                }
            }
            self.pending_releases.clear();
        }
        self.selected_endpoint = index;
        if self.device_filter.is_some() {
            self.device_filter = Some(id.to_owned());
        }
        self.reset_selected();
        self.activation_deadline =
            (!self.endpoints[index].detached).then(|| Instant::now() + ACTIVATION_TIMEOUT);
        cx.notify();
        true
    }

    fn release_selected(&mut self) {
        let endpoint = &mut self.endpoints[self.selected_endpoint];
        // A handshake started with an active surface cannot be demoted without
        // its boot ID. Retire that attempt rather than let it finish in background.
        if endpoint.initial_surface
            && (endpoint.connection.handle.is_none() || endpoint.live.snapshot.is_none())
        {
            endpoint.stop();
            endpoint.retry_at = Instant::now();
            return;
        }
        if endpoint.initial_surface
            && let (Some(handle), Some(snapshot)) =
                (&endpoint.connection.handle, &endpoint.live.snapshot)
        {
            let mut release = Release {
                inbox: endpoint.connection.inbox.clone(),
                drained: endpoint.connection.drained.clone(),
                phase: ReleasePhase::Deferred(handle.clone()),
                boot: snapshot.boot_id.clone(),
            };
            if !release.resolved() {
                self.pending_releases.push(release);
            }
        } else if let Ok(mut state) = endpoint.connection.inbox.try_lock() {
            state.set_outer_focus(false);
            state.surface = None;
        }
        endpoint.initial_surface = false;
        endpoint.live.surface = None;
    }

    pub(super) fn navigate_endpoint(
        &mut self,
        endpoint: &str,
        target: NavigationTarget<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() {
            return false;
        }
        if !self.select_endpoint(endpoint, cx) {
            return false;
        }
        self.pending_toast = None;
        self.pending_navigation = None;
        if self.input_ready() {
            self.navigate(target, cx);
        } else {
            self.pending_navigation = Some((&target).into());
        }
        true
    }

    pub(super) fn input_ready(&self) -> bool {
        self.pending_toast.is_none() && self.navigation_ready()
    }

    // A coherent surface permits the deferred navigation attempt, not terminal
    // input while its toast target is still waiting for inbox validation.
    pub(crate) fn navigation_ready(&self) -> bool {
        self.endpoints[self.selected_endpoint]
            .connection
            .handle
            .is_some()
            && self.endpoints[self.selected_endpoint].surface_requested()
            && self.pending_releases.is_empty()
            && self.live.surface_ready()
            && self.live.surface.as_ref().is_some_and(|surface| {
                surface.frame.width == self.options.surface_size.cols
                    && surface.frame.height == self.options.surface_size.rows
            })
    }

    pub(super) fn poll_endpoints(&mut self, cx: &mut Context<Self>) {
        if let Some(error) = self.catalog.poll_write() {
            self.local_error = Some(format!("Save host selection: {error}"));
            cx.notify();
        }
        if let Some(result) = self.catalog.poll() {
            match result {
                Ok(update) => {
                    self.catalog.accept(&update);
                    self.reconcile_catalog(update.hosts, cx);
                }
                Err(error) => {
                    self.local_error = Some(format!("Host catalog: {error}"));
                    cx.notify();
                }
            }
        }
        let mut changed = Redraw::None;
        // Whether the selected endpoint itself moved on. Only then does the
        // window take its state: another endpoint changing, or this one's
        // inbox being busy for a poll, must not replace what the window has
        // stamped since, such as the split drag request it is waiting on.
        let mut selected_changed = false;
        for (index, endpoint) in self.endpoints.iter_mut().enumerate() {
            let updated = endpoint.poll(Instant::now());
            selected_changed |= index == self.selected_endpoint && updated != Redraw::None;
            self.sound.poll(
                &mut endpoint.sounds,
                &mut endpoint.live,
                index == self.selected_endpoint && self.active,
                Instant::now(),
            );
            changed = changed.max(updated);
            // Remote cwd strings must never be resolved against this machine's Git repos.
            if updated == Redraw::Window
                && index == 0
                && let (Some(avatars), Some(snapshot)) =
                    (&mut self.avatars, &endpoint.live.snapshot)
            {
                for workspace in &snapshot.workspaces {
                    avatars.request(&workspace.new_workspace_cwd);
                }
            }
            if endpoint.enabled
                && !endpoint.detached
                && endpoint.connection.handle.is_none()
                && Instant::now() >= endpoint.retry_at
            {
                endpoint.connect(self.options, index == 0 && self.selected_endpoint == 0);
                selected_changed |= index == self.selected_endpoint;
                changed = Redraw::Window;
            }
        }
        self.restore_selection(cx);
        if self.tick_toasts(
            self.menu.page.is_some() || self.toasts_hidden,
            Instant::now(),
        ) {
            changed = Redraw::Window;
        }
        let endpoint = &mut self.endpoints[self.selected_endpoint];
        if self.selected_generation != endpoint.generation {
            self.reset_selected();
        }
        let endpoint = &mut self.endpoints[self.selected_endpoint];
        if selected_changed {
            self.live = endpoint.live.clone();
            if !self.live.status.is_connected() {
                self.local_error = None;
            }
        }
        self.pending_releases
            .retain_mut(|release| !release.resolved());
        if !endpoint.initial_surface
            && self.pending_releases.is_empty()
            && let (Some(handle), Some(snapshot)) =
                (&endpoint.connection.handle, &endpoint.live.snapshot)
            && let Ok(mut state) = endpoint.connection.inbox.try_lock()
        {
            let result = handle
                .resize(&snapshot.boot_id, self.options)
                .and_then(|()| handle.set_surface_active(&snapshot.boot_id, true));
            match result {
                Ok(request) => {
                    state.surface = None;
                    state.activation = Some(super::state::SurfaceActivation {
                        request,
                        boot: snapshot.boot_id.clone(),
                        revision: None,
                        failed: false,
                        focus: None,
                        active: true,
                    });
                    state.dirty = true;
                    endpoint.initial_surface = true;
                    self.live = state.clone();
                    // take_update already moved this response out of the inbox.
                    // Installing an activation fence must not discard that delivery.
                    self.live.dialog_response = endpoint.live.dialog_response.clone();
                    self.activation_deadline = Some(Instant::now() + ACTIVATION_TIMEOUT);
                    self.last_queued_options = Some(self.options);
                    self.sent_focus = None;
                }
                Err(error) => {
                    self.local_error = Some(format!("Activate: {error}"));
                    self.activation_deadline = Some(Instant::now());
                }
            }
            changed = Redraw::Window;
        }
        if self.navigation_ready() {
            self.activation_deadline = None;
            if let Some(id) = self.pending_toast {
                self.navigate_toast(id, cx);
            } else if let Some(target) = self.pending_navigation.take() {
                self.dispatch_navigation(target.as_deref(), cx);
            }
        } else if self
            .activation_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
            || self.live.activation.as_ref().is_some_and(|a| a.failed)
            || (self.selected_endpoint != 0 && self.live.status == ConnectionStatus::Disconnected)
        {
            let mut error = format!(
                "{}: surface activation failed or timed out",
                self.endpoints[self.selected_endpoint].label
            );
            if let Some(reason) = &self.live.error {
                error.push_str(&format!(": {reason}"));
            }
            if self.selected_endpoint != 0 {
                self.switch_endpoint(LOCAL, cx);
            } else {
                // Recover Local with a fresh active handshake, even if the
                // previous surface lane or its acknowledgement was unavailable.
                self.reconnect();
            }
            self.local_error = Some(error);
            changed = Redraw::Window;
        }
        match changed {
            Redraw::None => {}
            Redraw::Terminal => self.redraw_terminal(cx),
            Redraw::Window => cx.notify(),
        }
    }

    /// Scan local sessions while the popup asks for them, and ask the saved
    /// devices for their own sessions on their own slower interval. Probing is
    /// I/O to the disk and to each host, so both run on workers and only this
    /// page starts one; results are cached between opens.
    pub(super) fn poll_sessions(&mut self, cx: &mut Context<Self>) {
        let development = self.local_development();
        let open = self.menu.page == Some(crate::menu::Page::Sessions);
        let now = Instant::now();
        let mut changed = self.sessions.poll(development, open, now);
        let targets = self.probe_targets();
        if self.sessions.devices.poll(&targets, open, now) {
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// The devices this window may ask for their sessions: every enabled saved
    /// device reachable over SSH. One the user disabled is never dialled, and the
    /// local endpoint has no host to ask.
    pub(super) fn probe_targets(&self) -> Vec<(String, String)> {
        self.endpoints
            .iter()
            .skip(1)
            .filter(|endpoint| endpoint.enabled)
            .filter_map(|endpoint| match &endpoint.connection.target {
                ConnectTarget::Ssh { target, .. } => Some((endpoint.id.clone(), target.clone())),
                _ => None,
            })
            .collect()
    }

    fn restore_selection(&mut self, cx: &mut Context<Self>) {
        if !self.catalog.restore_pending {
            return;
        }
        let Some(id) = self
            .catalog
            .desired
            .as_ref()
            .map(|id| format!("{SAVED_PREFIX}{id}"))
        else {
            return;
        };
        if self.endpoints.iter().any(|endpoint| {
            endpoint.id == id
                && endpoint.enabled
                && endpoint.connection.handle.is_some()
                && endpoint.live.status.is_connected()
                && endpoint.live.snapshot.is_some()
        }) {
            // One handoff attempt: activation failure may fall back to Local,
            // but must neither overwrite the preference nor loop on every tick.
            self.catalog.restore_pending = false;
            self.switch_endpoint(&id, cx);
        }
    }

    pub(super) fn reconcile_catalog(&mut self, hosts: Vec<SavedHost>, cx: &mut Context<Self>) {
        let selected = &self.endpoints[self.selected_endpoint];
        let selected_id = selected.id.clone();
        let selected_retired = self.selected_endpoint != 0
            && !hosts.iter().any(|host| {
                format!("{SAVED_PREFIX}{}", host.id) == selected_id
                    && host.enabled
                    && !entry_changed(selected, host)
            });
        if selected_retired {
            self.switch_endpoint(LOCAL, cx);
        }
        let selected_id = self.endpoints[self.selected_endpoint].id.clone();
        let mut previous = std::mem::take(&mut self.endpoints);
        let mut next = vec![previous.remove(0)];
        for host in hosts {
            let id = format!("{SAVED_PREFIX}{}", host.id);
            let mut endpoint = if let Some(index) = previous.iter().position(|e| e.id == id) {
                previous.remove(index)
            } else {
                Endpoint::new(
                    id,
                    host.label.clone(),
                    ConnectTarget::Ssh {
                        target: host.target.clone(),
                        session: host.session.clone(),
                    },
                    host.enabled,
                )
            };
            let changed = endpoint.enabled != host.enabled || entry_changed(&endpoint, &host);
            if changed {
                endpoint.stop();
                endpoint.attempts = 0;
                endpoint.connection.target = ConnectTarget::Ssh {
                    target: host.target.clone(),
                    session: host.session.clone(),
                };
                endpoint.enabled = host.enabled;
                endpoint.detached = false;
                endpoint.retry_at = Instant::now();
            }
            endpoint.label = host.label.clone();
            endpoint.saved_host = Some(host);
            next.push(endpoint);
        }
        self.endpoints = next;
        self.selected_endpoint = self
            .endpoints
            .iter()
            .position(|e| e.id == selected_id)
            .unwrap_or(0);
        cx.notify();
    }
}

/// Whether a saved entry differs from the one this endpoint was last reconciled
/// against, which is what an edit to a device's saved profile looks like. An
/// endpoint that has never been reconciled compares its live target instead, so
/// one built outside the catalog still retires when its entry changes.
fn entry_changed(endpoint: &Endpoint, host: &SavedHost) -> bool {
    match &endpoint.saved_host {
        Some(saved) => saved.target != host.target || saved.session != host.session,
        None => !same_target(&endpoint.connection.target, host),
    }
}

/// Whether an endpoint's live target is exactly the saved entry's, session
/// included. A device's identity as the catalog describes it; the sessions list
/// deliberately points an endpoint at other sessions of the same device.
fn same_target(target: &ConnectTarget, host: &SavedHost) -> bool {
    matches!(target, ConnectTarget::Ssh { target, session } if target == &host.target && session == &host.session)
}

// The fixtures drive the real client over bound Unix sockets and POSIX processes.
#[cfg(all(test, unix))]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use herdr_client::ClientEvent;

    #[test]
    fn saved_profile_ids_are_the_catalog_ids_behind_ssh_endpoints() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(saved_profile_id(&format!("ssh:{id}")), Some(id));
        for endpoint in [id, LOCAL, "ssh:", "ssh:fixture", "ssh:../x"] {
            assert_eq!(saved_profile_id(endpoint), None, "{endpoint}");
        }
    }

    fn host(id: &str, enabled: bool) -> SavedHost {
        SavedHost {
            id: id.into(),
            label: id.into(),
            target: format!("user@{id}"),
            session: "default".into(),
            enabled,
        }
    }

    #[test]
    fn notifications_stay_endpoint_owned_and_expire_without_new_updates() {
        use crate::notifications::tests::notification;
        use herdr_client::protocol::ServerMessage;
        let mut local = Endpoint::new(LOCAL.into(), "Local".into(), ConnectTarget::Local, true);
        let mut remote = Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            ConnectTarget::Local,
            true,
        );
        for endpoint in [&mut local, &mut remote] {
            let mut state = endpoint.connection.inbox.lock().unwrap();
            state.apply(ClientEvent::Snapshot(Arc::new(
                crate::sidebar::layout_tests::snapshot(1),
            )));
            state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                notification(&endpoint.label),
            )));
            state.apply(ClientEvent::Snapshot(Arc::new(
                crate::sidebar::layout_tests::snapshot(2),
            )));
        }
        let now = Instant::now();
        assert_eq!(local.poll(now), Redraw::Window);
        assert_eq!(remote.poll(now), Redraw::Window);
        assert!(local.live.notifications.is_empty());
        assert!(remote.live.notifications.is_empty());
        assert_eq!(local.toasts.entries[0].1.title, "Local");
        assert_eq!(remote.toasts.entries[0].1.title, "Remote");
        assert_eq!(remote.poll(now), Redraw::None);
        local.stop();
        assert!(local.toasts.entries.is_empty());
        assert_eq!(remote.toasts.entries.len(), 1);
        let mut endpoints = [local, remote];
        let config = crate::config::NotificationConfig {
            enabled: true,
            delay_seconds: 0,
            ..Default::default()
        };
        assert!(crate::notifications::tick(
            &mut endpoints,
            0,
            config,
            false,
            None,
            now
        ));
        let deadline = endpoints[1].toasts.entries[0].1.expires;
        assert!(crate::notifications::tick(
            &mut endpoints,
            0,
            config,
            false,
            None,
            deadline
        ));
        assert!(endpoints[1].toasts.entries.is_empty());
    }

    #[test]
    fn lost_ingress_retires_predecessors_even_when_replacement_was_evicted() {
        use crate::notifications::{Notice, PENDING_LIMIT, tests::notification};
        use herdr_client::protocol::{SemanticNotificationKind, ServerMessage};
        let mut endpoint = Endpoint::new(LOCAL.into(), "Local".into(), ConnectTarget::Local, true);
        let mut wire = notification("old attention");
        wire.pane_id = Some("p".into());
        let mut old = Notice::new(wire.clone(), Instant::now()).preview();
        old.promote(Instant::now());
        endpoint.toasts.receive([old]);
        let inbox = endpoint.connection.inbox.clone();
        {
            let mut state = inbox.lock().unwrap();
            state.status = ConnectionStatus::Connected;
            wire.kind = SemanticNotificationKind::Finished;
            state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                wire,
            )));
            for index in 0..PENDING_LIMIT {
                state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                    notification(&index.to_string()),
                )));
            }
            assert!(state.notifications_lost);
            assert_eq!(state.notifications.len(), PENDING_LIMIT);
            assert!(state.notifications.iter().all(|n| n.pane_id.is_none()));
        }
        assert_eq!(endpoint.poll(Instant::now()), Redraw::Window);
        assert_eq!(endpoint.toasts.entries.len(), PENDING_LIMIT);
        assert!(
            endpoint
                .toasts
                .entries
                .iter()
                .all(|(_, n)| n.title != "old attention")
        );
        // The loss marker moves with the batch exactly once, not every snapshot.
        inbox.lock().unwrap().dirty = true;
        assert_eq!(
            endpoint.poll(Instant::now()),
            Redraw::Terminal,
            "an update that changes nothing the chrome reads spares the sidebar"
        );
        assert!(!endpoint.live.notifications_lost);
        assert_eq!(endpoint.toasts.entries.len(), PENDING_LIMIT);
    }

    #[test]
    fn notifications_clear_on_boot_change_and_disconnect() {
        use crate::notifications::tests::notification;
        use herdr_client::protocol::ServerMessage;
        let mut endpoint = Endpoint::new(LOCAL.into(), "Local".into(), ConnectTarget::Local, true);
        let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
        endpoint
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Snapshot(Arc::new(snapshot.clone())));
        endpoint
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                notification("old"),
            )));
        endpoint.poll(Instant::now());
        assert_eq!(endpoint.toasts.entries.len(), 1);
        endpoint
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                notification("pending old"),
            )));
        snapshot.boot_id = "replacement-boot".into();
        endpoint
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Snapshot(Arc::new(snapshot)));
        endpoint.poll(Instant::now());
        assert!(endpoint.toasts.entries.is_empty());
        endpoint
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                notification("new"),
            )));
        endpoint.poll(Instant::now());
        assert_eq!(endpoint.toasts.entries.len(), 1);
        endpoint
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Disconnected {
                reason: "test".into(),
            });
        endpoint.poll(Instant::now());
        assert!(endpoint.toasts.entries.is_empty());
    }

    #[test]
    fn retired_generation_cannot_publish_into_replacement() {
        let mut endpoint = Endpoint::new(LOCAL.into(), "Local".into(), ConnectTarget::Local, true);
        let old = endpoint.connection.inbox.clone();
        let generation = endpoint.generation;
        endpoint.stop();
        assert!(endpoint.generation > generation);
        old.lock().unwrap().apply(ClientEvent::Snapshot(Arc::new(
            crate::sidebar::layout_tests::snapshot(1),
        )));
        endpoint.poll(Instant::now());
        assert!(endpoint.live.snapshot.is_none());
        assert!(!Arc::ptr_eq(&old, &endpoint.connection.inbox));
    }

    #[test]
    fn explicit_socket_never_reads_shared_catalog() {
        let mut catalog = Catalog::new(&ConnectTarget::Socket("/unused.sock".into()));
        assert!(catalog.poll().is_none());
        assert!(catalog.pending.is_none());
        catalog.choose(LOCAL);
        assert!(catalog.queued_write.is_none());
        assert!(catalog.poll_write().is_none());
        assert!(catalog.writing.is_none());
        assert!(
            Catalog::new(&ConnectTarget::Session {
                name: "test".into(),
                development: true
            })
            .development
                == Some(true)
        );
    }

    #[test]
    fn selection_writes_are_serialized_and_failure_keeps_the_ui_choice() {
        let mut catalog = Catalog::new(&ConnectTarget::Local);
        let (tx, rx) = mpsc::sync_channel(1);
        catalog.writing = Some(rx);
        catalog.choose("ssh:first");
        catalog.choose(LOCAL);
        catalog.choose("ssh:last");
        assert!(catalog.poll_write().is_none());
        assert!(catalog.writing.is_some());
        assert_eq!(catalog.queued_write, Some(Some("last".into())));
        // Simulate a failed worker without accessing the real user's state root.
        catalog.queued_write = None;
        tx.send(Err(std::io::Error::other("disk unavailable").into()))
            .unwrap();
        assert!(
            matches!(catalog.poll_write(), Some(Error::Io(error)) if error.to_string() == "disk unavailable")
        );
        assert!(catalog.writing.is_none());
        assert_eq!(catalog.desired.as_deref(), Some("last"));
        assert!(!catalog.restore_pending);
    }

    #[test]
    fn desired_selection_is_client_local_and_catalog_changes_cancel_stale_restore() {
        let update = |enabled, selection| CatalogUpdate {
            hosts: vec![host("a", enabled)],
            selection,
        };
        let mut first = Catalog::new(&ConnectTarget::Local);
        let mut second = Catalog::new(&ConnectTarget::Local);
        first.accept(&update(true, Some(Some("a".into()))));
        second.accept(&update(true, Some(Some("a".into()))));
        second.choose(LOCAL);
        first.accept(&update(true, Some(None)));
        assert_eq!(first.desired.as_deref(), Some("a"));
        assert!(first.restore_pending);
        assert_eq!(second.desired, None);
        first.accept(&update(false, None));
        assert_eq!(first.desired, None);
        assert!(!first.restore_pending);
        first.accept(&update(true, None));
        assert!(!first.restore_pending);
        let mut clicked = Catalog::new(&ConnectTarget::Local);
        clicked.choose(LOCAL);
        clicked.accept(&update(true, Some(Some("a".into()))));
        assert_eq!(
            clicked.desired, None,
            "late startup read cannot undo a click"
        );
        assert_eq!(clicked.queued_write, Some(None));
        second.choose("ssh:a");
        second.accept(&CatalogUpdate {
            hosts: vec![],
            selection: None,
        });
        assert_eq!(second.desired, None);
    }

    #[test]
    fn release_waits_for_its_own_successful_inactive_acknowledgement() {
        let inbox = Arc::new(Mutex::new(LiveState::default()));
        inbox.lock().unwrap().activation = Some(crate::state::SurfaceActivation {
            request: "off".into(),
            boot: "boot".into(),
            revision: None,
            failed: false,
            focus: None,
            active: false,
        });
        let mut release = Release {
            inbox: inbox.clone(),
            drained: Arc::new(AtomicBool::new(false)),
            phase: ReleasePhase::Sent("off".into()),
            boot: "boot".into(),
        };
        assert!(!release.resolved());
        let response = serde_json::json!({"result": {
            "type": "client_shell_surface_set", "active": false, "projection_revision": 7
        }});
        inbox.lock().unwrap().apply(ClientEvent::Response {
            request_id: "old".into(),
            response: response.clone(),
        });
        assert!(!release.resolved());
        inbox.lock().unwrap().apply(ClientEvent::Response {
            request_id: "off".into(),
            response,
        });
        assert!(release.resolved());
        inbox.lock().unwrap().activation.as_mut().unwrap().failed = true;
        assert!(!release.resolved());
        inbox.lock().unwrap().activation = None;
        assert!(
            !release.resolved(),
            "discarding the acknowledgement is not retirement"
        );
        release.drained.store(true, Ordering::Release);
        assert!(
            release.resolved(),
            "confirmed transport termination releases ownership"
        );
    }

    #[gpui::test]
    fn catalog_preserves_order_labels_and_scoped_collapse_but_retires_changed_targets(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.reconcile_catalog(vec![host("b", true), host("a", false)], cx);
            assert_eq!(
                view.endpoints
                    .iter()
                    .map(|e| e.id.as_str())
                    .collect::<Vec<_>>(),
                [LOCAL, "ssh:b", "ssh:a"]
            );
            assert_eq!(view.endpoints[2].status(), "disabled");
            assert!(!view.select_endpoint("ssh:a", cx));
            view.endpoints[1]
                .collapsed_repos
                .insert("/same/repo".into());
            view.endpoints[1].collapsed = true;
            let inbox = view.endpoints[1].connection.inbox.clone();
            let mut renamed = host("b", true);
            renamed.label = "renamed".into();
            view.reconcile_catalog(vec![renamed.clone(), host("a", true)], cx);
            assert!(Arc::ptr_eq(&inbox, &view.endpoints[1].connection.inbox));
            assert_eq!(view.endpoints[1].label, "renamed");
            assert!(view.endpoints[1].collapsed);
            assert!(view.endpoints[2].collapsed_repos.is_empty());
            assert!(view.select_endpoint("ssh:b", cx));
            let epoch = view.selection_epoch;
            renamed.session = "changed".into();
            view.reconcile_catalog(vec![host("a", true), renamed], cx);
            assert_eq!(view.selected_endpoint, 0);
            assert!(view.selection_epoch > epoch);
            assert!(!Arc::ptr_eq(&inbox, &view.endpoints[2].connection.inbox));
            view.select_endpoint("ssh:a", cx);
            view.reconcile_catalog(vec![], cx);
            assert_eq!(view.selected_endpoint, 0);
            assert_eq!(view.endpoints.len(), 1);
        });
    }

    /// The sessions list retargets a device to another of its sessions without
    /// touching the catalog, which still names the session the device was saved
    /// with. Reconciliation must not read that pick as a change of device and drag
    /// the window back to the session it was previously on.
    #[gpui::test]
    fn a_session_picked_from_the_device_list_survives_catalog_reconciliation(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.reconcile_catalog(vec![host("b", true)], cx);
            assert!(view.select_endpoint("ssh:b", cx));
            view.select_device_session("ssh:b", "other", cx);
            assert!(matches!(
                &view.endpoints[1].connection.target,
                ConnectTarget::Ssh { session, .. } if session == "other"
            ));
            view.reconcile_catalog(vec![host("b", true)], cx);
            assert_eq!(view.selected_endpoint, 1);
            assert!(matches!(
                &view.endpoints[1].connection.target,
                ConnectTarget::Ssh { session, .. } if session == "other"
            ));
        });
    }

    #[gpui::test]
    fn device_filter_follows_navigation_and_catalog_retirement(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.reconcile_catalog(vec![host("a", true), host("b", true)], cx);
            assert!(view.select_endpoint("ssh:a", cx));
            assert!(
                view.device_filter.is_none(),
                "All Devices stays an aggregate"
            );
            view.device_filter = Some("ssh:a".into());
            assert!(view.select_endpoint("ssh:b", cx));
            assert_eq!(view.device_filter.as_deref(), Some("ssh:b"));
            let mut renamed = host("b", true);
            renamed.label = "Renamed device".into();
            view.reconcile_catalog(vec![renamed], cx);
            assert_eq!(view.device_filter.as_deref(), Some("ssh:b"));
            view.reconcile_catalog(vec![host("b", false)], cx);
            assert_eq!(view.device_filter.as_deref(), Some(LOCAL));
            assert!(!view.select_endpoint("ssh:b", cx));
            view.reconcile_catalog(vec![host("b", true)], cx);
            assert!(view.select_endpoint("ssh:b", cx));
            view.reconcile_catalog(vec![], cx);
            assert_eq!(view.device_filter.as_deref(), Some(LOCAL));
        });
    }

    #[gpui::test]
    fn switching_away_retires_an_active_handshake_without_a_boot_id(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.reconcile_catalog(vec![host("remote", true)], cx);
            view.endpoints[0].initial_surface = true;
            let inbox = view.endpoints[0].connection.inbox.clone();
            assert!(view.select_endpoint("ssh:remote", cx));
            assert!(!view.endpoints[0].initial_surface);
            assert!(!Arc::ptr_eq(&inbox, &view.endpoints[0].connection.inbox));
            assert!(view.pending_releases.is_empty());
        });
    }

    #[gpui::test]
    fn timeout_and_return_to_local_do_not_wait_for_remote_release(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        view.update(cx, |view, cx| {
            view.active = true;
            let painted = view.endpoints[view.selected_endpoint]
                .connection
                .inbox
                .clone();
            let epoch = view.selection_epoch;
            let generation = view.endpoints[0].generation;
            // Selection epoch, connection generation and inbox identity together
            // decide whether deferred work still belongs to the current endpoint.
            let current = |view: &HerdrWindow| {
                view.selection_epoch == epoch
                    && view.endpoints[view.selected_endpoint].generation == generation
                    && Arc::ptr_eq(
                        &view.endpoints[view.selected_endpoint].connection.inbox,
                        &painted,
                    )
            };
            assert!(current(view));
            view.reconcile_catalog(vec![host("remote", true)], cx);
            for endpoint in &mut view.endpoints {
                endpoint.detached = true;
            }
            view.select_endpoint("ssh:remote", cx);
            assert!(!current(view));
            view.pending_releases.push(Release {
                inbox: view.endpoints[view.selected_endpoint]
                    .connection
                    .inbox
                    .clone(),
                drained: Arc::new(AtomicBool::new(false)),
                phase: ReleasePhase::Sent("never-acked".into()),
                boot: "boot".into(),
            });
            view.activation_deadline = Some(Instant::now());
            view.poll_endpoints(cx);
            assert_eq!(view.selected_endpoint, 0);
            assert!(!current(view));
            assert!(view.pending_releases.is_empty());
            assert!(view.local_error.as_ref().unwrap().contains("timed out"));
            view.select_endpoint("ssh:remote", cx);
            let remote = view.endpoints[view.selected_endpoint]
                .connection
                .inbox
                .clone();
            view.select_endpoint(LOCAL, cx);
            assert!(!Arc::ptr_eq(
                &remote,
                &view.endpoints[view.selected_endpoint].connection.inbox
            ));
            assert!(view.pending_navigation.is_none());
        });
    }
}
