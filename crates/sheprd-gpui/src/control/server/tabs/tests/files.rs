//! `sheprd files` finds the caller's checkout the way `sheprd review` does,
//! and refuses a folder outside any repository.
use super::*;

fn space_in(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, branch: Option<&str>) {
    let directory = std::env::temp_dir().join("clone");
    cx.cx.update(|cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            let space = &mut shown.workspaces[0];
            space.new_workspace_cwd = directory.to_str().unwrap().into();
            space.branch = branch.map(Into::into);
            space.worktree = None;
            view.live.snapshot = Some(Arc::new(shown));
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.live.local_daemon_peer = true;
            view.update_git();
        })
    });
}

fn files(cx: &mut VisualTestContext) -> Response {
    let address = socket::test_address();
    let server = socket::Server::bind(&address.path).unwrap();
    let (call, incoming, waiting) = queued(&server, TabOp::Files, Caller::default());
    let mut watches = Watches::default();
    cx.cx.update(|cx| {
        serve(call, incoming, &mut watches, cx);
        watches.serve(cx);
    });
    waiting.join().unwrap().unwrap()
}

#[gpui::test]
fn files_opens_the_workspaces_tree_and_lists_as_files(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    space_in(&view, cx, Some("main"));
    let response = files(cx);
    let Response::Tab {
        reply: Reply::Opened { id, workspace },
    } = response
    else {
        panic!("no file tree opened: {response:?}");
    };
    assert_eq!(workspace, "w0");
    cx.cx.update(|cx| {
        let tab = cx.global::<Store>().get(TabId::from(id)).cloned().unwrap();
        assert!(matches!(tab.location, Some(Location::Files { .. })));
        assert_eq!(kind(&tab), TabKind::Files);
        assert!(location(&tab).unwrap().ends_with("clone"));
    });
}

#[gpui::test]
fn files_is_refused_in_a_folder_outside_any_repository(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    space_in(&view, cx, None);
    let response = files(cx);
    assert!(
        matches!(
            response,
            Response::Error {
                code: ErrorCode::Failed,
                ..
            }
        ),
        "{response:?}"
    );
}
