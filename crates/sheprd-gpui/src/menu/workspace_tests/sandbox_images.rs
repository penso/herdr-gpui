//! A Git workspace's menu offers Build sandbox images once the user has a
//! device from a provider that holds images, and not before.

use super::*;
use crate::cloud::CloudProvider;

fn local(view: &mut SheprdWindow) {
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
}

fn add_device(view: &mut SheprdWindow, provider: CloudProvider, id: &str) {
    view.endpoints.push(crate::endpoint::Endpoint::new(
        format!("{}:{id}", provider.key()),
        id.into(),
        sheprd_client::ConnectTarget::Cloud {
            provider,
            account: "account".into(),
            id: id.into(),
            machine: id.into(),
            session: "default".into(),
        },
        true,
    ));
}

fn offered(view: &gpui::Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            view.open_workspace_menu("w3", Default::default(), window, cx);
            view.workspace_items()
                .iter()
                .any(|(action, _)| *action == WorkspaceMenuAction::SandboxImages)
        })
    })
}

#[gpui::test]
fn images_are_offered_only_where_a_provider_holds_them(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|_, cx| view.update(cx, |view, _| local(view)));
    assert!(!offered(&view, cx), "no cloud device yet");
    // Sprites make new machines for a dispatch, but hold no images.
    cx.update(|_, cx| view.update(cx, |view, _| add_device(view, CloudProvider::Sprites, "s1")));
    assert!(!offered(&view, cx));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            add_device(view, CloudProvider::Container, "c1")
        })
    });
    assert!(offered(&view, cx));
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).sandbox_image_providers(),
            [CloudProvider::Container]
        );
    });
}
