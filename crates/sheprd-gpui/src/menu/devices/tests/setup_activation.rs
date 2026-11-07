use super::*;
use crate::{endpoint::Endpoint, state::ConnectionStatus};
use sheprd_client::protocol::ClientShellSnapshot;
use std::sync::Arc;

/// Herdr refuses `workspace.create` from a connection whose surface is
/// inactive, as Local's is while the window shows another device. The setup
/// moves the window to Local, and the window's own poll activates Local's
/// surface before the setup asks for its workspace on the same connection.
#[gpui::test]
fn setup_space_waits_for_the_local_surface(cx: &mut gpui::TestAppContext) {
    let mut peer = crate::window::MockPeer::welcoming(
        &["client_shell.surface.set", "workspace.create"],
        &["surface_interest", "presentation_effects_fence"],
    );
    let (view, cx) = cx.add_window_view(fixture_window);
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let boot = snapshot.boot_id.clone();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(Endpoint::new(
                "remote".into(),
                "Remote".into(),
                sheprd_client::ConnectTarget::Socket("/unused-remote.sock".into()),
                false,
            ));
            view.selected_endpoint = 1;
            let local = &mut view.endpoints[0];
            local.connection.handle = Some(peer.client.handle.clone());
            // The connection's worker keeps its inbox in step with `live`.
            local.live.snapshot = Some(Arc::new(snapshot));
            local.live.status = ConnectionStatus::Connected;
            *local.connection.inbox.lock().unwrap() = local.live.clone();
            assert!(!local.surface_requested());

            let request = setup::Request::new("dev@box", "Box", "").unwrap();
            open_form(view, Step::Verifying(request, Offer::Update), cx);
            view.setup_space_verified(
                Ok((
                    setup::Claim::fixture("activation.test"),
                    Ok(setup::TerminalSetup::fixture(
                        "'herdr-gpui' '__device-setup-attach' 'x'",
                    )),
                )),
                cx,
            );
            assert_eq!(view.selected_endpoint, 0);
            // Switching closes dialogs; this one stays open over Local.
            assert_eq!(view.menu.page, Some(Page::AddDevice));
            let form = view.menu.device_setup.as_ref().unwrap();
            assert!(
                matches!(form.step, Step::Activating(..)),
                "{:?}",
                form.error
            );
            assert!(form.claim.is_some());
            // Nothing is asked of the inactive surface.
            view.poll_device_setup(window, cx);
            assert!(matches!(step(view), Step::Activating(..)));

            // The window's poll queues the activation, then the request.
            view.poll_endpoints(cx);
            assert!(view.endpoints[0].surface_requested());
            view.poll_device_setup(window, cx);
            assert!(matches!(step(view), Step::Opening(..)));
        });
    });
    let activation = peer.request();
    assert_eq!(activation["method"], "client_shell.surface.set");
    assert_eq!(activation["params"]["active"], true);
    let id = activation["id"].as_str().unwrap().to_owned();
    peer.respond(&boot, &id, &serde_json::json!({"id": id, "result": {}}));
    assert_eq!(peer.request()["method"], "workspace.create");
}

#[gpui::test]
fn setup_space_fails_when_local_is_disconnected_elsewhere(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(Endpoint::new(
                "remote".into(),
                "Remote".into(),
                sheprd_client::ConnectTarget::Socket("/unused-remote.sock".into()),
                false,
            ));
            view.selected_endpoint = 1;
            view.endpoints[0].live.status = ConnectionStatus::Disconnected;
            let request = setup::Request::new("dev@box", "Box", "").unwrap();
            open_form(view, Step::Verifying(request, Offer::Update), cx);
            view.setup_space_verified(
                Ok((
                    setup::Claim::fixture("activation-down.test"),
                    Ok(setup::TerminalSetup::fixture(
                        "'herdr-gpui' '__device-setup-attach' 'x'",
                    )),
                )),
                cx,
            );
            // No wait for a surface that cannot come: back to the question.
            view.poll_device_setup(window, cx);
            assert!(matches!(step(view), Step::Confirm(_, Offer::Update)));
            assert!(
                view.menu
                    .device_setup
                    .as_ref()
                    .unwrap()
                    .error
                    .as_deref()
                    .unwrap()
                    .starts_with("Open a local workspace:")
            );
        });
    });
}
