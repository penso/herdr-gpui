use super::*;
use crate::notifications::tests::notification;
use herdr_client::protocol::{
    AgentStatus, ClientShellSnapshot, FrameData, PaneSurfaceFrame,
    SemanticNotificationKind as Kind, ServerMessage,
};
use std::time::Duration;

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

fn add_blocked_agent_in_another_tab(snapshot: &mut ClientShellSnapshot) -> &'static str {
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "w1:t2".into();
    tab.focused = false;
    snapshot.tabs.push(tab);
    let mut pane = snapshot.panes[0].clone();
    pane.pane_id = "w1:p2".into();
    pane.tab_id = "w1:t2".into();
    pane.focused = false;
    snapshot.panes.push(pane);
    let mut agent = snapshot.agents[0].clone();
    agent.pane_id = "w1:p2".into();
    agent.tab_id = "w1:t2".into();
    agent.agent_status = AgentStatus::Blocked;
    snapshot.agents.push(agent);
    "w1:p2"
}

/// A notice arrives with metadata that still waits for its surface. The
/// displayed snapshot says the agent is working, or does not know its pane.
#[test]
fn notices_judge_agents_by_metadata_that_waits_for_its_surface() {
    type Stage = fn(&mut ClientShellSnapshot) -> &'static str;
    let cases: [(Kind, Stage); 3] = [
        (Kind::NeedsAttention, |s| {
            s.agents[0].agent_status = AgentStatus::Blocked;
            "w1:p1"
        }),
        (Kind::Finished, |s| {
            s.agents[0].agent_status = AgentStatus::Done;
            "w1:p1"
        }),
        (Kind::NeedsAttention, add_blocked_agent_in_another_tab),
    ];
    for (kind, stage) in cases {
        let mut displayed: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        displayed.agents[0].agent_status = AgentStatus::Working;
        let displayed = Arc::new(displayed);
        let mut staged = (*displayed).clone();
        staged.revision += 1;
        let pane = stage(&mut staged);
        let staged = Arc::new(staged);
        let mut wire = notification("Agent");
        wire.kind = kind;
        wire.workspace_id = Some("w1".into());
        wire.pane_id = Some(pane.into());
        let mut endpoint = Endpoint::new(LOCAL.into(), "Local".into(), ConnectTarget::Local, true);
        {
            let mut state = endpoint.connection.inbox.lock().unwrap();
            state.apply(ClientEvent::Snapshot(displayed.clone()));
            state.apply(ClientEvent::Surface(surface(&displayed)));
            state.apply(ClientEvent::Snapshot(staged.clone()));
            assert!(state.pending_snapshot.is_some(), "{pane}: not staged");
            state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                wire,
            )));
        }
        let now = Instant::now();
        endpoint.poll(now);
        let config = crate::config::NotificationConfig {
            enabled: true,
            ..Default::default()
        };
        let mut endpoints = [endpoint];
        // Selected index 1 is no endpoint, so the focused tab suppresses nothing.
        for at in [now, now + Duration::from_secs(1)] {
            crate::notifications::tick(&mut endpoints, 1, config, None, false, true, None, at);
        }
        let entries = &endpoints[0].toasts.entries;
        assert!(
            entries.len() == 1 && entries[0].1.visible,
            "{kind:?} for {pane} was dropped"
        );
        // The matching surface commits the metadata without another notice.
        endpoints[0]
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Surface(surface(&staged)));
        endpoints[0].poll(now + Duration::from_secs(1));
        assert!(Arc::ptr_eq(
            endpoints[0].live.snapshot.as_ref().unwrap(),
            &staged
        ));
        crate::notifications::tick(
            &mut endpoints,
            1,
            config,
            None,
            false,
            true,
            None,
            now + Duration::from_secs(2),
        );
        assert_eq!(endpoints[0].toasts.entries.len(), 1, "{kind:?} for {pane}");
    }
}
