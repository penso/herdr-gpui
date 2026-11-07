use super::*;
use sheprd_client::protocol::ClientShellWorktree;

mod single_move;

fn workspace(id: &str, tree: Option<(&str, bool)>) -> ClientShellWorkspace {
    ClientShellWorkspace {
        workspace_id: id.into(),
        active_tab_id: String::new(),
        new_workspace_cwd: String::new(),
        number: 0,
        label: id.into(),
        custom_label: false,
        branch: None,
        git_ahead_behind: None,
        tokens: Vec::new(),
        worktree: tree.map(|(key, linked)| ClientShellWorktree {
            key: key.into(),
            label: key.into(),
            is_linked_worktree: linked,
        }),
        focused: false,
        agent_status: sheprd_client::protocol::AgentStatus::Idle,
    }
}

/// `a` heads a group whose child `a2` sits after `b` in daemon order.
fn list() -> Vec<ClientShellWorkspace> {
    vec![
        workspace("a", Some(("repo-a", false))),
        workspace("a1", Some(("repo-a", true))),
        workspace("b", None),
        workspace("a2", Some(("repo-a", true))),
        workspace("c", None),
    ]
}

fn request(ids: &[&str], before: Option<&str>) -> Option<MoveBlock> {
    Some(MoveBlock {
        workspace_ids: ids.iter().map(|&id| id.into()).collect(),
        before: before.map(Into::into),
    })
}

/// Every parent of a group carries the whole group, its other parents
/// included; without a linked worktree, plain checkouts move alone.
#[test]
fn a_group_s_parents_move_together_and_plain_checkouts_alone() {
    let mut list = list();
    list.push(workspace("a3", Some(("repo-a", false))));
    for pressed in ["a", "a3"] {
        let plan = Plan::new(&list, pressed).unwrap();
        assert_eq!(plan.units, [vec![0, 5, 1, 3], vec![2], vec![4]]);
        assert_eq!(plan.dragged, 0);
        assert_eq!(
            plan.request(&list, 3),
            request(&["a", "a3", "a1", "a2"], None)
        );
    }
    // A child still moves only among its siblings.
    let plan = Plan::new(&list, "a2").unwrap();
    assert_eq!(plan.units, [vec![1], vec![3]]);

    list.retain(|w| !w.worktree.as_ref().is_some_and(|t| t.is_linked_worktree));
    let plan = Plan::new(&list, "a3").unwrap();
    assert_eq!(plan.units, [vec![0], vec![1], vec![2], vec![3]]);
    assert_eq!(plan.dragged, 3);
    assert_eq!(plan.request(&list, 1), request(&["a3"], Some("b")));
}

#[test]
fn a_top_level_row_carries_its_whole_group() {
    let list = list();
    let plan = Plan::new(&list, "a").unwrap();
    assert_eq!(plan.units, [vec![0, 1, 3], vec![2], vec![4]]);
    assert_eq!(plan.dragged, 0);
    // The gaps around the group itself move nothing.
    assert_eq!(plan.request(&list, 0), None);
    assert_eq!(plan.request(&list, 1), None);
    assert_eq!(
        plan.request(&list, 2),
        request(&["a", "a1", "a2"], Some("c"))
    );
    assert_eq!(plan.request(&list, 3), request(&["a", "a1", "a2"], None));
    assert_eq!(plan.request(&list, 4), None);
}

#[test]
fn a_standalone_row_lands_before_a_group_s_earliest_member() {
    let mut list = list();
    // The group shows where `a1` sits, ahead of its own main checkout.
    list.swap(0, 1);
    let plan = Plan::new(&list, "c").unwrap();
    assert_eq!(plan.units, [vec![1, 0, 3], vec![2], vec![4]]);
    assert_eq!(plan.request(&list, 0), request(&["c"], Some("a1")));
}

#[test]
fn a_child_moves_only_among_its_siblings() {
    let list = list();
    let plan = Plan::new(&list, "a1").unwrap();
    assert!(plan.children);
    assert_eq!(plan.units, [vec![1], vec![3]]);
    assert_eq!(plan.unit_of(2), None);
    assert_eq!(plan.request(&list, 0), None);
    assert_eq!(plan.request(&list, 1), None);
    // Last among siblings stays ahead of what follows the group.
    assert_eq!(plan.request(&list, 2), request(&["a1"], Some("c")));
    let plan = Plan::new(&list, "a2").unwrap();
    assert_eq!(plan.request(&list, 0), request(&["a2"], Some("a1")));
}

#[test]
fn a_last_child_moved_to_the_end_of_the_list_needs_no_anchor() {
    let list = vec![
        workspace("a", Some(("repo-a", false))),
        workspace("a1", Some(("repo-a", true))),
        workspace("a2", Some(("repo-a", true))),
    ];
    let plan = Plan::new(&list, "a1").unwrap();
    assert_eq!(plan.request(&list, 2), request(&["a1"], None));
}

#[test]
fn rows_with_nowhere_to_go_do_not_arm() {
    let list = vec![
        workspace("a", Some(("repo-a", false))),
        workspace("a1", Some(("repo-a", true))),
    ];
    assert_eq!(Plan::new(&list, "a"), None);
    assert_eq!(Plan::new(&list, "a1"), None);
    assert_eq!(Plan::new(&list, "missing"), None);
}
