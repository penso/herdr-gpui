use super::*;
use herdr_client::SshFailure;

#[gpui::test]
fn a_local_network_denial_returns_to_the_form_to_try_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("penso@10.0.0.19", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(
                Ok((
                    setup::Claim::fixture("local-network-denied"),
                    Ok(HostProbe::SshFailed(SshFailure::LocalNetworkDenied)),
                )),
                cx,
            );
            // A terminal cannot grant this app the permission, so none is offered.
            assert!(matches!(step(view), Step::Form));
            assert!(
                view.menu
                    .error
                    .as_deref()
                    .unwrap()
                    .starts_with("macOS denied Herdr access to your local network")
            );
        });
    });
}
