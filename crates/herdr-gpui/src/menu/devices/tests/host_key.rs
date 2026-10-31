use super::*;
use herdr_client::{HostKeyScan, SshFailure};

fn fetching(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) -> setup::Request {
    let request = setup::Request::new("dev@box", "", "").unwrap();
    open_form(view, Step::FetchingKey(request.clone()), cx);
    request
}

#[gpui::test]
fn an_unknown_host_key_is_fetched_for_approval(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("dev@box", "", "").unwrap();
            open_form(view, Step::Checking(request), cx);
            view.device_probed(
                Ok((
                    setup::Claim::fixture("unknown-host-key"),
                    Ok(HostProbe::SshFailed(SshFailure::HostKey)),
                )),
                cx,
            );
            // No terminal yet: the dialog fetches the key, holding the host.
            assert!(
                matches!(step(view), Step::FetchingKey(request) if request.target() == "dev@box")
            );
            let setup = view.menu.device_setup.as_ref().unwrap();
            assert!(setup.claim.is_some());
            assert!(setup.task.is_some());
            assert!(view.menu.device_setup.as_ref().unwrap().error.is_none());
        });
    });
}

#[gpui::test]
fn a_changed_host_key_is_never_offered_for_approval(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            fetching(view, cx);
            view.host_key_fetched(Ok(HostKeyScan::Refused(SshFailure::HostKey)), cx);
            assert!(matches!(step(view), Step::Confirm(_, Offer::Terminal)));
            assert!(
                view.menu
                    .device_setup
                    .as_ref()
                    .unwrap()
                    .error
                    .as_deref()
                    .unwrap()
                    .replace(['\u{2068}', '\u{2069}'], "")
                    .starts_with("The host key of dev@box differs from the one you trusted before")
            );
        });
    });
}

#[gpui::test]
fn a_failed_key_fetch_offers_the_terminal_with_its_reason(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            fetching(view, cx);
            view.host_key_fetched(Ok(HostKeyScan::Refused(SshFailure::Unreachable)), cx);
            assert!(matches!(step(view), Step::Confirm(_, Offer::Terminal)));
            let error = |view: &HerdrWindow| {
                view.menu
                    .device_setup
                    .as_ref()
                    .unwrap()
                    .error
                    .as_deref()
                    .map(|error| error.replace(['\u{2068}', '\u{2069}'], ""))
            };
            assert_eq!(
                error(view).as_deref(),
                Some("SSH to dev@box: the host is unreachable")
            );

            fetching(view, cx);
            view.host_key_fetched(
                Err(herdr_client::Error::Ssh(herdr_client::ssh::Error::Timeout)),
                cx,
            );
            assert!(matches!(step(view), Step::Confirm(_, Offer::Terminal)));
            assert_eq!(
                error(view).as_deref(),
                Some("Fetch the host key of dev@box: SSH discovery timed out")
            );
        });
    });
}

#[gpui::test]
fn a_key_fetch_for_a_closed_or_changed_dialog_is_ignored(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            open_form(view, Step::Form, cx);
            view.host_key_fetched(Ok(HostKeyScan::Refused(SshFailure::HostKey)), cx);
            assert!(matches!(step(view), Step::Form));
            assert!(view.menu.device_setup.as_ref().unwrap().error.is_none());
        });
    });
}

#[gpui::test]
fn cancel_gives_up_the_host_so_it_can_be_tried_again(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("dev@box", "", "").unwrap();
            open_form(view, Step::Confirm(request, Offer::Terminal), cx);
            view.menu.device_setup.as_mut().unwrap().claim =
                Some(setup::Claim::fixture("cancel-retry"));
            view.set_device_error(Some("SSH to dev@box: refused".into()));
            view.cancel_device_step(cx);
            assert!(matches!(step(view), Step::Form));
            // A held claim would refuse the next submit as `Error::Adding`.
            assert!(view.menu.device_setup.as_ref().unwrap().claim.is_none());
            assert!(view.menu.device_setup.as_ref().unwrap().error.is_none());
        });
    });
}
