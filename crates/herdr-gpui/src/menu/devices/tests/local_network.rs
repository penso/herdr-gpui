use super::*;
use herdr_client::SshFailure;

fn denied(name: &str) -> crate::Result<(setup::Claim, crate::Result<HostProbe>)> {
    Ok((
        setup::Claim::fixture(name),
        Ok(HostProbe::SshFailed(SshFailure::LocalNetworkDenied)),
    ))
}

/// macOS refuses the first local network operation while its alert is up, so
/// a refused check waits for the user, keeping the host, instead of failing.
/// The wait itself runs on the background executor, which these tests never
/// drive, so they do not touch the network.
#[gpui::test]
fn a_local_network_refusal_waits_for_the_user(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("penso@10.0.0.19", "", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(denied("local-network-wait"), cx);
            assert!(matches!(step(view), Step::AwaitingLocalNetwork(_)));
            assert!(view.menu.device_setup.as_ref().unwrap().error.is_none());
            let form = view.menu.device_setup.as_ref().unwrap();
            assert!(form.task.is_some());
            assert!(
                super::super::local_network::waiting_status("penso@10.0.0.19")
                    .contains("Privacy & Security › Local Network")
            );

            // Cancelling ends the wait, and with it the claim it holds.
            view.cancel_device_step(cx);
            assert!(matches!(step(view), Step::Form));
            assert!(view.menu.device_setup.as_ref().unwrap().task.is_none());
        });
    });
}

#[gpui::test]
fn a_refusal_after_the_wait_returns_to_the_form(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("penso@10.0.0.20", "", "", "").unwrap();
            open_form(view, Step::AwaitingLocalNetwork(request), cx);
            view.device_probed(denied("local-network-denied"), cx);
            // A terminal cannot grant this app the permission, so none is offered.
            assert!(matches!(step(view), Step::Form));
            assert!(
                view.menu
                    .device_setup
                    .as_ref()
                    .unwrap()
                    .error
                    .as_deref()
                    .unwrap()
                    .starts_with("macOS denied Herdr access to your local network")
            );
        });
    });
}

/// Once allowed, the second check's answer is handled like any first one.
#[gpui::test]
fn an_allowed_wait_continues_the_setup(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("penso@10.0.0.21", "", "", "").unwrap();
            open_form(view, Step::AwaitingLocalNetwork(request), cx);
            view.device_probed(
                Ok((
                    setup::Claim::fixture("local-network-allowed"),
                    Ok(HostProbe::Missing),
                )),
                cx,
            );
            assert!(matches!(step(view), Step::Confirm(_, Offer::Install)));
        });
    });
}
