use herdr_client::{
    ClientEvent, Method,
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
    pub(crate) sound_events: std::collections::VecDeque<(
        std::time::Instant,
        herdr_client::protocol::SemanticNotification,
    )>,
    pub(crate) reload_sound: bool,
    pub(crate) sound_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) sound_connection_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub snapshot: Option<Arc<ClientShellSnapshot>>,
    pub surface: Option<Arc<PaneSurfaceFrame>>,
    pub status: ConnectionStatus,
    pub error: Option<String>,
    pub missing_installation: bool,
    /// Same-user peer at the owned standard socket, not executable attestation.
    pub(crate) local_daemon_peer: bool,
    pub(crate) supports_workspace_get: bool,
    /// `pane.clear` arrived after Herdr 0.9.1; older daemons reject it.
    pub(crate) supports_pane_clear: bool,
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
            sound_events: Default::default(),
            reload_sound: false,
            sound_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sound_connection_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            snapshot: None,
            surface: None,
            status: ConnectionStatus::Connecting,
            error: None,
            missing_installation: false,
            local_daemon_peer: false,
            supports_workspace_get: false,
            supports_pane_clear: false,
            dirty: true,
            dialog_response: None,
            notifications: Default::default(),
            notifications_lost: false,
            outer_focused: None,
            activation: None,
            supports_surface: false,
            tab_rename: None,
            pane_rename: None,
            drag_request: None,
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
            sound_cancel,
            sound_connection_cancel,
            snapshot,
            surface: _,
            status,
            error,
            missing_installation,
            local_daemon_peer,
            supports_workspace_get,
            supports_pane_clear,
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
            && Arc::ptr_eq(sound_cancel, &self.sound_cancel)
            && Arc::ptr_eq(sound_connection_cancel, &self.sound_connection_cancel)
            && same_arc(snapshot, &self.snapshot)
            && *status == self.status
            && *error == self.error
            && *missing_installation == self.missing_installation
            && *local_daemon_peer == self.local_daemon_peer
            && *supports_workspace_get == self.supports_workspace_get
            && *supports_pane_clear == self.supports_pane_clear
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
    }

    fn has_operation_result(&self, request_id: &str) -> bool {
        self.dialog_response
            .as_ref()
            .is_some_and(|(id, _)| id == request_id)
            || [&self.tab_rename, &self.pane_rename]
                .into_iter()
                .flatten()
                .any(|rename| rename.request == request_id)
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
                self.supports_workspace_get = Method::WorkspaceGet.advertised_in(&welcome.methods);
                self.supports_pane_clear = Method::PaneClear.advertised_in(&welcome.methods);
                self.supports_surface = Method::ClientShellSurfaceSet
                    .advertised_in(&welcome.methods)
                    && ["surface_interest", "presentation_effects_fence"]
                        .iter()
                        .all(|capability| {
                            welcome.capabilities.iter().any(|value| value == capability)
                        });
                self.missing_installation = false;
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
                self.status = ConnectionStatus::Connected;
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
            ClientEvent::Disconnected { reason } => {
                self.notifications.clear();
                self.cancel_sounds();
                self.status = ConnectionStatus::Disconnected;
                self.error = Some(reason);
                self.snapshot = None;
                self.surface = None;
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
            ClientEvent::Message(ServerMessage::ReloadSoundConfig) => self.reload_sound = true,
            _ => return,
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
        let changes: [Change; 9] = [
            ("snapshot", |s| {
                s.snapshot = s.snapshot.as_deref().cloned().map(Arc::new);
            }),
            ("status", |s| s.status = ConnectionStatus::Disconnected),
            ("error", |s| s.error = Some("lost".into())),
            ("notification lost", |s| s.notifications_lost = true),
            ("sound", |s| s.reload_sound = true),
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
}
