use super::*;
use herdr_client::{CloudProvider, ConnectTarget};

#[gpui::test]
fn all_cloud_providers_are_sources_and_destinations_without_probing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints[0].connection.target = ConnectTarget::Local;
            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.workspaces = crate::sidebar::layout_tests::snapshot(7).workspaces;
            for &provider in CloudProvider::ALL {
                // Invalid credentials deliberately prove that merely listing a
                // device never reads keys, signs in, or waits on the network.
                let target = ConnectTarget::Cloud {
                    provider,
                    account: "unconfigured".into(),
                    id: "saved-machine".into(),
                    machine: "Display name".into(),
                    session: "work".into(),
                };
                view.endpoints[0].connection.target = target.clone();
                view.open_workspace_menu("w4", Default::default(), window, cx);
                assert_eq!(
                    teleport_items(view),
                    usize::from(cfg!(any(target_os = "linux", target_os = "macos"))),
                    "{provider:?} can be a source"
                );
                view.dismiss_menu(window, cx);
                view.endpoints.push(crate::endpoint::Endpoint::new(
                    format!("cloud:{}", provider.key()),
                    crate::cloud::name(provider).into(),
                    target,
                    true,
                ));
            }
            view.endpoints[0].connection.target = ConnectTarget::Local;
            view.open_workspace_menu("w4", Default::default(), window, cx);
            view.activate_workspace_menu(
                super::super::super::WorkspaceMenuAction::Teleport,
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    for i in 0..CloudProvider::ALL.len() {
        let row: &'static str = Box::leak(format!("teleport-host-{i}").into_boxed_str());
        assert!(cx.debug_bounds(row).is_some());
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.dismiss_menu(window, cx);
            view.poll_teleport(window, cx);
        });
    });
}
