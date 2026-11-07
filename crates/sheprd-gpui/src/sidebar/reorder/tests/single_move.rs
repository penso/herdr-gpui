use super::*;

/// The order Herdr leaves after `reorder`, by its own two algorithms.
fn apply(list: &[ClientShellWorkspace], reorder: &WorkspaceReorder) -> Vec<String> {
    let mut ids: Vec<String> = list.iter().map(|w| w.workspace_id.clone()).collect();
    match reorder {
        WorkspaceReorder::Move(params) => {
            let source = ids
                .iter()
                .position(|id| *id == params.workspace_id)
                .unwrap();
            let target = if source < params.insert_index {
                params.insert_index - 1
            } else {
                params.insert_index
            };
            let id = ids.remove(source);
            ids.insert(target, id);
        }
        WorkspaceReorder::MoveBlock(params) => {
            ids.retain(|id| !params.workspace_ids.contains(id));
            let at = params
                .before_workspace_id
                .as_ref()
                .and_then(|before| ids.iter().position(|id| id == before))
                .unwrap_or(ids.len());
            ids.splice(at..at, params.workspace_ids.iter().cloned());
        }
    }
    ids
}

/// Every drop of a lone row, top-level or child, is a `workspace.move`
/// where offered, and leaves the order the block move would have.
#[test]
fn a_lone_workspace_moves_singly_to_the_same_place() {
    let list = list();
    for pressed in ["b", "c", "a1", "a2"] {
        let plan = Plan::new(&list, pressed).unwrap();
        for slot in 0..=plan.len() {
            let Some(request) = plan.request(&list, slot) else {
                continue;
            };
            let single = request.reorder(&list, true);
            let WorkspaceReorder::Move(params) = &single else {
                panic!("{pressed} to {slot} sent {single:?}");
            };
            assert_eq!(params.workspace_id, pressed);
            let block = request.reorder(&list, false);
            assert!(matches!(block, WorkspaceReorder::MoveBlock(_)));
            assert_eq!(
                apply(&list, &single),
                apply(&list, &block),
                "{pressed} {slot}"
            );
        }
    }
    // Concretely: `c` before `a` counts from `a`, and `b` last is the length.
    let plan = Plan::new(&list, "c").unwrap();
    assert_eq!(
        plan.request(&list, 0).unwrap().reorder(&list, true),
        WorkspaceReorder::Move(WorkspaceMoveParams {
            workspace_id: "c".into(),
            insert_index: 0,
        })
    );
    let plan = Plan::new(&list, "b").unwrap();
    assert_eq!(
        plan.request(&list, 3).unwrap().reorder(&list, true),
        WorkspaceReorder::Move(WorkspaceMoveParams {
            workspace_id: "b".into(),
            insert_index: 5,
        })
    );
}

#[test]
fn a_group_always_moves_as_a_block() {
    let list = list();
    let plan = Plan::new(&list, "a").unwrap();
    let request = plan.request(&list, 3).unwrap();
    assert_eq!(
        request.reorder(&list, true),
        WorkspaceReorder::MoveBlock(WorkspaceMoveBlockParams {
            workspace_ids: vec!["a".into(), "a1".into(), "a2".into()],
            before_workspace_id: None,
        })
    );
}

/// An anchor the current order no longer holds cannot become an index,
/// so the daemon resolves it by name instead.
#[test]
fn an_anchor_missing_from_the_order_falls_back_to_the_block_move() {
    let list = list();
    let request = Plan::new(&list, "c").unwrap().request(&list, 0).unwrap();
    let mut stale = list.clone();
    stale.retain(|w| w.workspace_id != "a");
    assert!(matches!(
        request.reorder(&stale, true),
        WorkspaceReorder::MoveBlock(_)
    ));
}
