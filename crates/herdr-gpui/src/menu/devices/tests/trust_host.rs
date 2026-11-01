use super::super::host_key::Fetched;
use super::super::trust_host::{Review, Step};
use super::*;
use herdr_client::{ConnectTarget, HostKeyScan, SshFailure};

/// Selects a saved SSH device that SSH refused for `failure`.
fn select_refused(view: &mut HerdrWindow, failure: SshFailure) {
    let mut remote = crate::endpoint::Endpoint::new(
        "ssh:box".into(),
        "Remote".into(),
        ConnectTarget::Ssh {
            target: "dev@box".into(),
            session: "default".into(),
        },
        // Disabled, so a redial after trusting never runs SSH in a test.
        false,
    );
    remote.live.ssh_failure = Some(failure);
    view.endpoints.push(remote);
    view.selected_endpoint = view.endpoints.len() - 1;
}

/// Opens the review as `review_host_key` does, without fetching the key.
fn open_review(view: &mut HerdrWindow) {
    view.menu.host_key = Some(Review::fixture("ssh:box", "dev@box"));
    view.menu.page = Some(Page::HostKey);
}

fn review_step(view: &HerdrWindow) -> &Step {
    &view.menu.host_key.as_ref().unwrap().step
}

#[gpui::test]
fn only_a_refused_host_key_offers_the_review(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // This device has no SSH target to review.
            assert_eq!(view.untrusted_host(), None);
            select_refused(view, SshFailure::Auth);
            assert_eq!(view.untrusted_host(), None);
            select_refused(view, SshFailure::HostKey);
            assert_eq!(view.untrusted_host(), Some("dev@box"));
        });
    });
}

#[gpui::test]
fn enter_never_replaces_and_failures_offer_nothing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            select_refused(view, SshFailure::HostKey);
            open_review(view);
            view.host_key_reviewed(
                Err(herdr_client::Error::Ssh(
                    herdr_client::ssh::Error::KeyNotReplaceable,
                )),
                window,
                cx,
            );
            assert!(matches!(review_step(view), Step::Failed(_)));
            // Neither Enter nor Replace saves anything without a key on offer.
            view.trust_reviewed_host_key(false, window, cx);
            view.trust_reviewed_host_key(true, window, cx);
            assert!(matches!(review_step(view), Step::Failed(_)));
            assert_eq!(view.menu.page, Some(Page::HostKey));
        });
    });
}

#[gpui::test]
fn a_failed_fetch_shows_its_reason(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            select_refused(view, SshFailure::HostKey);
            open_review(view);
            view.host_key_reviewed(
                Ok(Fetched::Scan(HostKeyScan::Refused(SshFailure::Unreachable))),
                window,
                cx,
            );
            assert!(
                matches!(review_step(view), Step::Failed(error) if error == "the host is unreachable")
            );

            open_review(view);
            view.host_key_reviewed(
                Err(herdr_client::Error::Ssh(herdr_client::ssh::Error::Timeout)),
                window,
                cx,
            );
            assert!(
                matches!(review_step(view), Step::Failed(error) if error == "SSH discovery timed out")
            );
        });
    });
}

#[gpui::test]
fn a_key_trusted_meanwhile_closes_the_review(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            select_refused(view, SshFailure::HostKey);
            open_review(view);
            view.host_key_reviewed(Ok(Fetched::Scan(HostKeyScan::Trusted)), window, cx);
            assert_eq!(view.menu.page, None);
            assert!(view.menu.host_key.is_none());
            let (flash, _) = view.flash.as_ref().unwrap();
            assert_eq!(
                flash.text.replace(['\u{2068}', '\u{2069}'], ""),
                "Trusted the host key of dev@box; reconnecting"
            );
        });
    });
}

#[gpui::test]
fn a_result_for_a_closed_review_is_ignored(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            select_refused(view, SshFailure::HostKey);
            view.host_key_reviewed(Ok(Fetched::Scan(HostKeyScan::Trusted)), window, cx);
            assert!(view.flash.is_none());
            view.host_key_trusted(Ok(()), window, cx);
            assert!(view.flash.is_none());

            // A save result arriving for a review that is not saving.
            open_review(view);
            view.host_key_trusted(Ok(()), window, cx);
            assert!(matches!(review_step(view), Step::Fetching));
        });
    });
}
