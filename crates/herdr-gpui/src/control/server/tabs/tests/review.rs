//! `herdr-gpui review` finds the caller's checkout the way the menu's Review
//! row does: in any repository its space is in, worktree metadata or not.
use super::*;

/// The window's own local daemon reports workspace `w0` in `directory`,
/// with no worktree metadata, and the Git chip follows it.
fn space_in(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, branch: Option<&str>) {
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

fn review(cx: &mut VisualTestContext) -> Response {
    let address = socket::test_address();
    let server = socket::Server::bind(&address.path).unwrap();
    let (call, incoming, waiting) = queued(&server, TabOp::Review, Caller::default());
    let mut watches = Watches::default();
    cx.cx.update(|cx| {
        serve(call, incoming, &mut watches, cx);
        watches.serve(cx);
    });
    waiting.join().unwrap().unwrap()
}

#[gpui::test]
fn review_opens_in_a_repository_herdr_did_not_make(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    space_in(&view, cx, Some("main"));
    let response = review(cx);
    let Response::Tab {
        reply: Reply::Opened { id, workspace },
    } = response
    else {
        panic!("no review opened: {response:?}");
    };
    assert_eq!(workspace, "w0");
    cx.cx.update(|cx| {
        let tab = cx.global::<Store>().get(TabId::from(id)).cloned().unwrap();
        assert!(matches!(tab.location, Some(Location::Review { .. })));
    });
}

#[gpui::test]
fn review_is_refused_in_a_folder_outside_any_repository(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    // The daemon reports no branch for a folder Git does not know.
    space_in(&view, cx, None);
    let response = review(cx);
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
