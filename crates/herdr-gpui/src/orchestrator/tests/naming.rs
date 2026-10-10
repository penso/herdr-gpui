use super::*;
use crate::orchestrator::naming::{LiveBranch, branch, found_runs, hash_of, review_branch};
use herdr_client::protocol::AgentStatus;

#[test]
fn branches_are_named_as_agent_launcher_names_them() {
    let issue = item(&github(), "377", "Prompt icons: render as tofu!");
    assert_eq!(
        branch(&issue),
        "agent/377-prompt-icons-render-as-tofu-97b1caca"
    );
    let mut bead = item(&beads(), "hg-a3f2.1", "Shared SQLite store");
    bead.identifier = "hg-a3f2.1".into();
    assert_eq!(
        branch(&bead),
        "agent/hg-a3f2-1-shared-sqlite-store-26d1c19b"
    );
    let mut pr = item(&github(), "pr/369", "Find bar");
    pr.identifier = "#369".into();
    assert_eq!(review_branch(&pr), "agent/review-369-find-bar-3af51766");
    assert_eq!(hash_of(&pr), "3af51766");
    for name in [branch(&issue), review_branch(&pr)] {
        assert!(crate::orchestrator::prompt::valid_branch(&name), "{name}");
    }
}

fn workspace(id: &str, branch: &str) -> LiveBranch {
    LiveBranch {
        host: None,
        workspace_id: id.into(),
        branch: branch.into(),
    }
}

fn agent(workspace: &str, status: AgentStatus) -> LiveAgent {
    LiveAgent {
        endpoint: 0,
        host: None,
        workspace_id: workspace.into(),
        pane_id: "p1".into(),
        status,
        title: None,
    }
}

/// With no database, work is found from branches alone: a workspace with an
/// agent, one without, and a branch no workspace shows. Recorded runs and
/// branches of unknown items are left alone.
#[test]
fn work_is_found_from_branches_without_a_database() {
    let working = item(&github(), "1", "Working");
    let parked = item(&github(), "2", "Parked");
    let shelved = item(&github(), "3", "Shelved");
    let recorded = item(&github(), "4", "Recorded");
    let items = [
        working.clone(),
        parked.clone(),
        shelved.clone(),
        recorded.clone(),
    ];
    let mut run = Run {
        id: "r".into(),
        item_key: recorded.key.canonical(),
        workspace: Some(Workspace {
            backend: Backend::Herdr,
            id: "w4".into(),
            host: None,
            path: None,
            branch: branch(&recorded),
        }),
        agent: "claude".into(),
        model: None,
        state: RunState::Running,
        message: None,
        session_id: None,
        started_at: at("2026-10-08T23:23:37+00:00"),
        updated_at: at("2026-10-08T23:23:37+00:00"),
        owner: Owner::HerdrGpui,
    };
    let workspaces = [
        workspace("w1", &branch(&working)),
        workspace("w2", &branch(&parked)),
        workspace("w4", &branch(&recorded)),
        workspace("w9", "agent/someone-else-0123abcd"),
        workspace("w8", "main"),
    ];
    let branches = [branch(&parked), branch(&shelved), "agent/x-zzzzzzzz".into()];
    let agents = [
        agent("w1", AgentStatus::Idle),
        agent("w1", AgentStatus::Working),
    ];
    let found = found_runs(
        &items,
        std::slice::from_ref(&run),
        &workspaces,
        &branches,
        &agents,
    );
    let shape: Vec<_> = found
        .iter()
        .map(|run| {
            let workspace = run.workspace.as_ref().unwrap();
            (
                run.item_key.clone(),
                workspace.id.as_str(),
                run.state,
                run.owner,
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (
                working.key.canonical(),
                "w1",
                RunState::Running,
                Owner::Branch
            ),
            (
                parked.key.canonical(),
                "w2",
                RunState::Cancelled,
                Owner::Branch
            ),
            (
                shelved.key.canonical(),
                "",
                RunState::Cancelled,
                Owner::Branch
            ),
        ]
    );
    // A recorded run on another branch leaves its item's branch to be found.
    run.workspace = None;
    let found = found_runs(&items, &[run], &workspaces, &[], &[]);
    assert!(
        found
            .iter()
            .any(|run| run.item_key == recorded.key.canonical())
    );
}
