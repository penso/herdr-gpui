use super::*;

fn pending_deletion(
    view: &mut HerdrWindow,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<HerdrWindow>,
) {
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.open_workspace_menu("w4", Default::default(), window, cx);
    view.open_workspace_dialog(WorkspaceAction::DeleteWorktree, window, cx);
    view.menu.error = None;
    view.menu.deletion = Some(Deletion {
        path: Some("/daemon/checkout".into()),
        archive: ArchiveCheck::Read(None),
        ..Deletion::new(None, false)
    });
    view.removal = Some(super::super::Removal {
        endpoint: view.menu.endpoint_target,
        boot_id: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
        workspace: "w4".into(),
        pending: Some("remove".into()),
        force: false,
    });
    cx.notify();
}

#[gpui::test]
fn dirty_removal_offers_force_in_the_same_dialog(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            pending_deletion(view, window, cx);
            // Repeated confirmation cannot queue another request, even though the
            // checkout lookup is complete. This fixture has no request handle.
            view.submit_workspace_dialog(window, cx);
            assert!(view.menu.error.is_none());
            assert_eq!(
                view.removal.as_ref().unwrap().pending.as_deref(),
                Some("remove")
            );
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("dialog-waiting").is_some());

    cx.update(|window, cx| view.update(cx, |view, cx| {
        view.live.dialog_response = Some(("remove".into(), Some(Ok(serde_json::json!({
            "error": {"code": "dirty_worktree_requires_force", "message": "modified or untracked files"}
        })))));
        view.update_workspace_dialog(window, cx);
        assert_eq!(view.menu.page, Some(super::super::Page::Dialog(WorkspaceAction::DeleteWorktree)));
        let deletion = view.menu.deletion.as_ref().unwrap();
        assert!(deletion.force && deletion.ready());
        assert_eq!(deletion.path.as_deref(), Some("/daemon/checkout"));
        assert_eq!(view.menu.error.as_deref(), Some("dirty_worktree_requires_force: modified or untracked files"));
        // Offering force does not submit it. Another update must not repeat the
        // reply or close the confirmation before the user chooses.
        assert!(view.removal.as_ref().unwrap().pending.is_none());
        view.update_workspace_dialog(window, cx);
        assert!(view.menu.deletion.as_ref().unwrap().force);
    }));
    cx.run_until_parked();
    assert!(cx.debug_bounds("dialog-waiting").is_none());
    assert!(cx.debug_bounds("dialog-error").is_some());
    assert!(cx.debug_bounds("dialog-submit").is_some());
}

#[gpui::test]
fn removal_success_closes_only_its_confirmation(cx: &mut gpui::TestAppContext) {
    for another_menu in [false, true] {
        let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                pending_deletion(view, window, cx);
                if another_menu {
                    view.dismiss_menu(window, cx);
                    view.open_workspace_menu("w3", Default::default(), window, cx);
                }
                view.live.dialog_response = Some((
                    "remove".into(),
                    Some(Ok(serde_json::json!({
                        "error": null, "result": {"type": "worktree_removed", "workspace_id": "w4"}
                    }))),
                ));
                view.update_workspace_dialog(window, cx);
                assert!(view.removal.is_none());
                assert!(view.local_error.is_none());
                assert_eq!(
                    view.menu.page,
                    another_menu.then_some(super::super::Page::Workspace)
                );
                if another_menu {
                    assert_eq!(view.menu.target.as_ref().unwrap().id, "w3");
                }
            })
        });
    }
}

#[gpui::test]
fn removal_failures_do_not_force_or_reopen_dismissed_dialogs(cx: &mut gpui::TestAppContext) {
    for (code, dismissed) in [
        ("worktree_remove_failed", false),
        ("dirty_worktree_requires_force", true),
    ] {
        let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                pending_deletion(view, window, cx);
                if dismissed {
                    view.dismiss_menu(window, cx);
                }
                view.live.dialog_response = Some((
                    "remove".into(),
                    Some(Ok(serde_json::json!({
                        "error": {"code": code, "message": "refused"}
                    }))),
                ));
                view.update_workspace_dialog(window, cx);
                assert!(view.local_error.is_some());
                if dismissed {
                    assert!(view.menu.page.is_none());
                    assert!(view.removal.as_ref().unwrap().force);
                } else {
                    assert!(view.menu.error.is_some());
                    assert!(!view.menu.deletion.as_ref().unwrap().force);
                    assert!(view.removal.is_none());
                }
            })
        });
    }
}
