use super::*;
use gpui::{KeyDownEvent, Keystroke, PlatformInput};

#[gpui::test]
fn held_enter_cannot_confirm_removal_or_its_force_retry(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.open_workspace_menu("w4", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::DeleteWorktree, window, cx);
            view.menu.error = None;
            view.menu.deletion = Some(Deletion {
                path: Some("/daemon/checkout".into()),
                archive: ArchiveCheck::Read(None),
                ..Deletion::new(None, false)
            });
            cx.notify();
        });
    });
    let enter = |cx: &mut gpui::VisualTestContext, is_held| {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse("enter").unwrap(),
                    is_held,
                    prefer_character_input: false,
                }),
                cx,
            );
        });
    };

    // A repeat from the key that opened the dialog cannot confirm deletion.
    enter(cx, true);
    view.update(cx, |view, _| {
        assert!(view.menu.error.is_none());
        assert!(view.removal.is_none());
    });

    // A fresh press does reach submission. There is no connection handle in
    // this fixture, so the queue attempt is observable as NotConnected.
    enter(cx, false);
    view.update(cx, |view, _| {
        assert_eq!(
            view.menu.error,
            Some(crate::Error::NotConnected.to_string())
        );
    });

    // Model the request accepted by a connected daemon and its dirty refusal
    // while that first Enter is still held down.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.error = None;
            view.removal = Some(super::super::Removal {
                endpoint: view.menu.endpoint_target,
                boot_id: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
                workspace: "w4".into(),
                pending: Some("remove".into()),
                force: false,
            });
            view.live.dialog_response = Some((
                "remove".into(),
                Some(Ok(serde_json::json!({"error": {
                    "code": "dirty_worktree_requires_force", "message": "dirty checkout"
                }}))),
            ));
            view.update_workspace_dialog(window, cx);
            assert!(view.menu.deletion.as_ref().unwrap().force);
        });
    });

    for _ in 0..3 {
        enter(cx, true);
        view.update(cx, |view, _| {
            assert_eq!(
                view.menu.error.as_deref(),
                Some("dirty_worktree_requires_force: dirty checkout")
            );
            assert!(view.removal.as_ref().unwrap().pending.is_none());
            assert!(view.menu.deletion.as_ref().unwrap().force);
        });
    }

    // Only releasing and pressing Enter again attempts the forced request.
    enter(cx, false);
    view.update(cx, |view, _| {
        assert_eq!(
            view.menu.error,
            Some(crate::Error::NotConnected.to_string())
        );
        assert!(view.menu.deletion.as_ref().unwrap().force);
    });
}
