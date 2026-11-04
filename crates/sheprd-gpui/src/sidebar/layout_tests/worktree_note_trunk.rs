use super::*;
use crate::{
    config::LayoutMode,
    worktree_notes::{Checkout, Notes},
};

fn checkout(branch: &str) -> Checkout {
    Checkout {
        endpoint: crate::endpoint::LOCAL.into(),
        repo_key: REPO_KEY.into(),
        branch: branch.into(),
    }
}

#[gpui::test]
fn a_childs_trunk_runs_on_past_its_note_to_the_next_sibling(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot(6)));
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.live.supports_surface = true;
        view.endpoints[0].live = view.live.clone();
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.update(|_, cx| {
        Notes::update(cx, |notes| {
            // w4 is a middle child and w5 the group's last.
            notes.set(checkout("worktree/sidebar-child"), "wait for the FX PR");
            notes.set(
                checkout("worktree/sidebar-child-with-a-long-readable-branch-name"),
                "last one",
            );
        });
    });
    cx.run_until_parked();
    let next = "row-sidebar-child-with-a-long-readable-branch-name";
    let mut drawn = 0;
    for mode in LayoutMode::ALL
        .into_iter()
        .filter(|mode| mode.lists_workspaces())
    {
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            view.sidebar_width = Some(320.);
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let tree = cx.debug_bounds("tree-sidebar-child");
        let trunk = cx.debug_bounds("trunk-local-w4");
        // The last child's elbow ends at its row, note or not.
        assert!(cx.debug_bounds("trunk-local-w5").is_none(), "{mode:?}");
        let Some(tree) = tree else {
            assert!(trunk.is_none(), "{mode:?} draws no tree, so no trunk");
            continue;
        };
        drawn += 1;
        let trunk = trunk.unwrap_or_else(|| panic!("{mode:?} drops the trunk under the note"));
        let line = cx.debug_bounds("note-line-local-w4").unwrap();
        // Same column as the row's own trunk, from the note's top down to
        // the next sibling, so the tree reads unbroken.
        assert_eq!(trunk.left(), tree.left(), "{mode:?}");
        assert_eq!(trunk.size.width, tree.size.width, "{mode:?}");
        assert_eq!(trunk.top(), line.top(), "{mode:?}");
        assert_eq!(trunk.top(), tree.bottom(), "{mode:?}");
        assert_eq!(
            trunk.bottom(),
            cx.debug_bounds(next).unwrap().top(),
            "{mode:?}"
        );
    }
    assert!(drawn > 0, "some layout draws tree lines");
}
