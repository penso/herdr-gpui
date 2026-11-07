//! Checkpoints watch the snapshot the window shows, so a turn whose
//! metadata waits for its surface still counts once it lands.

use super::*;
use crate::checkpoint::{Checkout, Checkpoints, Request};
use sheprd_client::protocol::{AgentStatus, ClientShellSnapshot, ClientShellWorktree};
use sheprd_usage::Host;

const KEY: &str = "/repo/.git";

/// Two agents: p0 in w4 (a linked worktree on `feature`), p1 in w0, both
/// set to `statuses`.
fn with(statuses: [AgentStatus; 2]) -> Arc<ClientShellSnapshot> {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(5);
    snapshot.workspaces[4].worktree = Some(ClientShellWorktree {
        key: KEY.into(),
        label: "repo".into(),
        is_linked_worktree: true,
    });
    snapshot.workspaces[4].branch = Some("feature".into());
    snapshot.agents[0].workspace_id = "w4".into();
    snapshot.agents[1].workspace_id = "w0".into();
    for (agent, status) in snapshot.agents.iter_mut().zip(statuses) {
        agent.agent_status = status;
    }
    Arc::new(snapshot)
}

/// A turn that ends in metadata still waiting for its surface is a turn.
#[test]
fn staged_metadata_reports_a_finished_turn() {
    use AgentStatus::*;
    use sheprd_client::{
        ClientEvent,
        protocol::{FrameData, PaneSurfaceFrame},
    };
    let displayed = with([Working, Idle]);
    let mut live = LiveState::default();
    live.apply(ClientEvent::Snapshot(displayed.clone()));
    live.apply(ClientEvent::Surface(Arc::new(PaneSurfaceFrame {
        boot_id: displayed.boot_id.clone(),
        projection_revision: displayed.revision,
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
    })));
    let mut done = (*with([Done, Idle])).clone();
    done.revision += 1;
    live.apply(ClientEvent::Snapshot(Arc::new(done)));
    assert!(Arc::ptr_eq(live.snapshot.as_ref().unwrap(), &displayed));
    let mut checkpoints = Checkpoints::default();
    checkpoints.observe("local", &Host::Local, &displayed, |_| None);
    checkpoints.observe(
        "local",
        &Host::Local,
        live.latest_snapshot().unwrap(),
        |_| None,
    );
    assert_eq!(
        checkpoints.queued(),
        [(
            Checkout::new(Host::Local, KEY, Some("feature")).unwrap(),
            Request::Capture {
                label: "Claude Code finished a turn".into(),
                origin: None,
            }
        )]
    );
}
