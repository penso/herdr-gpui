use super::super::add_device::{LocalSpace, enter};
use super::*;
use crate::{NavigationTarget, state::ConnectionStatus};
use herdr_client::protocol::{ClientMessage, ClientPaneInputEvent, ClientShellSnapshot};
use std::sync::Arc;

#[gpui::test]
fn install_needs_a_connected_local_daemon(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("dev@box", "Box", "").unwrap();
            view.endpoints[0].live.status = ConnectionStatus::Disconnected;
            open_form(view, Step::Verifying(request, Offer::Install), cx);
            view.setup_space_verified(
                Ok((
                    setup::Claim::fixture("disconnected.test"),
                    Ok(setup::TerminalSetup::fixture(
                        "'herdr-gpui' '__device-setup-attach' 'x'",
                    )),
                )),
                cx,
            );
            // The same offer stays, so trying again does what was accepted.
            assert!(matches!(step(view), Step::Confirm(_, Offer::Install)));
            // The claim stays with the dialog for a retry.
            assert!(view.menu.device_setup.as_ref().unwrap().claim.is_some());
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

#[gpui::test]
fn created_space_runs_setup_in_its_root_pane(cx: &mut gpui::TestAppContext) {
    let mut peer = crate::window::MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let boot = snapshot.boot_id.clone();
    let space = |request: &str| {
        let request = request.to_owned();
        let boot = boot.clone();
        move || LocalSpace {
            request,
            boot,
            setup: setup::TerminalSetup::fixture("'herdr-gpui' '__device-setup-attach' 'x'"),
            offer: Offer::Terminal,
        }
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let local = &mut view.endpoints[0];
            local.connection.handle = Some(peer.client.handle.clone());
            local.live.snapshot = Some(Arc::new(snapshot));
            local.live.status = ConnectionStatus::Connected;
            let request = setup::Request::new("dev@box", "Box", "").unwrap();

            // A refusal returns to the question with the daemon's reason.
            open_form(view, Step::Opening(request.clone(), space("create")()), cx);
            view.endpoints[0].live.dialog_response = Some((
                "create".into(),
                Some(Ok(serde_json::json!({"error":{"code":"denied","message":"no"}}))),
            ));
            view.poll_device_setup(window, cx);
            assert!(matches!(step(view), Step::Confirm(_, Offer::Terminal)));
            assert!(view.menu.device_setup.as_ref().unwrap().error.as_deref().unwrap().replace(['\u{2068}', '\u{2069}'], "").ends_with("denied: no"));

            // Another dialog's response is not this workspace.
            let created = serde_json::json!({"result":{"type":"workspace_created","workspace":{"workspace_id":"w9"},"tab":{"tab_id":"t9"},"root_pane":{"pane_id":"w9:p1"}}});
            open_form(view, Step::Opening(request, space("create")()), cx);
            view.endpoints[0].live.dialog_response =
                Some(("other".into(), Some(Ok(created.clone()))));
            view.poll_device_setup(window, cx);
            assert!(matches!(step(view), Step::Opening(..)));

            view.endpoints[0].live.dialog_response =
                Some(("create".into(), Some(Ok(created))));
            view.poll_device_setup(window, cx);
            assert!(view.menu.page.is_none());
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Workspace("w9".into()))
            );
        });
    });
    assert_eq!(
        peer.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w9:p1".into(),
            events: vec![
                ClientPaneInputEvent::TextCommit("'herdr-gpui' '__device-setup-attach' 'x'".into()),
                enter(),
            ],
        }
    );
}
