use super::*;

#[gpui::test]
fn custom_command_failures_never_fall_back_to_system_ssh_setup(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        for probe in [
            HostProbe::Missing,
            HostProbe::Outdated,
            HostProbe::SshFailed(herdr_client::SshFailure::Auth),
            HostProbe::SshFailed(herdr_client::SshFailure::HostKey),
        ] {
            let request = setup::Request::new("user@box", "Box", "", "tailscale ssh").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(
                Ok((setup::Claim::fixture("custom-command-refusal"), Ok(probe))),
                cx,
            );
            assert!(matches!(step(view), Step::Form));
            let form = view.menu.device_setup.as_ref().unwrap();
            assert!(
                form.claim.is_none(),
                "failure releases the host for a retry"
            );
            assert!(form.task.is_none(), "no installer or key scan may start");
        }
        let request = setup::Request::new("user@box", "", "", "/missing/ssh").unwrap();
        open_form(view, Step::Checking(request), cx);
        view.device_probed(
            Err(std::io::Error::from(std::io::ErrorKind::NotFound).into()),
            cx,
        );
        assert!(matches!(step(view), Step::Form));
        assert!(view.menu.device_setup.as_ref().unwrap().task.is_none());
    });
}

#[gpui::test]
fn keyboard_navigation_reaches_the_command_and_wraps_in_both_directions(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            open_form(view, Step::Form, cx);
            let form = view.menu.device_setup.as_ref().unwrap();
            let focus = form.fields[0].read(cx).focus.clone();
            window.focus(&focus, cx);
        });
    });
    for index in [1, 2, 3, 0] {
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| {
            let form = view.read(cx).menu.device_setup.as_ref().unwrap();
            assert!(form.fields[index].read(cx).focus.is_focused(window));
        });
    }
    cx.simulate_keystrokes("shift-tab");
    cx.update(|window, cx| {
        let form = view.read(cx).menu.device_setup.as_ref().unwrap();
        assert!(form.fields[3].read(cx).focus.is_focused(window));
    });
}
