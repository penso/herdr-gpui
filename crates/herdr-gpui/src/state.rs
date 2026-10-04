use herdr_client::{
    ClientEvent, Method, SurfaceImages,
    protocol::{ClientShellSnapshot, PaneSurfaceFrame, ServerMessage},
};
use std::sync::Arc;

pub(crate) type DialogResponse = Result<serde_json::Value, Arc<crate::Error>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    StartingDaemon,
    AwaitingSnapshot,
    Connected,
    Disconnected,
    Detached,
}

impl ConnectionStatus {
    pub fn is_connected(self) -> bool {
        matches!(self, Self::AwaitingSnapshot | Self::Connected)
    }
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Connecting => "Connecting...",
            Self::StartingDaemon => "Starting Herdr server...",
            Self::AwaitingSnapshot => "Connected; waiting for snapshot",
            Self::Connected => "Connected",
            Self::Disconnected => "Disconnected",
            Self::Detached => "Detached (daemon still running)",
        })
    }
}

#[derive(Clone)]
pub struct LiveState {
    pub(crate) settings_reload: bool,
    pub(crate) sound_events: std::collections::VecDeque<(
        std::time::Instant,
        herdr_client::protocol::SemanticNotification,
    )>,
    pub(crate) reload_sound: bool,
    /// Decoded OSC 52 clipboard writes from the daemon, in arrival order. The
    /// UI thread drains them to the pasteboard; the queue is bounded like
    /// sounds, since a pane may write faster than the window repaints.
    pub(crate) clipboard_writes: std::collections::VecDeque<String>,
    /// Terminal bells forwarded since the UI last drained them. Only their
    /// presence matters to the window, so a saturating count bounds a burst.
    pub(crate) bells: u16,
    /// The outer window title the daemon pushed: an agent's
    /// `client.window_title.set`, or its rendered `ui.window_title`. `None`
    /// leaves the window on its own title. Sanitized on receipt.
    pub(crate) window_title: Option<String>,
    pub(crate) sound_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) sound_connection_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub snapshot: Option<Arc<ClientShellSnapshot>>,
    /// The pane focused before the current one, in any workspace or tab of
    /// this daemon boot, for Herdr's `last_pane`.
    pub(crate) previous_pane: Option<String>,
    pub surface: Option<Arc<PaneSurfaceFrame>>,
    /// Pixels for the images the connection's surfaces place, by asset key.
    pub(crate) surface_images: Arc<SurfaceImages>,
    pub status: ConnectionStatus,
    pub error: Option<String>,
    pub missing_installation: bool,
    /// Same-user peer at the owned standard socket, not executable attestation.
    pub(crate) local_daemon_peer: bool,
    /// `pane.clear` arrived after Herdr 0.9.1; older daemons reject it.
    pub(crate) supports_pane_clear: bool,
    /// `tab.move` reorders a workspace's tabs; daemons that do not offer it
    /// to clients keep their tabs where they are.
    pub(crate) supports_tab_move: bool,
    /// `workspace.move` takes a single workspace; without it a drop sends
    /// every reorder as `workspace.move_block`.
    pub(crate) supports_workspace_move: bool,
    /// `pane.link.resolve` and `pane.link.activate` let the daemon find links
    /// across wrapped rows and run plugin link handlers; without them links
    /// are found row by row here and always opened by this client.
    pub(crate) supports_link_resolve: bool,
    pub(crate) supports_link_activate: bool,
    /// `pane.copy_search` drives the find bar; without it Find says so.
    pub(crate) supports_copy_search: bool,
    /// `pane.selection.read` copies selections reaching beyond the screen.
    pub(crate) supports_selection_read: bool,
    /// `pane.copy_motion` drives copy mode's text motions.
    pub(crate) supports_copy_motion: bool,
    /// `pane.edit_scrollback` opens a pane's history in the user's editor.
    pub(crate) supports_edit_scrollback: bool,
    pub dirty: bool,
    pub(crate) dialog_response: Option<(String, Option<DialogResponse>)>,
    pub(crate) notifications: std::collections::VecDeque<crate::notifications::Notice>,
    pub(crate) notifications_lost: bool,
    outer_focused: Option<bool>,
    pub activation: Option<SurfaceActivation>,
    pub supports_surface: bool,
    // Bounded rename slots survive coalesced snapshots and do not overwrite a
    // worktree operation whose dialog has already closed.
    pub tab_rename: Option<RenameResult>,
    pub pane_rename: Option<RenameResult>,
    /// The one scrollbar or split drag request in flight; the next waits for
    /// it so a slow link coalesces to the latest position instead of queueing
    /// a backlog.
    pub drag_request: Option<String>,
    /// The product announcement this client dismissed. It stays hidden until
    /// the daemon drops it from the snapshot, and reappears if the daemon
    /// rejects the dismissal.
    pub(crate) announcement_dismissal: Option<AnnouncementDismissal>,
    /// Both dismiss methods arrived with endpoint announcements; a daemon that
    /// does not offer one can only have it hidden for this connection.
    pub(crate) supports_announcement_dismiss: bool,
    pub(crate) supports_release_notes_dismiss: bool,
    /// The daemon's `ClientShellKeyboardReportAll`: the focused pane asked for
    /// every key, including releases, as escape codes. Keys the window already
    /// sends as key events then also send their release.
    pub(crate) keyboard_report_all: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnnouncementDismissal {
    /// `None` when the daemon does not offer `product_announcement.dismiss`.
    pub request: Option<String>,
    pub version: String,
    pub id: String,
}

impl AnnouncementDismissal {
    fn covers(&self, snapshot: &ClientShellSnapshot) -> bool {
        snapshot
            .product_announcement
            .as_ref()
            .is_some_and(|current| current.version == self.version && current.id == self.id)
    }
}

#[derive(Clone)]
pub struct RenameResult {
    pub request: String,
    pub result: Option<Result<(), Arc<crate::Error>>>,
}

#[derive(Clone)]
pub struct SurfaceActivation {
    pub request: String,
    pub boot: String,
    pub revision: Option<u64>,
    pub failed: bool,
    pub focus: Option<crate::OwnedNavigationTarget>,
    pub active: bool,
}

impl Default for LiveState {
    fn default() -> Self {
        Self {
            settings_reload: false,
            sound_events: Default::default(),
            reload_sound: false,
            clipboard_writes: Default::default(),
            bells: 0,
            window_title: None,
            sound_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sound_connection_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            snapshot: None,
            previous_pane: None,
            surface: None,
            surface_images: Default::default(),
            status: ConnectionStatus::Connecting,
            error: None,
            missing_installation: false,
            local_daemon_peer: false,
            supports_pane_clear: false,
            supports_tab_move: false,
            supports_workspace_move: false,
            supports_link_resolve: false,
            supports_link_activate: false,
            supports_copy_search: false,
            supports_selection_read: false,
            supports_copy_motion: false,
            supports_edit_scrollback: false,
            dirty: true,
            dialog_response: None,
            notifications: Default::default(),
            notifications_lost: false,
            keyboard_report_all: false,
            outer_focused: None,
            activation: None,
            supports_surface: false,
            tab_rename: None,
            pane_rename: None,
            drag_request: None,
            announcement_dismissal: None,
            supports_announcement_dismiss: false,
            supports_release_notes_dismiss: false,
        }
    }
}

impl LiveState {
    /// Whether `next` repaints only the terminal: everything else the window
    /// draws from, the sidebar above all, reads as it did in `self`. Every
    /// field is named so a new one must decide whether it can change quietly.
    pub(crate) fn only_surface_changed(&self, next: &Self) -> bool {
        let Self {
            sound_events,
            reload_sound,
            settings_reload,
            clipboard_writes,
            bells,
            window_title,
            sound_cancel,
            sound_connection_cancel,
            snapshot,
            previous_pane,
            surface: _,
            surface_images: _,
            status,
            error,
            missing_installation,
            local_daemon_peer,
            supports_pane_clear,
            supports_tab_move,
            supports_workspace_move,
            supports_link_resolve,
            supports_link_activate,
            supports_copy_search,
            supports_selection_read,
            supports_copy_motion,
            supports_edit_scrollback,
            dirty: _,
            dialog_response,
            notifications,
            notifications_lost,
            outer_focused,
            activation,
            supports_surface,
            tab_rename,
            pane_rename,
            drag_request,
            announcement_dismissal,
            supports_announcement_dismiss,
            supports_release_notes_dismiss,
            // Shapes the next key events, not anything drawn.
            keyboard_report_all: _,
        } = next;
        let same_arc = |a: &Option<Arc<_>>, b: &Option<Arc<_>>| match (a, b) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (a, b) => a.is_none() && b.is_none(),
        };
        let pending_rename = |a: &Option<RenameResult>, b: &Option<RenameResult>| match (a, b) {
            (Some(a), Some(b)) => {
                a.request == b.request && a.result.is_none() && b.result.is_none()
            }
            (a, b) => a.is_none() && b.is_none(),
        };
        sound_events.is_empty()
            && !reload_sound
            && !settings_reload
            && clipboard_writes.is_empty()
            && *bells == 0
            && *window_title == self.window_title
            && Arc::ptr_eq(sound_cancel, &self.sound_cancel)
            && Arc::ptr_eq(sound_connection_cancel, &self.sound_connection_cancel)
            && same_arc(snapshot, &self.snapshot)
            && *previous_pane == self.previous_pane
            && *status == self.status
            && *error == self.error
            && *missing_installation == self.missing_installation
            && *local_daemon_peer == self.local_daemon_peer
            && *supports_pane_clear == self.supports_pane_clear
            && *supports_tab_move == self.supports_tab_move
            && *supports_workspace_move == self.supports_workspace_move
            && *supports_link_resolve == self.supports_link_resolve
            && *supports_link_activate == self.supports_link_activate
            && *supports_copy_search == self.supports_copy_search
            && *supports_selection_read == self.supports_selection_read
            && *supports_copy_motion == self.supports_copy_motion
            && *supports_edit_scrollback == self.supports_edit_scrollback
            && match (dialog_response, &self.dialog_response) {
                (Some((a, None)), Some((b, None))) => a == b,
                (a, b) => a.is_none() && b.is_none(),
            }
            && notifications.is_empty()
            && !notifications_lost
            && *outer_focused == self.outer_focused
            && match (activation, &self.activation) {
                (Some(a), Some(b)) => {
                    a.request == b.request
                        && a.boot == b.boot
                        && a.revision == b.revision
                        && a.failed == b.failed
                        && a.focus == b.focus
                        && a.active == b.active
                }
                (a, b) => a.is_none() && b.is_none(),
            }
            && *supports_surface == self.supports_surface
            && pending_rename(tab_rename, &self.tab_rename)
            && pending_rename(pane_rename, &self.pane_rename)
            && *drag_request == self.drag_request
            && *announcement_dismissal == self.announcement_dismissal
            && *supports_announcement_dismiss == self.supports_announcement_dismiss
            && *supports_release_notes_dismiss == self.supports_release_notes_dismiss
    }

    fn has_operation_result(&self, request_id: &str) -> bool {
        self.dialog_response
            .as_ref()
            .is_some_and(|(id, _)| id == request_id)
            || [&self.tab_rename, &self.pane_rename]
                .into_iter()
                .flatten()
                .any(|rename| rename.request == request_id)
            || self.dismissal_request(request_id)
    }

    fn dismissal_request(&self, request_id: &str) -> bool {
        self.announcement_dismissal
            .as_ref()
            .and_then(|dismissal| dismissal.request.as_deref())
            == Some(request_id)
    }

    /// The daemon's product announcement, unless this client dismissed it.
    pub(crate) fn product_announcement(
        &self,
    ) -> Option<&herdr_client::protocol::ClientShellProductAnnouncement> {
        let snapshot = self.snapshot.as_deref()?;
        if self
            .announcement_dismissal
            .as_ref()
            .is_some_and(|dismissal| dismissal.covers(snapshot))
        {
            return None;
        }
        snapshot.product_announcement.as_ref()
    }

    /// Whether a navigation barrier is unacknowledged, failed, or still waiting
    /// for its focus. An acknowledged activation stays recorded afterwards, so
    /// its presence alone does not mean navigation is in flight.
    pub fn activation_pending(&self) -> bool {
        self.activation.as_ref().is_some_and(|activation| {
            activation.failed
                || !activation.active
                || activation.revision.is_none()
                || activation.focus.is_some()
        })
    }

    pub fn surface_ready(&self) -> bool {
        let (Some(snapshot), Some(surface)) = (&self.snapshot, &self.surface) else {
            return false;
        };
        coherent(snapshot, surface)
            && self.activation.as_ref().is_none_or(|activation| {
                !activation.failed
                    && activation.active
                    && activation.boot == snapshot.boot_id
                    && activation
                        .revision
                        .is_some_and(|revision| surface.projection_revision >= revision)
                    && activation.focus.as_ref().is_none_or(|target| match target {
                        crate::NavigationTarget::Workspace(id) => {
                            snapshot.focused_workspace_id.as_ref() == Some(id)
                        }
                        crate::NavigationTarget::Tab(id) => {
                            snapshot.focused_tab_id.as_ref() == Some(id)
                        }
                        crate::NavigationTarget::Pane(id) => {
                            snapshot.focused_pane_id.as_ref() == Some(id)
                        }
                    })
            })
    }

    pub fn daemon_starting(&mut self) {
        self.status = ConnectionStatus::StartingDaemon;
        self.dirty = true;
    }

    pub fn status_text(&self, local_error: Option<&str>) -> String {
        let error = if self.status.is_connected() {
            local_error.or(self.error.as_deref())
        } else {
            self.error.as_deref()
        };
        match error {
            Some(error) => format!("{}: {error}", self.status),
            None => self.status.to_string(),
        }
    }

    /// Track activation without treating receipt or focus gain as presentation.
    pub fn set_outer_focus(&mut self, focused: bool) {
        // A focus report retried after inbox contention must still cause a draw.
        self.dirty |= self.outer_focused != Some(focused);
        self.outer_focused = Some(focused);
    }

    pub fn apply(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::Connected(welcome) => {
                self.settings_reload = false;
                self.supports_pane_clear = Method::PaneClear.advertised_in(&welcome.methods);
                self.supports_tab_move = Method::TabMove.advertised_in(&welcome.methods);
                self.supports_workspace_move =
                    Method::WorkspaceMove.advertised_in(&welcome.methods);
                self.supports_announcement_dismiss =
                    Method::ProductAnnouncementDismiss.advertised_in(&welcome.methods);
                self.supports_release_notes_dismiss =
                    Method::ReleaseNotesDismiss.advertised_in(&welcome.methods);
                self.supports_link_resolve =
                    crate::links::LinkRequest::Resolve.advertised_in(&welcome.methods);
                self.supports_link_activate =
                    crate::links::LinkRequest::Activate.advertised_in(&welcome.methods);
                self.supports_copy_search = Method::PaneCopySearch.advertised_in(&welcome.methods);
                self.supports_selection_read =
                    Method::PaneSelectionRead.advertised_in(&welcome.methods);
                self.supports_copy_motion = Method::PaneCopyMotion.advertised_in(&welcome.methods);
                self.supports_edit_scrollback =
                    Method::PaneEditScrollback.advertised_in(&welcome.methods);
                self.supports_surface = Method::ClientShellSurfaceSet
                    .advertised_in(&welcome.methods)
                    && ["surface_interest", "presentation_effects_fence"]
                        .iter()
                        .all(|capability| {
                            welcome.capabilities.iter().any(|value| value == capability)
                        });
                self.missing_installation = false;
                // The daemon reports the mode to each new connection.
                self.keyboard_report_all = false;
                self.status = ConnectionStatus::AwaitingSnapshot;
                self.error = None;
            }
            ClientEvent::Snapshot(snapshot) => {
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|old| old.boot_id != snapshot.boot_id)
                {
                    self.notifications.clear();
                    self.cancel_sounds();
                    self.sound_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    // A restarted daemon pushes its own title; the old one's
                    // must not outlive it.
                    self.bells = 0;
                    self.window_title = None;
                }
                if let Some(activation) = &mut self.activation
                    && activation.boot != snapshot.boot_id
                {
                    activation.failed = true;
                }
                self.missing_installation = false;
                if self
                    .surface
                    .as_ref()
                    .is_some_and(|s| !coherent(&snapshot, s))
                {
                    self.surface = None;
                }
                // Once the daemon drops or replaces the announcement there is
                // nothing left to hide.
                if self
                    .announcement_dismissal
                    .as_ref()
                    .is_some_and(|dismissal| !dismissal.covers(&snapshot))
                {
                    self.announcement_dismissal = None;
                }
                self.status = ConnectionStatus::Connected;
                // Pane IDs are only meaningful within one daemon boot.
                self.previous_pane = match &self.snapshot {
                    Some(old) if old.boot_id == snapshot.boot_id => old
                        .focused_pane_id
                        .clone()
                        .filter(|old| Some(old) != snapshot.focused_pane_id.as_ref())
                        .or_else(|| self.previous_pane.take()),
                    _ => None,
                };
                self.snapshot = Some(snapshot);
            }
            ClientEvent::Surface(surface) => {
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| coherent(s, &surface))
                {
                    self.surface = Some(surface);
                }
            }
            ClientEvent::SurfaceImages(images) => self.surface_images = images,
            ClientEvent::Disconnected { reason } => {
                self.settings_reload = false;
                self.notifications.clear();
                self.cancel_sounds();
                self.keyboard_report_all = false;
                self.bells = 0;
                self.window_title = None;
                self.status = ConnectionStatus::Disconnected;
                self.error = Some(reason);
                self.snapshot = None;
                self.surface = None;
                self.announcement_dismissal = None;
                self.surface_images = Default::default();
            }
            ClientEvent::CommandRejected { request_id, reason } => {
                if request_id.is_some() && request_id == self.drag_request {
                    self.drag_request = None;
                }
                if !request_id
                    .as_deref()
                    .is_some_and(|id| self.has_operation_result(id))
                {
                    self.error = Some(reason.to_string());
                }
                let reason = Arc::new(crate::Error::Client(reason));
                if let Some((id, result)) = &mut self.dialog_response
                    && request_id.as_ref() == Some(id)
                {
                    *result = Some(Err(reason.clone()));
                }
                for rename in [&mut self.tab_rename, &mut self.pane_rename]
                    .into_iter()
                    .flatten()
                {
                    if request_id.as_ref() == Some(&rename.request) {
                        rename.result = Some(Err(reason.clone()));
                    }
                }
                if let Some(activation) = &mut self.activation
                    && request_id.as_ref() == Some(&activation.request)
                {
                    activation.failed = true;
                }
                if request_id
                    .as_deref()
                    .is_some_and(|id| self.dismissal_request(id))
                {
                    self.announcement_dismissal = None;
                }
            }
            ClientEvent::Response {
                request_id,
                response,
            } => {
                if self.drag_request.as_ref() == Some(&request_id) {
                    self.drag_request = None;
                }
                for rename in [&mut self.tab_rename, &mut self.pane_rename]
                    .into_iter()
                    .flatten()
                {
                    if request_id == rename.request {
                        rename.result = Some(
                            match response.get("error").filter(|error| !error.is_null()) {
                                Some(error) => {
                                    Err(Arc::new(crate::Error::DaemonResponse(error.clone())))
                                }
                                None => Ok(()),
                            },
                        );
                    }
                }
                if let Some(activation) = &mut self.activation
                    && request_id == activation.request
                {
                    let result = &response["result"];
                    activation.revision = (response.get("error").is_none_or(|e| e.is_null())
                        && result["type"] == "client_shell_surface_set"
                        && result["active"] == activation.active)
                        .then(|| result["projection_revision"].as_u64())
                        .flatten();
                    activation.failed = activation.revision.is_none();
                    if activation.failed {
                        self.error = Some("Invalid surface activation acknowledgement".into());
                    }
                }
                if let Some(error) = response.get("error")
                    && !error.is_null()
                    && !self.has_operation_result(&request_id)
                {
                    self.error = Some(crate::Error::DaemonResponse(error.clone()).to_string());
                }
                if self.dismissal_request(&request_id)
                    && response.get("error").is_some_and(|error| !error.is_null())
                {
                    self.announcement_dismissal = None;
                }
                if let Some((id, result)) = &mut self.dialog_response
                    && *id == request_id
                {
                    *result = Some(Ok(response));
                }
            }
            ClientEvent::Message(ServerMessage::ClientShellError { message }) => {
                self.error = Some(message)
            }
            ClientEvent::Message(ServerMessage::SemanticNotification(notification)) => {
                if !self.status.is_connected() {
                    return;
                }
                if self.sound_events.len() == crate::sound::MAX_PENDING {
                    self.sound_events.pop_front();
                }
                let received = std::time::Instant::now();
                self.sound_events
                    .push_back((received, notification.clone()));
                if let Some(pane) = notification.pane_id.as_ref() {
                    self.notifications
                        .retain(|n| n.pane_id.as_ref() != Some(pane));
                }
                if self.notifications.len() == crate::notifications::PENDING_LIMIT {
                    self.notifications.pop_front();
                    // A dropped event may have invalidated an already displayed pane.
                    self.notifications_lost = true;
                }
                self.notifications.push_back(
                    crate::notifications::Notice::new(notification, received)
                        .with_snapshot(self.snapshot.as_deref()),
                );
            }
            ClientEvent::Message(ServerMessage::ReloadSoundConfig) => {
                self.reload_sound = true;
                self.settings_reload = true;
            }
            ClientEvent::Message(ServerMessage::Clipboard { data }) => {
                // OSC 52 bytes from a pane, base64-encoded by the daemon. Only
                // bounded UTF-8 text is written; anything else is dropped.
                if let Some(text) = crate::osc52::decode(&data) {
                    while self.clipboard_writes.len() >= crate::osc52::MAX_PENDING {
                        self.clipboard_writes.pop_front();
                    }
                    self.clipboard_writes.push_back(text);
                } else {
                    tracing::debug!("dropped an invalid or oversized clipboard payload");
                }
            }
            ClientEvent::Message(ServerMessage::TerminalBell { count }) => {
                if count == 0 || !self.status.is_connected() {
                    return;
                }
                self.bells = self.bells.saturating_add(count);
            }
            ClientEvent::Message(ServerMessage::WindowTitle { title }) => {
                let title = title.as_deref().and_then(sanitize_window_title);
                if title == self.window_title {
                    return;
                }
                self.window_title = title;
            }
            ClientEvent::Message(ServerMessage::ClientShellKeyboardReportAll { enabled }) => {
                if enabled == self.keyboard_report_all {
                    return;
                }
                self.keyboard_report_all = enabled;
            }
            // Herdr's own TUI drops `Notify` in client-shell mode as well. A
            // shell client is notified through `SemanticNotification`, which
            // drives toasts and sounds here; `Notify` is the flat form for
            // direct terminal clients.
            ClientEvent::Message(ServerMessage::Notify { .. })
            // Tells the TUI whether to turn on its outer terminal's mouse
            // reporting. Every native mouse event already reaches this window,
            // and each surface pane's `mouse_reporting` decides between local
            // selection and forwarding for the pane under the pointer.
            | ClientEvent::Message(ServerMessage::MouseCapture { .. })
            // Sent only to direct `herdr attach` terminal clients, never to a
            // client shell; the daemon encodes keys for each pane itself.
            | ClientEvent::Message(ServerMessage::DirectTerminalKeyboardProtocol { .. })
            // ANSI frames and kitty graphics for the TUI's `TerminalAnsi`
            // renderer; this client asks for semantic surfaces instead.
            | ClientEvent::Message(ServerMessage::Terminal(_))
            | ClientEvent::Message(ServerMessage::Graphics { .. })
            | ClientEvent::Message(ServerMessage::GraphicsTransmissionRetired { .. })
            // Never read: a daemon-named path is untrusted (see AGENTS.md).
            | ClientEvent::Message(ServerMessage::GraphicsFile { .. })
            // Legacy terminal handshake and binary snapshot; generation-1
            // endpoints use the `endpoint.*` controls, which the session
            // handles before anything reaches this state.
            | ClientEvent::Message(ServerMessage::Welcome { .. })
            | ClientEvent::Message(ServerMessage::ClientShellSnapshot(_))
            // Consumed by the session worker and never forwarded here.
            | ClientEvent::Message(ServerMessage::ServerShutdown { .. })
            | ClientEvent::Message(ServerMessage::PaneSurface(_))
            | ClientEvent::Message(ServerMessage::PaneSurfacePatch(_))
            | ClientEvent::Message(ServerMessage::ClientShellEndpointResponseChunk { .. })
            | ClientEvent::Message(ServerMessage::EndpointControl { .. }) => return,
        }
        // Focus is evidence for completing one navigation, not a permanent
        // constraint on later server-driven focus changes. Settle in the inbox
        // reducer so coalesced updates cannot miss the successful transition.
        if self.surface_ready()
            && let Some(activation) = &mut self.activation
        {
            activation.focus = None;
        }
        self.dirty = true;
    }

    pub(crate) fn cancel_sounds(&mut self) {
        self.sound_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        self.sound_events.clear();
        self.reload_sound = false;
    }
}

/// Longest window title shown, in characters, as Herdr caps its own.
pub(crate) const MAX_WINDOW_TITLE_CHARS: usize = 200;

/// Daemon titles are untrusted pane-influenced text: drop control characters
/// (escape, BEL, C1 terminators included) and cap the length, as Herdr's
/// `sanitize_window_title_text` does. A blank result means no title.
fn sanitize_window_title(title: &str) -> Option<String> {
    let title = title
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_WINDOW_TITLE_CHARS)
        .collect::<String>();
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

fn coherent(snapshot: &ClientShellSnapshot, surface: &PaneSurfaceFrame) -> bool {
    snapshot.boot_id == surface.boot_id && snapshot.revision == surface.projection_revision
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use herdr_client::protocol::AgentStatus;
    use herdr_client::protocol::FrameData;

    #[test]
    fn the_single_workspace_move_follows_each_welcome() {
        let welcome = |methods: &[&str]| {
            ClientEvent::Connected(herdr_client::protocol::endpoint::EndpointServerWelcome {
                generation: 1,
                server_version: "test".into(),
                snapshot_codec: String::new(),
                surface_codec: String::new(),
                input_codec: String::new(),
                blob_codec: String::new(),
                methods: methods.iter().map(|method| (*method).into()).collect(),
                capabilities: Vec::new(),
                error: None,
            })
        };
        let mut state = LiveState::default();
        state.apply(welcome(&["workspace.move_block", "workspace.move"]));
        assert!(state.supports_workspace_move);
        // A reconnect to a daemon without it forgets the previous answer.
        state.apply(welcome(&["workspace.move_block"]));
        assert!(!state.supports_workspace_move);
    }

    #[test]
    fn window_titles_are_sanitized_capped_and_cleared() {
        let mut state = LiveState {
            status: ConnectionStatus::Connected,
            ..LiveState::default()
        };
        let title = |title: &str| {
            ClientEvent::Message(ServerMessage::WindowTitle {
                title: Some(title.into()),
            })
        };
        state.apply(title("  agent\u{1b}]2;evil\u{7}\u{9c}\r\n done  "));
        assert_eq!(state.window_title.as_deref(), Some("agent]2;evil done"));

        state.apply(title(&"é".repeat(MAX_WINDOW_TITLE_CHARS + 50)));
        assert_eq!(
            state.window_title.as_ref().map(|t| t.chars().count()),
            Some(MAX_WINDOW_TITLE_CHARS)
        );

        // Nothing printable left is no title, as is an explicit clear.
        state.apply(title("\u{1b}\u{7} \t"));
        assert_eq!(state.window_title, None);
        state.apply(title("x"));
        state.apply(ClientEvent::Message(ServerMessage::WindowTitle {
            title: None,
        }));
        assert_eq!(state.window_title, None);

        // An unchanged title does not wake the window.
        state.apply(title("same"));
        state.dirty = false;
        state.apply(title("same"));
        assert!(!state.dirty);
    }

    #[test]
    fn bells_count_only_while_connected_and_reset_with_the_connection() {
        let bell = |count| ClientEvent::Message(ServerMessage::TerminalBell { count });
        let title = || {
            ClientEvent::Message(ServerMessage::WindowTitle {
                title: Some("t".into()),
            })
        };
        let mut state = LiveState::default();
        state.apply(bell(1));
        assert_eq!(state.bells, 0, "no bells before the connection is up");

        state.status = ConnectionStatus::Connected;
        state.apply(bell(0));
        assert_eq!(state.bells, 0);
        state.apply(bell(u16::MAX));
        state.apply(bell(2));
        assert_eq!(state.bells, u16::MAX, "a burst saturates");
        assert!(!state.only_surface_changed(&state.clone()));

        state.apply(title());
        state.apply(ClientEvent::Disconnected {
            reason: "gone".into(),
        });
        assert_eq!((state.bells, state.window_title.as_deref()), (0, None));

        // A restarted daemon's first snapshot drops the old daemon's title.
        state.apply(ClientEvent::Snapshot(snapshot()));
        state.apply(title());
        state.apply(bell(1));
        let mut restarted = (*snapshot()).clone();
        restarted.boot_id = "restarted".into();
        state.apply(ClientEvent::Snapshot(Arc::new(restarted)));
        assert_eq!((state.bells, state.window_title.as_deref()), (0, None));
    }

    #[test]
    fn worktree_failure_stays_in_dialog_after_snapshots_and_successful_retry() {
        let mut state = LiveState {
            status: ConnectionStatus::Connected,
            dialog_response: Some(("create".into(), None)),
            ..LiveState::default()
        };
        let response = serde_json::json!({"error": {
            "code": "worktree_create_failed",
            "message": "fatal: 'config reload' is not a valid branch name"
        }});
        state.apply(ClientEvent::Response {
            request_id: "create".into(),
            response: response.clone(),
        });
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert_eq!(state.status_text(None), "Connected");
        assert!(matches!(&state.dialog_response,
            Some((id, Some(Ok(value)))) if id == "create" && value == &response));

        state.dialog_response = Some(("retry".into(), None));
        state.apply(ClientEvent::Response {
            request_id: "retry".into(),
            response: serde_json::json!({"result": {}}),
        });
        state.dialog_response = None;
        assert_eq!(state.status_text(None), "Connected");
    }

    /// Herdr's `last_pane` returns to the pane focused before this one,
    /// across tabs and workspaces, but never across a daemon reboot.
    #[test]
    fn previous_pane_follows_focus_within_one_boot() {
        let focus = |pane: Option<&str>, boot: &str| {
            let mut next = (*snapshot()).clone();
            next.focused_pane_id = pane.map(str::to_owned);
            next.boot_id = boot.into();
            ClientEvent::Snapshot(Arc::new(next))
        };
        let mut state = LiveState::default();
        state.apply(focus(Some("a"), "boot"));
        assert_eq!(state.previous_pane, None);
        state.apply(focus(Some("a"), "boot"));
        assert_eq!(state.previous_pane, None);
        state.apply(focus(Some("b"), "boot"));
        assert_eq!(state.previous_pane.as_deref(), Some("a"));
        // A snapshot that changes something else keeps the last pane.
        state.apply(focus(Some("b"), "boot"));
        assert_eq!(state.previous_pane.as_deref(), Some("a"));
        // Losing focus remembers the pane that had it.
        state.apply(focus(None, "boot"));
        assert_eq!(state.previous_pane.as_deref(), Some("b"));
        state.apply(focus(Some("c"), "reboot"));
        assert_eq!(state.previous_pane, None);
        state.apply(focus(Some("d"), "reboot"));
        state.apply(ClientEvent::Disconnected {
            reason: "gone".into(),
        });
        state.apply(focus(Some("e"), "reboot"));
        assert_eq!(state.previous_pane, None);
    }

    #[test]
    fn rejected_dialog_command_preserves_connection_diagnostic() {
        let mut state = LiveState {
            status: ConnectionStatus::Disconnected,
            error: Some("socket closed".into()),
            dialog_response: Some(("create".into(), None)),
            ..LiveState::default()
        };
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("create".into()),
            reason: herdr_client::Error::Disconnected,
        });
        assert_eq!(state.status_text(None), "Disconnected: socket closed");
        assert!(matches!(&state.dialog_response, Some((_, Some(Err(_))))));
    }

    #[test]
    fn untracked_daemon_errors_remain_visible_as_readable_messages() {
        let mut state = LiveState {
            status: ConnectionStatus::Connected,
            ..LiveState::default()
        };
        for (payload, expected) in [
            (
                serde_json::json!({"code": "failed", "message": "Input failed"}),
                "Input failed",
            ),
            (serde_json::json!("Legacy failure"), "Legacy failure"),
            (serde_json::json!({"code": "unsupported"}), "unsupported"),
            (
                serde_json::json!({"unexpected": true}),
                "Invalid daemon error",
            ),
        ] {
            state.apply(ClientEvent::Response {
                request_id: "input".into(),
                response: serde_json::json!({"error": payload}),
            });
            assert_eq!(state.status_text(None), format!("Connected: {expected}"));
        }
    }

    #[test]
    fn drag_request_clears_only_on_its_own_answer() {
        let mut state = LiveState {
            drag_request: Some("scroll".into()),
            ..LiveState::default()
        };
        state.apply(ClientEvent::Response {
            request_id: "other".into(),
            response: serde_json::json!({"result": {}}),
        });
        state.apply(ClientEvent::CommandRejected {
            request_id: None,
            reason: herdr_client::Error::Disconnected,
        });
        assert_eq!(state.drag_request.as_deref(), Some("scroll"));
        state.apply(ClientEvent::Response {
            request_id: "scroll".into(),
            response: serde_json::json!({"result": {}}),
        });
        assert_eq!(state.drag_request, None);

        state.drag_request = Some("scroll".into());
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("scroll".into()),
            reason: herdr_client::Error::CommandBoot,
        });
        assert_eq!(state.drag_request, None);
    }

    #[test]
    fn dialog_response_is_correlated_and_survives_coalescing() {
        let mut state = LiveState {
            dialog_response: Some(("remove".into(), None)),
            ..LiveState::default()
        };
        let response = serde_json::json!({"error":{"code":"dirty_worktree_requires_force", "message":"dirty"}});
        state.apply(ClientEvent::Response {
            request_id: "remove".into(),
            response: response.clone(),
        });
        state.apply(ClientEvent::Response {
            request_id: "other".into(),
            response: serde_json::json!({"result":{}}),
        });
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert!(
            matches!(&state.dialog_response, Some((id, Some(Ok(value)))) if id == "remove" && value == &response)
        );
        state.dialog_response = Some(("next".into(), None));
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("other".into()),
            reason: herdr_client::Error::Disconnected,
        });
        assert!(matches!(&state.dialog_response, Some((id, None)) if id == "next"));
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("next".into()),
            reason: herdr_client::Error::CommandBoot,
        });
        assert!(
            matches!(&state.dialog_response, Some((id, Some(Err(error)))) if id == "next" && matches!(error.as_ref(), crate::Error::Client(herdr_client::Error::CommandBoot)))
        );
    }

    #[test]
    fn rename_failures_stay_typed_and_shared_across_mailbox_clones() {
        let mut state = LiveState {
            tab_rename: Some(RenameResult {
                request: "rename".into(),
                result: None,
            }),
            ..LiveState::default()
        };
        let payload = serde_json::json!({"code": "invalid_label", "message": "Invalid label"});
        state.apply(ClientEvent::Response {
            request_id: "other".into(),
            response: serde_json::json!({"error": payload}),
        });
        assert!(state.tab_rename.as_ref().unwrap().result.is_none());
        assert_eq!(state.error.take().as_deref(), Some("Invalid label"));
        state.apply(ClientEvent::Response {
            request_id: "rename".into(),
            response: serde_json::json!({"error": payload}),
        });
        let cloned = state.clone();
        let error = state
            .tab_rename
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err();
        let shared = cloned
            .tab_rename
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err();
        assert!(Arc::ptr_eq(error, shared));
        assert!(matches!(error.as_ref(), crate::Error::DaemonResponse(value) if value == &payload));
        assert_eq!(error.to_string(), "Invalid label");
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("rename".into()),
            reason: herdr_client::Error::CommandBoot,
        });
        assert!(matches!(
            state
                .tab_rename
                .as_ref()
                .unwrap()
                .result
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap_err()
                .as_ref(),
            crate::Error::Client(herdr_client::Error::CommandBoot)
        ));
        assert!(state.error.is_none());
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("rename".into()),
            reason: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied").into(),
        });
        let cloned = state.clone();
        let error = state
            .tab_rename
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err();
        let shared = cloned
            .tab_rename
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err();
        assert!(Arc::ptr_eq(error, shared));
        use std::error::Error as _;
        assert!(
            error
                .source()
                .and_then(|source| source.source())
                .and_then(|source| source.downcast_ref::<std::io::Error>())
                .is_some_and(|source| source.kind() == std::io::ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn missing_installation_survives_disconnect_but_clears_on_success() {
        let mut state = LiveState::default();
        assert!(!state.missing_installation);
        state.missing_installation = true;
        state.apply(ClientEvent::Disconnected {
            reason: "Herdr not found".into(),
        });
        assert!(state.missing_installation);
        state.set_outer_focus(true);
        assert!(state.missing_installation);
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert!(!state.missing_installation);
    }

    #[test]
    fn daemon_loader_stops_on_success_or_failure() {
        let mut state = LiveState::default();
        assert_eq!(state.status, ConnectionStatus::Connecting);
        state.dirty = false;
        state.daemon_starting();
        assert!(state.dirty);
        assert_eq!(state.status, ConnectionStatus::StartingDaemon);
        assert!(!state.status.is_connected());
        assert_eq!(
            state.status_text(Some("old input error")),
            "Starting Herdr server..."
        );
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert_eq!(state.status, ConnectionStatus::Connected);

        state.daemon_starting();
        state.apply(ClientEvent::Disconnected {
            reason: "startup failed".into(),
        });
        assert_eq!(state.status, ConnectionStatus::Disconnected);
        assert_eq!(state.error.as_deref(), Some("startup failed"));
    }

    #[test]
    fn connection_status_and_error_priority_follow_lifecycle() {
        let mut state = LiveState::default();
        assert_eq!(state.status, ConnectionStatus::Connecting);
        assert!(!state.status.is_connected());
        assert_eq!(state.status_text(Some("old input error")), "Connecting...");
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert!(state.status.is_connected());
        assert_eq!(
            state.status_text(Some("input error")),
            "Connected: input error"
        );
        state.apply(ClientEvent::Disconnected {
            reason: "socket closed".into(),
        });
        assert!(!state.status.is_connected());
        assert_eq!(
            state.status_text(Some("old input error")),
            "Disconnected: socket closed"
        );
        state.status = ConnectionStatus::Detached;
        state.error = None;
        assert!(!state.status.is_connected());
        assert_eq!(
            state.status_text(Some("old input error")),
            "Detached (daemon still running)"
        );
        assert!(ConnectionStatus::AwaitingSnapshot.is_connected());
        assert_eq!(
            ConnectionStatus::AwaitingSnapshot.to_string(),
            "Connected; waiting for snapshot"
        );
    }

    /// The daemon aggregates a workspace's and tab's status itself, so a
    /// snapshot carries the same value on all three rows.
    fn agent_snapshot(status: AgentStatus, sequence: u64) -> Arc<ClientShellSnapshot> {
        let mut snapshot = snapshot();
        let next = Arc::make_mut(&mut snapshot);
        next.agents[0].agent_status = status;
        next.agents[0].state_change_seq = sequence;
        next.tabs[0].agent_status = status;
        next.workspaces[0].agent_status = status;
        next.revision = sequence;
        snapshot
    }

    fn agent_surface(snapshot: &ClientShellSnapshot) -> Arc<PaneSurfaceFrame> {
        let mut frame = surface(snapshot);
        Arc::make_mut(&mut frame).panes.push(
            serde_json::from_value(serde_json::json!({
                "pane_id": snapshot.agents[0].pane_id,
                "content_revision": 1,
                "rect": {"x": 0, "y": 0, "width": 1, "height": 1},
                "inner_rect": {"x": 0, "y": 0, "width": 1, "height": 1},
                "focused": true, "mouse_reporting": false, "sgr_pixel_mouse": false,
                "alternate_screen_active": false, "pixel_width": 0, "pixel_height": 0
            }))
            .unwrap(),
        );
        frame
    }

    fn assert_status(state: &LiveState, status: AgentStatus) {
        let snapshot = state.snapshot.as_ref().unwrap();
        assert_eq!(snapshot.agents[0].agent_status, status);
        assert_eq!(snapshot.tabs[0].agent_status, status);
        assert_eq!(snapshot.workspaces[0].agent_status, status);
    }

    #[test]
    fn daemon_status_reaches_the_sidebar_unchanged() {
        // Every client shows the same dot: painting a focused surface, changing
        // boot, or reconnecting must not rewrite what the daemon reported.
        let mut state = LiveState::default();
        state.set_outer_focus(true);
        for status in [
            AgentStatus::Working,
            AgentStatus::Done,
            AgentStatus::Idle,
            AgentStatus::Blocked,
        ] {
            let snapshot = agent_snapshot(status, 10);
            state.apply(ClientEvent::Snapshot(snapshot.clone()));
            state.apply(ClientEvent::Surface(agent_surface(&snapshot)));
            assert_status(&state, status);
        }
        let mut reboot = agent_snapshot(AgentStatus::Done, 100);
        Arc::make_mut(&mut reboot).boot_id = "new-boot".into();
        state.apply(ClientEvent::Snapshot(reboot));
        assert_status(&state, AgentStatus::Done);
        state.apply(ClientEvent::Disconnected {
            reason: "test".into(),
        });
        state.apply(ClientEvent::Snapshot(agent_snapshot(
            AgentStatus::Done,
            200,
        )));
        assert_status(&state, AgentStatus::Done);
    }

    #[test]
    fn surface_images_follow_the_connection_and_file_paths_stay_ignored() {
        use herdr_client::{
            SurfaceImages,
            protocol::{
                SurfaceGraphicsAsset, SurfaceGraphicsAssetKey, SurfaceGraphicsFormat,
                SurfaceGraphicsSource, SurfaceGraphicsTarget,
            },
        };
        let key = SurfaceGraphicsAssetKey {
            source: SurfaceGraphicsSource::Terminal {
                target: SurfaceGraphicsTarget::Pane {
                    pane_id: "p1".into(),
                },
                image_id: 1,
            },
            image_width: 1,
            image_height: 1,
            format: SurfaceGraphicsFormat::Rgba,
            data_len: 4,
            data_fingerprint: 1,
        };
        let images: SurfaceImages = [SurfaceGraphicsAsset {
            key: key.clone(),
            data: vec![0; 4],
        }]
        .into_iter()
        .collect();
        let mut state = LiveState::default();
        state.apply(ClientEvent::SurfaceImages(Arc::new(images)));
        assert!(state.surface_images.get(&key).is_some());
        // A daemon naming a file for this client is never followed.
        let before = state.surface_images.clone();
        state.apply(ClientEvent::Message(ServerMessage::GraphicsFile {
            path: "/etc/passwd".into(),
            expected_len: 4,
            image_id: 1,
            transfer_id: 1,
            leading: vec![],
            control: String::new(),
            surface_asset: Some(key.clone()),
        }));
        assert!(Arc::ptr_eq(&before, &state.surface_images));
        state.apply(ClientEvent::Disconnected {
            reason: "test".into(),
        });
        assert!(state.surface_images.is_empty());
    }

    #[test]
    fn agent_view_projection_is_a_query_not_an_activity_override() {
        // This is the endpoint.agent-view.v1 envelope and AgentViewSetParams
        // shape from upstream, not a per-agent status payload.
        let mut state = LiveState::default();
        state.apply(ClientEvent::Snapshot(agent_snapshot(AgentStatus::Idle, 7)));
        state.apply(ClientEvent::Message(ServerMessage::EndpointControl {
            kind: "endpoint.agent-view.v1".into(),
            data: include_str!("../../herdr-protocol/tests/fixtures/endpoint-agent-view-v1.json")
                .into(),
        }));
        assert_status(&state, AgentStatus::Idle);
    }

    fn snapshot() -> Arc<ClientShellSnapshot> {
        Arc::new(
            serde_json::from_str(include_str!(
                "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
            ))
            .unwrap(),
        )
    }

    fn activating(snapshot: &ClientShellSnapshot) -> SurfaceActivation {
        SurfaceActivation {
            request: "activate-1".into(),
            boot: snapshot.boot_id.clone(),
            revision: None,
            failed: false,
            focus: None,
            active: true,
        }
    }

    #[test]
    fn completed_navigation_retires_focus_in_inbox_before_later_focus_changes() {
        for kind in ["pane", "tab", "workspace"] {
            for ack_first in [false, true] {
                let snapshot = snapshot();
                let id = match kind {
                    "workspace" => snapshot.focused_workspace_id.clone().unwrap(),
                    "tab" => snapshot.focused_tab_id.clone().unwrap(),
                    _ => snapshot.focused_pane_id.clone().unwrap(),
                };
                let mut state = LiveState::default();
                state.apply(ClientEvent::Snapshot(snapshot.clone()));
                state.activation = Some(SurfaceActivation {
                    focus: Some(match kind {
                        "workspace" => crate::NavigationTarget::Workspace(id),
                        "tab" => crate::NavigationTarget::Tab(id),
                        _ => crate::NavigationTarget::Pane(id),
                    }),
                    ..activating(&snapshot)
                });
                let ack = ClientEvent::Response {
                    request_id: "activate-1".into(),
                    response: serde_json::json!({"result": {
                        "type": "client_shell_surface_set", "active": true,
                        "projection_revision": snapshot.revision
                    }}),
                };
                let frame = ClientEvent::Surface(surface(&snapshot));
                let events = if ack_first {
                    [ack, frame]
                } else {
                    [frame, ack]
                };
                for (index, event) in events.into_iter().enumerate() {
                    state.apply(event);
                    assert_eq!(state.surface_ready(), index == 1);
                    assert_eq!(
                        state.activation.as_ref().unwrap().focus.is_none(),
                        index == 1
                    );
                }
                // No UI poll between completion and the split/tab/workspace's
                // next projection: the authoritative reducer must already settle.
                let mut next = snapshot.clone();
                let next_snapshot = Arc::make_mut(&mut next);
                next_snapshot.revision += 1;
                next_snapshot.focused_pane_id = Some("split-pane".into());
                next_snapshot.focused_tab_id = Some("created-tab".into());
                next_snapshot.focused_workspace_id = Some("created-workspace".into());
                state.apply(ClientEvent::Snapshot(next.clone()));
                assert!(!state.surface_ready());
                state.apply(ClientEvent::Surface(surface(&next)));
                assert!(state.surface_ready());
                let settled = state.activation.as_ref().unwrap();
                assert_eq!(settled.revision, Some(snapshot.revision));
                assert_eq!(settled.boot, snapshot.boot_id);
                assert!(settled.active && !settled.failed);
            }
        }
    }

    #[test]
    fn activation_requires_matching_ack_coherent_revision_and_focus() {
        let snapshot = snapshot();
        let mut state = LiveState::default();
        state.apply(ClientEvent::Snapshot(snapshot.clone()));
        state.activation = Some(activating(&snapshot));
        state.apply(ClientEvent::Surface(surface(&snapshot)));
        assert!(!state.surface_ready());
        let response = serde_json::json!({"result": {
            "type": "client_shell_surface_set", "active": true, "projection_revision": snapshot.revision
        }});
        state.apply(ClientEvent::Response {
            request_id: "stale".into(),
            response: response.clone(),
        });
        assert!(!state.surface_ready());
        state.apply(ClientEvent::Response {
            request_id: "activate-1".into(),
            response,
        });
        assert!(state.surface_ready());
        state.activation.as_mut().unwrap().focus =
            Some(crate::NavigationTarget::Pane("wrong-pane".into()));
        assert!(!state.surface_ready());
        state.activation.as_mut().unwrap().focus = None;
        state.activation.as_mut().unwrap().revision = Some(snapshot.revision + 1);
        assert!(!state.surface_ready());
        let mut reboot = snapshot.clone();
        Arc::make_mut(&mut reboot).boot_id = "reboot".into();
        state.apply(ClientEvent::Snapshot(reboot.clone()));
        state.apply(ClientEvent::Surface(surface(&reboot)));
        assert!(state.activation.as_ref().unwrap().failed);
        assert!(!state.surface_ready());
    }

    #[test]
    fn rejected_malformed_and_wrong_direction_acks_never_enable_input() {
        let snapshot = snapshot();
        for response in [
            serde_json::json!({"error": {"message": "unsupported"}}),
            serde_json::json!({"result": {"type": "other", "active": true, "projection_revision": 0}}),
            serde_json::json!({"result": {"type": "client_shell_surface_set", "active": false, "projection_revision": 0}}),
        ] {
            let mut state = LiveState::default();
            state.apply(ClientEvent::Snapshot(snapshot.clone()));
            state.apply(ClientEvent::Surface(surface(&snapshot)));
            state.activation = Some(activating(&snapshot));
            state.apply(ClientEvent::Response {
                request_id: "activate-1".into(),
                response,
            });
            assert!(state.activation.as_ref().unwrap().failed);
            assert!(!state.surface_ready());
        }
        let mut state = LiveState {
            activation: Some(activating(&snapshot)),
            ..Default::default()
        };
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("activate-1".into()),
            reason: herdr_client::Error::UnsupportedMethod,
        });
        assert!(state.activation.as_ref().unwrap().failed);
    }

    fn surface(snapshot: &ClientShellSnapshot) -> Arc<PaneSurfaceFrame> {
        Arc::new(PaneSurfaceFrame {
            boot_id: snapshot.boot_id.clone(),
            projection_revision: snapshot.revision,
            surface_revision: 1,
            frame: FrameData {
                cells: vec![],
                width: 0,
                height: 0,
                cursor: None,
                hyperlinks: vec![],
                graphics: vec![],
            },
            panes: vec![],
            splits: vec![],
            popup: None,
            graphics: Default::default(),
        })
    }

    #[test]
    fn snapshot_change_invalidates_cells_until_matching_surface() {
        let mut state = LiveState::default();
        let mut snapshot = snapshot();
        let old = surface(&snapshot);
        state.apply(ClientEvent::Snapshot(snapshot.clone()));
        state.apply(ClientEvent::Surface(old.clone()));
        assert!(state.surface.is_some());
        Arc::make_mut(&mut snapshot).revision += 1;
        state.apply(ClientEvent::Snapshot(snapshot.clone()));
        assert!(state.surface.is_none());
        state.apply(ClientEvent::Surface(old));
        assert!(state.surface.is_none());
        state.apply(ClientEvent::Surface(surface(&snapshot)));
        assert!(state.surface.is_some());
    }

    #[test]
    fn surface_before_snapshot_is_not_retained_or_replayed() {
        let mut state = LiveState::default();
        let snapshot = snapshot();
        let frame = surface(&snapshot);
        state.apply(ClientEvent::Surface(frame.clone()));
        assert!(state.surface.is_none());
        state.apply(ClientEvent::Snapshot(snapshot));
        assert!(state.surface.is_none());
        state.apply(ClientEvent::Surface(frame.clone()));
        assert!(Arc::ptr_eq(state.surface.as_ref().unwrap(), &frame));

        state.apply(ClientEvent::Disconnected {
            reason: "closed".into(),
        });
        state.apply(ClientEvent::Surface(frame));
        assert!(state.snapshot.is_none());
        assert!(state.surface.is_none());
        assert_eq!(state.status, ConnectionStatus::Disconnected);
        assert_eq!(state.error.as_deref(), Some("closed"));
    }

    #[test]
    fn different_boot_and_disconnect_cannot_retain_old_cells() {
        let mut state = LiveState::default();
        let snapshot = snapshot();
        let mut wrong_boot = surface(&snapshot);
        Arc::make_mut(&mut wrong_boot).boot_id = "old-boot".into();
        state.apply(ClientEvent::Snapshot(snapshot.clone()));
        state.apply(ClientEvent::Surface(wrong_boot));
        assert!(state.surface.is_none());
        state.apply(ClientEvent::Surface(surface(&snapshot)));
        state.apply(ClientEvent::Disconnected {
            reason: "closed".into(),
        });
        assert!(state.snapshot.is_none() && state.surface.is_none());
        assert!(!state.status.is_connected());
        assert_eq!(state.error.as_deref(), Some("closed"));
    }

    #[test]
    fn only_a_new_surface_spares_the_chrome() {
        let snapshot = snapshot();
        let mut old = LiveState::default();
        old.apply(ClientEvent::Snapshot(snapshot.clone()));
        old.apply(ClientEvent::Surface(surface(&snapshot)));
        // The mailbox hands the window clones: shared snapshot, new surface.
        let mut next = old.clone();
        next.apply(ClientEvent::Surface(Arc::new(PaneSurfaceFrame {
            surface_revision: 2,
            ..(*surface(&snapshot)).clone()
        })));
        assert!(old.only_surface_changed(&next));
        assert!(old.only_surface_changed(&old.clone()));

        type Change = (&'static str, fn(&mut LiveState));
        let changes: [Change; 10] = [
            ("snapshot", |s| {
                s.snapshot = s.snapshot.as_deref().cloned().map(Arc::new);
            }),
            ("status", |s| s.status = ConnectionStatus::Disconnected),
            ("error", |s| s.error = Some("lost".into())),
            ("notification lost", |s| s.notifications_lost = true),
            ("sound", |s| s.reload_sound = true),
            ("clipboard write", |s| {
                s.clipboard_writes.push_back("x".into())
            }),
            ("dialog answer", |s| {
                s.dialog_response = Some(("remove".into(), Some(Ok(serde_json::Value::Null))));
            }),
            ("rename answer", |s| {
                s.pane_rename = Some(RenameResult {
                    request: "rename".into(),
                    result: Some(Ok(())),
                });
            }),
            ("drag answer", |s| s.drag_request = Some("scroll".into())),
            ("outer focus", |s| s.outer_focused = Some(true)),
        ];
        for (what, change) in changes {
            let mut changed = next.clone();
            change(&mut changed);
            assert!(!old.only_surface_changed(&changed), "{what}");
        }
    }

    #[test]
    fn keyboard_report_all_follows_the_daemon_and_ends_with_the_connection() {
        let report =
            |enabled| ClientEvent::Message(ServerMessage::ClientShellKeyboardReportAll { enabled });
        let mut state = LiveState::default();
        state.apply(ClientEvent::Snapshot(snapshot()));
        state.dirty = false;
        state.apply(report(true));
        assert!(state.keyboard_report_all && state.dirty);
        // A repeated report changes nothing to draw.
        state.dirty = false;
        state.apply(report(true));
        assert!(!state.dirty);
        // It only shapes later key events, so the chrome is spared.
        let mut next = state.clone();
        next.apply(report(false));
        assert!(!next.keyboard_report_all);
        assert!(state.only_surface_changed(&next));
        state.apply(ClientEvent::Disconnected {
            reason: "gone".into(),
        });
        assert!(!state.keyboard_report_all);
    }

    #[test]
    fn messages_meant_for_the_tui_change_nothing() {
        let mut state = LiveState::default();
        state.apply(ClientEvent::Snapshot(snapshot()));
        let before = state.clone();
        state.dirty = false;
        for message in [
            // Shell clients are notified through SemanticNotification.
            ServerMessage::Notify {
                kind: herdr_client::protocol::NotifyKind::Toast,
                message: "Agent finished".into(),
                body: None,
            },
            // Each surface pane carries its own mouse_reporting.
            ServerMessage::MouseCapture {
                enabled: true,
                sgr_pixels: true,
            },
            ServerMessage::DirectTerminalKeyboardProtocol {
                flags: 31,
                modify_other_keys_level: 2,
            },
        ] {
            state.apply(ClientEvent::Message(message));
        }
        assert!(!state.dirty);
        assert!(before.only_surface_changed(&state));
        assert!(state.notifications.is_empty() && state.sound_events.is_empty());
        assert!(!state.keyboard_report_all);
    }

    #[test]
    fn daemon_clipboard_payloads_are_decoded_bounded_and_dropped_when_invalid() {
        let mut state = LiveState::default();
        state.apply(ClientEvent::Message(ServerMessage::Clipboard {
            data: "aGVsbG8=".into(),
        }));
        state.apply(ClientEvent::Message(ServerMessage::Clipboard {
            data: "not base64!".into(),
        }));
        assert_eq!(
            state
                .clipboard_writes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["hello"]
        );
        // A pane that spams OSC 52 cannot grow the mailbox without bound, and
        // only the newest writes survive: the seeded "hello" is evicted first.
        for index in 0..crate::osc52::MAX_PENDING * 2 {
            state.apply(ClientEvent::Message(ServerMessage::Clipboard {
                data: "eA==".into(),
            }));
            assert_eq!(
                state.clipboard_writes.len(),
                (index + 2).min(crate::osc52::MAX_PENDING)
            );
        }
        assert!(state.clipboard_writes.iter().all(|text| text == "x"));
    }

    fn dismissed(request: &str) -> LiveState {
        let mut state = LiveState::default();
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert_eq!(
            state.product_announcement().map(|a| a.id.as_str()),
            Some("announcement-v1")
        );
        state.announcement_dismissal = Some(AnnouncementDismissal {
            request: Some(request.into()),
            version: "1.0.0".into(),
            id: "announcement-v1".into(),
        });
        assert!(state.product_announcement().is_none());
        state
    }

    #[test]
    fn dismissed_announcement_stays_hidden_until_the_daemon_drops_it() {
        let mut state = dismissed("dismiss-1");
        state.apply(ClientEvent::Response {
            request_id: "dismiss-1".into(),
            response: serde_json::json!({"result": {"type": "ok"}}),
        });
        // A snapshot that still carries it, sent before the daemon handled
        // the request, must not bring the card back.
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert!(state.product_announcement().is_none());
        assert!(state.error.is_none());

        let mut dropped = (*snapshot()).clone();
        dropped.product_announcement = None;
        state.apply(ClientEvent::Snapshot(Arc::new(dropped)));
        assert!(state.announcement_dismissal.is_none());

        // A new announcement is never covered by an old dismissal.
        let mut next = (*snapshot()).clone();
        if let Some(announcement) = &mut next.product_announcement {
            announcement.id = "announcement-v2".into();
        }
        let mut state = dismissed("dismiss-2");
        state.apply(ClientEvent::Snapshot(Arc::new(next)));
        assert_eq!(
            state.product_announcement().map(|a| a.id.as_str()),
            Some("announcement-v2")
        );
    }

    #[test]
    fn rejected_dismissal_shows_the_announcement_again() {
        let mut state = dismissed("dismiss-1");
        state.apply(ClientEvent::Response {
            request_id: "other".into(),
            response: serde_json::json!({"error": {"code": "x", "message": "y"}}),
        });
        assert!(state.product_announcement().is_none());
        state.error = None;
        state.apply(ClientEvent::Response {
            request_id: "dismiss-1".into(),
            response: serde_json::json!({"error": {
                "code": "stale_announcement",
                "message": "the product announcement is no longer current"
            }}),
        });
        assert!(state.product_announcement().is_some());
        // The card coming back is the feedback; the status line stays quiet.
        assert!(state.error.is_none());

        let mut state = dismissed("dismiss-2");
        state.apply(ClientEvent::CommandRejected {
            request_id: Some("dismiss-2".into()),
            reason: herdr_client::Error::Disconnected,
        });
        assert!(state.product_announcement().is_some());
    }

    #[test]
    fn dismissal_is_part_of_the_window_state() {
        let state = dismissed("dismiss-1");
        let mut next = state.clone();
        assert!(state.only_surface_changed(&next));
        next.announcement_dismissal = None;
        assert!(!state.only_surface_changed(&next));
    }
}
