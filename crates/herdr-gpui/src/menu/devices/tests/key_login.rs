#[cfg(unix)]
use super::super::add_device::LocalSpace;
use super::*;
use herdr_client::SshFailure;

#[gpui::test]
fn a_host_that_refuses_every_key_offers_to_install_one(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("m5mini.local", "", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(
                Ok((
                    setup::Claim::fixture("password-only"),
                    Ok(HostProbe::SshFailed(SshFailure::Auth)),
                )),
                cx,
            );
            assert!(matches!(step(view), Step::Confirm(_, Offer::InstallKey)));
            // The offer explains it; agent advice does not fit a new host.
            assert!(view.menu.device_setup.as_ref().unwrap().error.is_none());
            assert!(
                Offer::InstallKey
                    .question("m5mini.local")
                    .contains("asks for the password once")
            );
        });
    });
}

#[cfg(unix)]
#[gpui::test]
fn a_failed_setup_workspace_still_offers_to_install_the_key(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("m5mini.local", "", "", "").unwrap();
            let space = LocalSpace {
                request: "create".into(),
                boot: "gone".into(),
                setup: setup::TerminalSetup::fixture("'herdr-gpui' '__device-setup-attach' 'x'"),
                offer: Offer::InstallKey,
            };
            // The local daemon is not connected, so the workspace fails.
            open_form(view, Step::Opening(request, space), cx);
            view.poll_device_setup(window, cx);
            assert!(matches!(step(view), Step::Confirm(_, Offer::InstallKey)));
            assert!(view.menu.device_setup.as_ref().unwrap().error.is_some());
        });
    });
}
