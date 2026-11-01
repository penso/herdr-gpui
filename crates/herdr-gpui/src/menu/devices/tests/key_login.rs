#[cfg(unix)]
use super::super::add_device::LocalSpace;
use super::*;
use herdr_client::SshFailure;

#[gpui::test]
fn a_host_that_refuses_every_key_offers_to_install_one(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("m5mini.local", "", "").unwrap();
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
                    .question("m5mini.local", None)
                    .contains(if cfg!(windows) {
                        "accepted none of your SSH keys"
                    } else {
                        "asks for the password once"
                    })
            );
        });
    });
}

#[gpui::test]
fn a_refused_target_without_a_user_names_the_login_ssh_tried(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("pi4.local", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            // The fixture claim resolves the login to `test`.
            view.device_probed(
                Ok((
                    setup::Claim::fixture("pi4.local"),
                    Ok(HostProbe::SshFailed(SshFailure::Auth)),
                )),
                cx,
            );
            let setup = view.menu.device_setup.as_ref().unwrap();
            let Step::Confirm(request, offer) = &setup.step else {
                panic!("expected the key offer");
            };
            let question = offer
                .question(
                    request.target(),
                    setup.claim.as_ref().map(setup::Claim::user),
                )
                .replace(['\u{2068}', '\u{2069}'], "");
            assert!(question.contains("SSH keys for the user test"));
            assert!(question.contains("name@pi4.local"));
        });
    });
}

#[test]
fn a_target_that_names_its_user_keeps_the_plain_key_offer() {
    for target in ["pi@pi4.local", "ssh://pi@pi4.local:2222"] {
        let question = Offer::InstallKey.question(target, Some("pi"));
        assert!(question.contains("accepted none of your SSH keys"));
        assert!(!question.contains("for the user"), "{question}");
        assert!(!question.contains("name@"), "{question}");
    }
    // A URI takes the user inside it, not before the scheme.
    let question = Offer::InstallKey.question("ssh://pi4.local:2222", Some("mac"));
    assert!(question.contains("ssh://name@pi4.local:2222"), "{question}");
    assert!(!question.contains("name@ssh://"), "{question}");
    // Nothing resolved: nothing to name.
    assert!(
        !Offer::InstallKey
            .question("pi4.local", None)
            .contains("for the user")
    );
}

#[gpui::test]
fn changing_the_target_withdraws_the_key_offer(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let target = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_add_device(window, cx);
            let setup = view.menu.device_setup.as_mut().unwrap();
            setup.step = Step::Confirm(
                setup::Request::new("pi4.local", "", "").unwrap(),
                Offer::InstallKey,
            );
            setup.claim = Some(setup::Claim::fixture("pi4.local"));
            setup.fields[0].clone()
        })
    });
    // Text that trims to the checked target keeps the offer.
    cx.update(|_, cx| target.update(cx, |field, cx| field.set_text_selected("pi4.local ", cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        assert!(matches!(setup.step, Step::Confirm(_, Offer::InstallKey)));
    });
    // Another account: submitting must check it, not install a key for the
    // account that was refused.
    cx.update(|_, cx| target.update(cx, |field, cx| field.set_text_selected("pi@pi4.local", cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        assert!(matches!(setup.step, Step::Form));
        assert!(setup.claim.is_none());
    });
}

#[cfg(unix)]
#[gpui::test]
fn a_failed_setup_workspace_still_offers_to_install_the_key(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("m5mini.local", "", "").unwrap();
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
