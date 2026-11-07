use super::*;

#[test]
fn a_standalone_linked_worktree_matches_its_displayed_workspace_label() {
    let mut snapshot = snapshot(6);
    snapshot
        .workspaces
        .retain(|workspace| workspace.workspace_id == "w4");
    let [local, _] = devices(&snapshot);
    let hits = search("agent-launcher", &[local]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, Kind::Worktree);
    assert_eq!(hits[0].text, "agent-launcher");
    assert_eq!(
        hits[0].target,
        Target::Workspace {
            endpoint: "local".into(),
            workspace: "w4".into(),
        }
    );

    let [local, _] = devices(&snapshot);
    let hits = search("sidebar-child", &[local]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, Kind::Branch);
    assert_eq!(hits[0].context, "agent-launcher");
}

#[test]
fn matches_follow_sidebar_group_order_even_when_children_arrive_first() {
    let mut snapshot = snapshot(6);
    snapshot.workspaces.swap(0, 4);
    let expected: Vec<_> = workspace_entries(&snapshot.workspaces)
        .into_iter()
        .filter_map(|(index, child)| {
            let workspace = &snapshot.workspaces[index];
            find(workspace_label(workspace, child), "a").map(|_| workspace.workspace_id.as_str())
        })
        .collect();
    let [local, _] = devices(&snapshot);
    let hits = search("a", &[local]);
    let actual: Vec<_> = hits
        .iter()
        .filter(|hit| hit.kind == Kind::Worktree)
        .filter_map(|hit| match &hit.target {
            Target::Workspace { workspace, .. } => Some(workspace.as_str()),
            Target::Device { .. } => None,
        })
        .collect();
    assert_eq!(actual, expected);
}
