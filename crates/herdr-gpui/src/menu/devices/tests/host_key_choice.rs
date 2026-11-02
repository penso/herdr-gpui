use super::*;
use herdr_client::HostKeys;

fn host_keys(view: &HerdrWindow) -> HostKeys {
    view.menu.device_setup.as_ref().unwrap().host_keys
}

#[gpui::test]
fn the_form_verifies_host_keys_until_the_user_opts_out(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            open_form(view, Step::Form, cx);
            assert_eq!(host_keys(view), HostKeys::Verify);
            view.toggle_host_key_choice(cx);
            assert_eq!(host_keys(view), HostKeys::Skip);
            view.toggle_host_key_choice(cx);
            assert_eq!(host_keys(view), HostKeys::Verify);
        });
    });
}

#[gpui::test]
fn a_submitted_request_keeps_the_choice_it_was_checked_with(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let request = setup::Request::new("dev.box.coder", "", "")
                .unwrap()
                .with_host_keys(HostKeys::Skip);
            assert_ne!(
                request,
                setup::Request::new("dev.box.coder", "", "").unwrap()
            );
            open_form(view, Step::Checking(request), cx);
            // Changing the box while the host is checked would not reach
            // the request being checked, so it does not change.
            view.toggle_host_key_choice(cx);
            assert_eq!(host_keys(view), HostKeys::Verify);
        });
    });
}
