use super::*;
use crate::{
    control::protocol::work::{NoteInfo, Reply},
    worktree_notes::Checkout,
};

#[gpui::test]
fn callers_resolve_to_their_checkout_by_repository_and_branch(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            connected(view);
            let found = find(view, &caller(Some("w4"))).ok().unwrap();
            assert_eq!(found.index, 0);
            assert_eq!(found.workspace.workspace_id, "w4");
            assert_eq!(repository(found.workspace), "agent-launcher");
            assert_eq!(
                notes::checkout_of(&view.endpoints[found.index].id, found.workspace),
                Some(Checkout {
                    endpoint: crate::endpoint::LOCAL.into(),
                    repo_key: REPO_KEY.into(),
                    branch: "worktree/sidebar-child".into(),
                })
            );
            // Outside Git there is no checkout to keep a note on.
            let outside = find(view, &caller(Some("w0"))).ok().unwrap();
            assert_eq!(notes::checkout_of("local", outside.workspace), None);
            assert_eq!(
                find(view, &caller(Some("w9")))
                    .err()
                    .as_ref()
                    .and_then(error_code),
                Some(ErrorCode::WorkspaceNotFound)
            );
            // Without a workspace, the one the window focuses.
            std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_workspace_id =
                Some("w3".into());
            let focused = find(view, &caller(None)).ok().unwrap();
            assert_eq!(focused.workspace.workspace_id, "w3");
            assert_eq!(
                selected_workspace(view, &caller(None)).ok().as_deref(),
                Some("w3")
            );
            view.live.status = crate::state::ConnectionStatus::Disconnected;
            assert_eq!(
                selected_workspace(view, &caller(None))
                    .err()
                    .as_ref()
                    .and_then(error_code),
                Some(ErrorCode::Busy),
                "dialogs act through a live connection"
            );
        })
    });
}

#[gpui::test]
fn notes_are_written_and_read_for_the_callers_checkout(cx: &mut gpui::TestAppContext) {
    let (view, window) = cx.add_window_view(fixture_window);
    window.update(|_, cx| view.update(cx, |view, _| connected(view)));
    // Outside the window's update, as the control socket serves requests.
    cx.update(|cx| {
        let caller = caller(Some("w4"));
        assert_eq!(
            notes::get(&caller, cx),
            Response::Work {
                reply: Reply::Note { note: None }
            }
        );
        assert_eq!(
            notes::set(&caller, "  waiting on\nreview ", cx),
            Response::done("Saved the note on worktree/sidebar-child")
        );
        let expected = NoteInfo {
            host: crate::endpoint::LOCAL.into(),
            repository: REPO_KEY.into(),
            branch: "worktree/sidebar-child".into(),
            text: "waiting on review".into(),
        };
        assert_eq!(
            notes::get(&caller, cx),
            Response::Work {
                reply: Reply::Note {
                    note: Some(expected.clone())
                }
            }
        );
        // The same checkout through its sibling workspace is another branch.
        assert_eq!(
            notes::get(&super::caller(Some("w5")), cx),
            Response::Work {
                reply: Reply::Note { note: None }
            }
        );
        assert_eq!(
            notes::list(cx),
            Response::Work {
                reply: Reply::Notes {
                    notes: vec![expected],
                    omitted: 0,
                }
            }
        );
        assert_eq!(
            error_code(&notes::set(&super::caller(Some("w0")), "x", cx)),
            Some(ErrorCode::InvalidRequest),
            "a workspace outside Git keeps no note"
        );
        assert_eq!(
            notes::set(&caller, "", cx),
            Response::done("Cleared the note on worktree/sidebar-child")
        );
        assert_eq!(
            notes::list(cx),
            Response::Work {
                reply: Reply::Notes {
                    notes: Vec::new(),
                    omitted: 0,
                }
            }
        );
    });
}
