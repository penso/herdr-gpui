use herdr_client::{
    ClientEvent,
    presentation::AgentPresentation,
    protocol::{ClientShellSnapshot, PaneSurfaceFrame, ServerMessage},
};
use std::sync::Arc;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestStatus {
    Queued,
    Succeeded,
    Failed,
}

#[derive(Clone)]
pub struct LiveState {
    pub snapshot: Option<Arc<ClientShellSnapshot>>,
    pub surface: Option<Arc<PaneSurfaceFrame>>,
    pub status: ConnectionStatus,
    pub error: Option<String>,
    pub missing_installation: bool,
    pub dirty: bool,
    agent_presentation: AgentPresentation,
    outer_focused: Option<bool>,
    tracked_request: Option<(String, RequestStatus)>,
    pub activation: Option<SurfaceActivation>,
    pub supports_surface: bool,
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
            snapshot: None,
            surface: None,
            status: ConnectionStatus::Connecting,
            error: None,
            missing_installation: false,
            dirty: true,
            agent_presentation: AgentPresentation::default(),
            outer_focused: None,
            tracked_request: None,
            activation: None,
            supports_surface: false,
        }
    }
}

impl LiveState {
    pub(super) fn track_request(&mut self, request_id: String) {
        self.tracked_request = Some((request_id, RequestStatus::Queued));
        self.dirty = true;
    }

    pub(super) fn request_status(&self, id: &str) -> Option<RequestStatus> {
        self.tracked_request
            .as_ref()
            .filter(|(request_id, _)| request_id == id)
            .map(|(_, status)| *status)
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

    /// Only acknowledge the coherent pair captured for an active UI paint.
    /// Reject superseded projections rather than consuming newer unseen events.
    pub fn acknowledge_presented_surface(
        &mut self,
        presented: &ClientShellSnapshot,
        surface: &PaneSurfaceFrame,
        focused: bool,
    ) -> bool {
        if self.activation.is_some() && !self.surface_ready() {
            return false;
        }
        let Some(snapshot) = self.snapshot.as_mut() else {
            return false;
        };
        if !focused
            || self.outer_focused != Some(true)
            || !coherent(presented, surface)
            || !coherent(snapshot, surface)
            || surface.panes.iter().any(|pane| {
                let sequence = |snapshot: &ClientShellSnapshot| {
                    snapshot
                        .agents
                        .iter()
                        .find(|agent| agent.pane_id == pane.pane_id)
                        .map(|agent| agent.state_change_seq)
                };
                sequence(presented) != sequence(snapshot)
            })
        {
            return false;
        }
        let changed =
            self.agent_presentation
                .acknowledge_surface(Arc::make_mut(snapshot), surface, focused);
        self.dirty |= changed;
        changed
    }

    pub fn apply(&mut self, event: ClientEvent) {
        // Record completion before the display-only branches can return early.
        if let Some((tracked_id, status)) = &mut self.tracked_request {
            let next = match &event {
                ClientEvent::Response {
                    request_id,
                    response,
                } if request_id == tracked_id => {
                    Some(if response.get("error").is_some_and(|e| !e.is_null()) {
                        RequestStatus::Failed
                    } else {
                        RequestStatus::Succeeded
                    })
                }
                ClientEvent::CommandRejected {
                    request_id: Some(request_id),
                    ..
                } if request_id == tracked_id => Some(RequestStatus::Failed),
                ClientEvent::Disconnected { .. } if *status == RequestStatus::Queued => {
                    Some(RequestStatus::Failed)
                }
                _ => None,
            };
            if let Some(next) = next {
                self.dirty |= *status != next;
                *status = next;
            }
        }
        let mut changed = true;
        match event {
            ClientEvent::Connected(welcome) => {
                self.supports_surface = welcome
                    .methods
                    .iter()
                    .any(|method| method == "client_shell.surface.set")
                    && ["surface_interest", "presentation_effects_fence"]
                        .iter()
                        .all(|capability| {
                            welcome.capabilities.iter().any(|value| value == capability)
                        });
                self.missing_installation = false;
                self.status = ConnectionStatus::AwaitingSnapshot;
                self.error = None;
            }
            ClientEvent::Snapshot(mut snapshot) => {
                if let Some(activation) = &mut self.activation
                    && activation.boot != snapshot.boot_id
                {
                    activation.failed = true;
                }
                self.missing_installation = false;
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|previous| previous.boot_id != snapshot.boot_id)
                {
                    self.tracked_request = None;
                }
                if self
                    .surface
                    .as_ref()
                    .is_some_and(|s| !coherent(&snapshot, s))
                {
                    self.surface = None;
                }
                self.status = ConnectionStatus::Connected;
                self.agent_presentation
                    .project_snapshot(Arc::make_mut(&mut snapshot));
                self.snapshot = Some(snapshot);
            }
            ClientEvent::Surface(surface) => {
                if !self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| coherent(s, &surface))
                    || self
                        .surface
                        .as_ref()
                        .is_some_and(|s| Arc::ptr_eq(s, &surface))
                {
                    return;
                }
                self.surface = Some(surface);
            }
            ClientEvent::Disconnected { reason } => {
                self.status = ConnectionStatus::Disconnected;
                self.error = Some(reason);
                self.snapshot = None;
                self.surface = None;
                self.agent_presentation = AgentPresentation::default();
            }
            ClientEvent::CommandRejected { request_id, reason } => {
                changed = self.error.as_ref() != Some(&reason);
                if let Some(activation) = &mut self.activation
                    && request_id.as_ref() == Some(&activation.request)
                {
                    changed |= !activation.failed;
                    activation.failed = true;
                }
                self.error = Some(reason);
            }
            ClientEvent::Response {
                request_id,
                response,
            } => {
                changed = false;
                let mut response_error = None;
                if let Some(activation) = &mut self.activation
                    && request_id == activation.request
                {
                    let previous = (activation.revision, activation.failed);
                    let result = &response["result"];
                    activation.revision = (response.get("error").is_none_or(|e| e.is_null())
                        && result["type"] == "client_shell_surface_set"
                        && result["active"] == activation.active)
                        .then(|| result["projection_revision"].as_u64())
                        .flatten();
                    activation.failed = activation.revision.is_none();
                    changed |= previous != (activation.revision, activation.failed);
                    if activation.failed {
                        response_error = Some("Invalid surface activation acknowledgement".into());
                    }
                }
                if let Some(error) = response.get("error")
                    && !error.is_null()
                {
                    response_error = Some(error.to_string());
                }
                if let Some(error) = response_error {
                    changed |= self.error.as_ref() != Some(&error);
                    self.error = Some(error);
                }
            }
            ClientEvent::Message(ServerMessage::ClientShellError { message }) => {
                changed = self.error.as_ref() != Some(&message);
                self.error = Some(message);
            }
            _ => return,
        }
        // Focus is evidence for completing one navigation, not a permanent
        // constraint on later server-driven focus changes. Settle in the inbox
        // reducer so coalesced updates cannot miss the successful transition.
        if self.surface_ready()
            && let Some(activation) = &mut self.activation
        {
            changed |= activation.focus.is_some();
            activation.focus = None;
        }
        self.dirty |= changed;
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

    fn agent_snapshot(status: AgentStatus, sequence: u64) -> Arc<ClientShellSnapshot> {
        let mut snapshot = snapshot();
        let next = Arc::make_mut(&mut snapshot);
        next.agents[0].agent_status = status;
        next.agents[0].state_change_seq = sequence;
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

    #[test]
    fn idle_wire_completion_is_done_until_focused_coherent_surface_is_painted() {
        let mut state = LiveState::default();
        state.set_outer_focus(false);
        state.apply(ClientEvent::Snapshot(agent_snapshot(
            AgentStatus::Working,
            9,
        )));
        assert_status(&state, AgentStatus::Working);
        let idle = agent_snapshot(AgentStatus::Idle, 10);
        state.apply(ClientEvent::Snapshot(idle.clone()));
        assert_status(&state, AgentStatus::Done);
        assert_eq!(
            idle.agents[0].agent_status,
            AgentStatus::Idle,
            "wire snapshot stays raw"
        );
        let frame = agent_surface(&idle);
        state.apply(ClientEvent::Surface(frame.clone()));
        assert_status(&state, AgentStatus::Done);
        assert!(!state.acknowledge_presented_surface(&idle, &frame, false));
        assert!(!state.acknowledge_presented_surface(&idle, &frame, true));
        state.dirty = false;
        state.set_outer_focus(true);
        assert!(
            state.dirty,
            "focus gain requests a draw, not an acknowledgement"
        );
        assert_status(&state, AgentStatus::Done);
        assert!(!state.acknowledge_presented_surface(&idle, &frame, false));
        let painted = state.snapshot.clone().unwrap();
        assert!(state.acknowledge_presented_surface(&painted, &frame, true));
        assert_status(&state, AgentStatus::Idle);
        assert_eq!(painted.agents[0].agent_status, AgentStatus::Done);
        state.apply(ClientEvent::Snapshot(idle));
        assert_status(&state, AgentStatus::Idle);
    }

    #[test]
    fn initial_idle_or_done_is_seen_and_clients_acknowledge_independently() {
        for initial_status in [AgentStatus::Idle, AgentStatus::Done] {
            let mut viewer = LiveState::default();
            viewer.set_outer_focus(true);
            viewer.apply(ClientEvent::Snapshot(agent_snapshot(initial_status, 9)));
            assert_status(&viewer, AgentStatus::Idle);
            let mut background = viewer.clone();
            let idle = agent_snapshot(AgentStatus::Idle, 10);
            viewer.apply(ClientEvent::Snapshot(idle.clone()));
            background.apply(ClientEvent::Snapshot(idle.clone()));
            let frame = agent_surface(&idle);
            viewer.apply(ClientEvent::Surface(frame.clone()));
            assert_status(&viewer, AgentStatus::Done);
            assert!(viewer.acknowledge_presented_surface(&idle, &frame, true));
            assert_status(&viewer, AgentStatus::Idle);
            assert_status(&background, AgentStatus::Done);
        }
    }

    #[test]
    fn stale_wrong_boot_and_offscreen_surfaces_do_not_consume_done() {
        let mut state = LiveState::default();
        state.set_outer_focus(true);
        let initial = agent_snapshot(AgentStatus::Working, 9);
        state.apply(ClientEvent::Snapshot(initial.clone()));
        let idle = agent_snapshot(AgentStatus::Idle, 10);
        state.apply(ClientEvent::Snapshot(idle.clone()));
        state.apply(ClientEvent::Surface(agent_surface(&initial)));
        assert!(!state.acknowledge_presented_surface(&initial, &agent_surface(&initial), true));
        assert_status(&state, AgentStatus::Done);
        let mut wrong_boot = agent_surface(&idle);
        Arc::make_mut(&mut wrong_boot).boot_id = "wrong".into();
        state.apply(ClientEvent::Surface(wrong_boot.clone()));
        assert!(!state.acknowledge_presented_surface(&idle, &wrong_boot, true));
        state.apply(ClientEvent::Surface(surface(&idle)));
        assert!(!state.acknowledge_presented_surface(&idle, &surface(&idle), true));
        assert_status(&state, AgentStatus::Done);
    }

    #[test]
    fn coalesced_receipts_and_stale_paints_cannot_acknowledge_newer_completion() {
        for same_revision in [false, true] {
            let mut state = LiveState::default();
            state.set_outer_focus(true);
            state.apply(ClientEvent::Snapshot(agent_snapshot(
                AgentStatus::Working,
                9,
            )));
            let a = agent_snapshot(AgentStatus::Idle, 10);
            let a_surface = agent_surface(&a);
            state.apply(ClientEvent::Snapshot(a));
            state.apply(ClientEvent::Surface(a_surface.clone()));
            let painted_a = state.snapshot.clone().unwrap();
            assert_status(&state, AgentStatus::Done);

            let mut b = agent_snapshot(AgentStatus::Idle, 11);
            if same_revision {
                Arc::make_mut(&mut b).revision = painted_a.revision;
            }
            let b_surface = agent_surface(&b);
            state.apply(ClientEvent::Snapshot(b.clone()));
            state.apply(ClientEvent::Surface(b_surface.clone()));
            assert_status(&state, AgentStatus::Done);
            assert!(!state.acknowledge_presented_surface(&painted_a, &a_surface, true));
            assert_status(&state, AgentStatus::Done);
            assert!(state.acknowledge_presented_surface(&b, &b_surface, true));
            assert_status(&state, AgentStatus::Idle);
        }
    }

    #[test]
    fn coherent_paint_from_previous_boot_cannot_acknowledge_current_boot() {
        let mut state = LiveState::default();
        state.set_outer_focus(true);
        let old = agent_snapshot(AgentStatus::Idle, 10);
        let old_surface = agent_surface(&old);
        state.apply(ClientEvent::Snapshot(old.clone()));
        let mut baseline = agent_snapshot(AgentStatus::Working, 9);
        Arc::make_mut(&mut baseline).boot_id = "new-boot".into();
        state.apply(ClientEvent::Snapshot(baseline));
        let mut next = agent_snapshot(AgentStatus::Idle, 10);
        Arc::make_mut(&mut next).boot_id = "new-boot".into();
        state.apply(ClientEvent::Snapshot(next));
        assert!(!state.acknowledge_presented_surface(&old, &old_surface, true));
        assert_status(&state, AgentStatus::Done);
    }

    #[test]
    fn paint_only_acknowledges_agents_in_presented_panes() {
        let mut state = LiveState::default();
        state.set_outer_focus(true);
        let mut initial = agent_snapshot(AgentStatus::Working, 9);
        let mut second = initial.agents[0].clone();
        second.pane_id = "offscreen".into();
        Arc::make_mut(&mut initial).agents.push(second);
        state.apply(ClientEvent::Snapshot(initial.clone()));
        let next = Arc::make_mut(&mut initial);
        next.revision += 1;
        for agent in &mut next.agents {
            agent.state_change_seq += 1;
            agent.agent_status = AgentStatus::Idle;
        }
        let frame = agent_surface(&initial);
        state.apply(ClientEvent::Snapshot(initial.clone()));
        state.apply(ClientEvent::Surface(frame.clone()));
        assert!(state.acknowledge_presented_surface(&initial, &frame, true));
        let snapshot = state.snapshot.as_ref().unwrap();
        assert_eq!(snapshot.agents[0].agent_status, AgentStatus::Idle);
        assert_eq!(snapshot.agents[1].agent_status, AgentStatus::Done);
        assert_eq!(snapshot.tabs[0].agent_status, AgentStatus::Done);
        assert_eq!(snapshot.workspaces[0].agent_status, AgentStatus::Done);
    }

    #[test]
    fn boot_change_and_disconnect_establish_a_new_seen_baseline() {
        let mut state = LiveState::default();
        state.apply(ClientEvent::Snapshot(agent_snapshot(
            AgentStatus::Working,
            9,
        )));
        state.apply(ClientEvent::Snapshot(agent_snapshot(AgentStatus::Idle, 10)));
        assert_status(&state, AgentStatus::Done);
        let mut reboot = agent_snapshot(AgentStatus::Idle, 100);
        Arc::make_mut(&mut reboot).boot_id = "new-boot".into();
        state.apply(ClientEvent::Snapshot(reboot));
        assert_status(&state, AgentStatus::Idle);
        state.apply(ClientEvent::Disconnected {
            reason: "test".into(),
        });
        state.apply(ClientEvent::Snapshot(agent_snapshot(
            AgentStatus::Done,
            200,
        )));
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
    fn deferred_paint_cannot_acknowledge_while_surface_activation_is_pending() {
        let mut state = LiveState::default();
        state.set_outer_focus(true);
        state.apply(ClientEvent::Snapshot(agent_snapshot(
            AgentStatus::Working,
            9,
        )));
        let idle = agent_snapshot(AgentStatus::Idle, 10);
        let frame = agent_surface(&idle);
        state.apply(ClientEvent::Snapshot(idle.clone()));
        state.apply(ClientEvent::Surface(frame.clone()));
        state.activation = Some(activating(&idle));
        assert!(!state.acknowledge_presented_surface(&idle, &frame, true));
        assert_status(&state, AgentStatus::Done);
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
            reason: "unsupported".into(),
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
    fn same_surface_arc_preserves_dirty_but_distinct_arc_marks_dirty() {
        let mut state = LiveState::default();
        let snapshot = snapshot();
        let frame = surface(&snapshot);
        state.apply(ClientEvent::Snapshot(snapshot));
        state.dirty = false;
        state.apply(ClientEvent::Surface(frame.clone()));
        assert!(state.dirty);
        for dirty in [false, true] {
            state.dirty = dirty;
            state.apply(ClientEvent::Surface(frame.clone()));
            assert_eq!(state.dirty, dirty);
            assert!(Arc::ptr_eq(state.surface.as_ref().unwrap(), &frame));
        }
        let replacement = Arc::new((*frame).clone());
        state.dirty = false;
        state.apply(ClientEvent::Surface(replacement.clone()));
        assert!(state.dirty);
        assert!(Arc::ptr_eq(state.surface.as_ref().unwrap(), &replacement));
    }

    #[test]
    fn rejected_surfaces_preserve_dirty_and_current_surface() {
        let snapshot = snapshot();
        let frame = surface(&snapshot);
        let mut stale = surface(&snapshot);
        Arc::make_mut(&mut stale).projection_revision += 1;
        let mut wrong_boot = surface(&snapshot);
        Arc::make_mut(&mut wrong_boot).boot_id = "wrong-boot".into();
        for dirty in [false, true] {
            let mut state = LiveState {
                dirty,
                ..LiveState::default()
            };
            state.apply(ClientEvent::Surface(frame.clone()));
            assert_eq!(state.dirty, dirty);
            assert!(state.surface.is_none());

            state.apply(ClientEvent::Snapshot(snapshot.clone()));
            state.apply(ClientEvent::Surface(frame.clone()));
            for rejected in [&stale, &wrong_boot] {
                state.dirty = dirty;
                state.apply(ClientEvent::Surface(rejected.clone()));
                assert_eq!(state.dirty, dirty);
                assert!(Arc::ptr_eq(state.surface.as_ref().unwrap(), &frame));
            }
        }
    }

    #[test]
    fn tracked_activation_responses_preserve_noop_dirty_and_readiness_fences() {
        let snapshot = snapshot();
        for failed in [false, true] {
            let mut state = LiveState::default();
            state.apply(ClientEvent::Snapshot(snapshot.clone()));
            state.apply(ClientEvent::Surface(surface(&snapshot)));
            state.activation = Some(activating(&snapshot));
            state.track_request("activate-1".into());
            let response = if failed {
                serde_json::json!({"error": "unsupported"})
            } else {
                serde_json::json!({"error": null, "result": {
                    "type": "client_shell_surface_set", "active": true,
                    "projection_revision": snapshot.revision
                }})
            };
            for expected_dirty in [true, false] {
                state.dirty = false;
                state.apply(ClientEvent::Response {
                    request_id: "activate-1".into(),
                    response: response.clone(),
                });
                assert_eq!(state.dirty, expected_dirty);
                assert_eq!(state.surface_ready(), !failed);
                assert_eq!(
                    state.request_status("activate-1"),
                    Some(if failed {
                        RequestStatus::Failed
                    } else {
                        RequestStatus::Succeeded
                    })
                );
            }
        }
    }

    #[test]
    fn tracked_success_marks_dirty_only_when_status_changes() {
        let mut state = LiveState::default();
        assert_eq!(state.request_status("navigation"), None);
        state.track_request("navigation".into());
        assert_eq!(
            state.request_status("navigation"),
            Some(RequestStatus::Queued)
        );
        for expected_dirty in [true, false] {
            state.dirty = false;
            state.apply(ClientEvent::Response {
                request_id: "navigation".into(),
                response: serde_json::json!({"result": {}}),
            });
            assert_eq!(state.dirty, expected_dirty);
            assert_eq!(
                state.request_status("navigation"),
                Some(RequestStatus::Succeeded)
            );
            assert_eq!(state.error, None);
        }
    }

    #[test]
    fn unrelated_completions_preserve_latest_request() {
        let mut state = LiveState::default();
        state.track_request("old".into());
        state.track_request("new".into());
        assert_eq!(state.request_status("old"), None);
        for event in [
            ClientEvent::Response {
                request_id: "old".into(),
                response: serde_json::json!({"result": {}}),
            },
            ClientEvent::Response {
                request_id: "old".into(),
                response: serde_json::json!({"error": "old failure"}),
            },
            ClientEvent::CommandRejected {
                request_id: Some("old".into()),
                reason: "old rejection".into(),
            },
            ClientEvent::CommandRejected {
                request_id: None,
                reason: "untracked rejection".into(),
            },
        ] {
            state.apply(event);
            assert_eq!(state.request_status("new"), Some(RequestStatus::Queued));
            assert_eq!(state.request_status("old"), None);
        }
    }

    #[test]
    fn tracked_failures_mark_dirty_even_when_displayed_error_is_unchanged() {
        for rejected in [false, true] {
            let event = || {
                if rejected {
                    ClientEvent::CommandRejected {
                        request_id: Some("navigation".into()),
                        reason: "failure".into(),
                    }
                } else {
                    ClientEvent::Response {
                        request_id: "navigation".into(),
                        response: serde_json::json!({"error": "failure"}),
                    }
                }
            };
            let mut state = LiveState::default();
            state.apply(event());
            state.track_request("navigation".into());
            for expected_dirty in [true, false] {
                state.dirty = false;
                state.apply(event());
                assert_eq!(state.dirty, expected_dirty);
                assert_eq!(
                    state.request_status("navigation"),
                    Some(RequestStatus::Failed)
                );
            }
        }
    }

    #[test]
    fn disconnect_fails_queued_request_and_boot_change_clears_tracking() {
        let mut state = LiveState::default();
        state.track_request("navigation".into());
        state.apply(ClientEvent::Snapshot(snapshot()));
        state.apply(ClientEvent::Snapshot(snapshot()));
        assert_eq!(
            state.request_status("navigation"),
            Some(RequestStatus::Queued)
        );
        state.apply(ClientEvent::Disconnected {
            reason: "closed".into(),
        });
        assert_eq!(
            state.request_status("navigation"),
            Some(RequestStatus::Failed)
        );
        state.apply(ClientEvent::Snapshot(snapshot()));
        state.track_request("new".into());
        let mut reboot = snapshot();
        Arc::make_mut(&mut reboot).boot_id = "new-boot".into();
        state.apply(ClientEvent::Snapshot(reboot));
        assert_eq!(state.request_status("new"), None);
    }

    #[test]
    fn successful_response_preserves_dirty_and_existing_error() {
        for dirty in [false, true] {
            for error in [None, Some("existing error".to_owned())] {
                let mut state = LiveState {
                    dirty,
                    error: error.clone(),
                    ..LiveState::default()
                };
                state.apply(ClientEvent::Response {
                    request_id: "test".into(),
                    response: serde_json::json!({"result": {}}),
                });
                assert_eq!(state.dirty, dirty);
                assert_eq!(state.error, error);
            }
        }
    }

    #[test]
    fn error_events_dirty_only_when_displayed_error_changes() {
        for kind in 0..3 {
            let event = |message: &str| match kind {
                0 => ClientEvent::CommandRejected {
                    request_id: Some("test".into()),
                    reason: message.into(),
                },
                1 => ClientEvent::Message(ServerMessage::ClientShellError {
                    message: message.into(),
                }),
                _ => ClientEvent::Response {
                    request_id: "test".into(),
                    response: serde_json::json!({"error": message}),
                },
            };
            let mut state = LiveState {
                dirty: false,
                ..LiveState::default()
            };
            state.apply(event("first"));
            assert!(state.dirty);
            let first = state.error.clone();
            assert!(first.is_some());
            for dirty in [false, true] {
                state.dirty = dirty;
                state.apply(event("first"));
                assert_eq!(state.dirty, dirty);
                assert_eq!(state.error, first);
            }
            state.dirty = false;
            state.apply(event("second"));
            assert!(state.dirty);
            assert_ne!(state.error, first);
        }
    }

    #[test]
    fn same_revision_snapshots_still_project_updates_and_mark_dirty() {
        let mut state = LiveState::default();
        let initial = agent_snapshot(AgentStatus::Working, 9);
        state.apply(ClientEvent::Snapshot(initial.clone()));
        state.dirty = false;
        state.apply(ClientEvent::Snapshot(initial.clone()));
        assert!(state.dirty);

        let mut next = agent_snapshot(AgentStatus::Idle, 10);
        Arc::make_mut(&mut next).revision = initial.revision;
        state.dirty = false;
        state.apply(ClientEvent::Snapshot(next));
        assert!(state.dirty);
        assert_status(&state, AgentStatus::Done);
        assert_eq!(
            state.snapshot.as_ref().unwrap().agents[0].state_change_seq,
            10
        );
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
}
