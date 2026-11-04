//! The new worktree dialog offers a new container beside the hosts once the
//! user has a container device, and submitting it makes one first.

use super::*;
use crate::{cloud::CloudProvider, dispatch::Target};

/// A window on a scriptable local daemon showing `snapshot(7)`.
fn local(view: &mut SheprdWindow) {
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
}

fn add_container(view: &mut SheprdWindow) {
    view.endpoints.push(crate::endpoint::Endpoint::new(
        "container:herdr-calm-otter-1a".into(),
        "herdr-calm-otter-1a".into(),
        sheprd_client::ConnectTarget::Cloud {
            provider: CloudProvider::Container,
            account: "docker".into(),
            id: "herdr-calm-otter-1a".into(),
            machine: "herdr-calm-otter-1a".into(),
            session: "default".into(),
        },
        true,
    ));
}

#[gpui::test]
fn a_new_container_is_offered_once_one_exists(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            local(view);
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            assert!(view.menu.dispatch.is_none(), "nothing to choose yet");
            view.dismiss_menu(window, cx);
            add_container(view);
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
            assert_eq!(
                view.menu.dispatch.as_ref().unwrap().sandboxes(),
                [CloudProvider::Container]
            );
        })
    });
    cx.run_until_parked();
    let tile = cx.debug_bounds("dispatch-sandbox-Container").unwrap();
    cx.simulate_click(tile.center(), gpui::Modifiers::none());
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.dispatch_choice(WorkspaceAction::NewWorktree),
            Some(Target::Sandbox(CloudProvider::Container))
        );
    });
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.menu.input = Some(DialogInput::new("bad branch name".into()));
            assert!(matches!(
                view.submit_dispatch(
                    WorkspaceAction::NewWorktree,
                    &Target::Sandbox(CloudProvider::Container),
                    None
                ),
                Err(crate::Error::InvalidBranchName)
            ));
            assert!(view.dispatch_job.is_none(), "nothing is made on a bad name");
            let _ = window;
        })
    });
}
