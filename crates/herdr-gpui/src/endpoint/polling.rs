//! The window's periodic endpoint work: draining each connection, activating
//! the selected surface, probing sessions, and reconciling the saved catalog.
use super::{ACTIVATION_TIMEOUT, Endpoint, LOCAL, Redraw, SAVED_PREFIX};
use crate::{HerdrWindow, state::ConnectionStatus};
use gpui::{ClipboardItem, Context};
use herdr_client::{ConnectTarget, SavedHost};
use std::time::Instant;

impl HerdrWindow {
    /// Writes daemon-forwarded OSC 52 payloads to the pasteboard and reports
    /// the copy the way a local selection does. Returns whether anything was
    /// written, so the window repaints for the flash.
    pub(super) fn apply_clipboard_writes(
        &mut self,
        writes: impl IntoIterator<Item = String>,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut writes = writes.into_iter().peekable();
        if writes.peek().is_none() {
            return false;
        }
        for text in writes {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if self.config.clipboard_toast.enabled {
            self.show_flash(crate::window::Flash::success("copied to clipboard"), cx);
        }
        true
    }

    pub(crate) fn poll_endpoints(&mut self, cx: &mut Context<Self>) {
        // Record the focused target the user last saw before a newer snapshot
        // can replace it; input held across a gap may only go there.
        self.flush_pending_input(cx);
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
        // OSC 52 writes are drained per endpoint so they are written once and
        // never linger in a live state that a later poll would re-read.
        let mut clipboard_writes = std::collections::VecDeque::new();
        for (index, endpoint) in self.endpoints.iter_mut().enumerate() {
            let updated = endpoint.poll(Instant::now());
            clipboard_writes.append(&mut endpoint.live.clipboard_writes);
            // Bells are a presentation effect: only the selected endpoint's
            // ring, as the TUI drops them from inactive endpoints.
            let bells = std::mem::take(&mut endpoint.live.bells);
            if index == self.selected_endpoint {
                self.bell.queue(bells);
            }
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
        if self.apply_clipboard_writes(clipboard_writes, cx) {
            changed = Redraw::Window;
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
                    state.activation = Some(crate::state::SurfaceActivation {
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
        // Held input precedes any deferred navigation.
        self.flush_pending_input(cx);
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
        self.sync_server_keymap(cx);
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
    pub(crate) fn poll_sessions(&mut self, cx: &mut Context<Self>) {
        let development = self.local_development();
        let open = self.menu.page == Some(crate::menu::Page::Sessions);
        let now = Instant::now();
        let targets = self.probe_targets();
        self.sessions.devices.forget_replaced(&targets);
        // Hold the catalog steady through the mutation and its successful exit
        // animation. Workers stay bounded; finished answers wait in their inbox.
        if self.sessions.mutation.is_some() {
            if open && self.sessions.departure.is_some() {
                cx.notify();
            }
            return;
        }
        let mut changed = self.sessions.poll(development, open, now);
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
    pub(crate) fn probe_targets(&self) -> Vec<(String, String)> {
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

    pub(super) fn restore_selection(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn reconcile_catalog(&mut self, hosts: Vec<SavedHost>, cx: &mut Context<Self>) {
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
