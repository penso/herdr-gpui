use super::*;

#[gpui::test]
fn focused_merged_worktree_asks_with_the_delete_dialog(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.ask_merged_removal("w4", 8, window, cx);
            assert_eq!(
                view.menu.page,
                Some(super::super::Page::Dialog(WorkspaceAction::DeleteWorktree))
            );
            assert_eq!(view.menu.target.as_ref().unwrap().id, "w4");
            assert_eq!(view.menu.deletion.as_ref().unwrap().merged, Some(8));
            // Asking sends nothing; the user confirms in the dialog.
            assert!(view.removal.is_none());
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("dialog-merged-pr").is_some());
}
