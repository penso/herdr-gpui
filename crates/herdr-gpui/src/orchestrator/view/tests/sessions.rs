use super::*;

#[test]
fn live_agents_are_scoped_to_the_named_session() {
    let issue = item(Provider::Github, "1", "Issue", "2026-10-04T00:00:00Z");
    let run = run("r1", &issue, RunState::Running, "2026-10-04T00:00:00Z");
    for host in [None, Some("remote".to_owned())] {
        let mut session = session(&run, "p1");
        session.host = host.clone();
        session.session = Some("one".into());
        let mut other = live(&run, "p1", AgentStatus::Working);
        other.host = host;
        other.session = Some("two".into());
        assert!(live_for(&session, &[other.clone()]).is_none());
        let mut correct = other.clone();
        correct.session = Some("one".into());
        correct.endpoint = 1;
        assert_eq!(live_for(&session, &[other, correct]).unwrap().endpoint, 1);
    }
}

#[test]
fn branch_runs_keep_sessions_distinct() {
    use crate::orchestrator::naming::{LiveBranch, branch, found_runs};
    let issue = item(Provider::Github, "1", "Issue", "2026-10-04T00:00:00Z");
    let workspace = LiveBranch {
        host: None,
        session: Some("one".into()),
        workspace_id: "w1".into(),
        branch: branch(&issue),
    };
    let other = LiveBranch {
        session: Some("two".into()),
        ..workspace.clone()
    };
    let agent = LiveAgent {
        endpoint: 0,
        host: None,
        session: Some("two".into()),
        workspace_id: "w1".into(),
        pane_id: "p1".into(),
        status: AgentStatus::Working,
        title: None,
    };
    let agents = [agent];
    let found = found_runs(&[issue], &[], &[workspace, other], &[], &agents);
    assert_ne!(found[0].id, found[1].id);
    assert_eq!(found[0].state, RunState::Cancelled);
    assert_eq!(found[1].state, RunState::Running);
    let runs = Runs::new(&found, &[], &agents);
    assert!(runs.live(0).is_none());
    assert!(runs.live(1).is_some());
}
