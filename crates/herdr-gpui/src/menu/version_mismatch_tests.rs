#![allow(clippy::unwrap_used)]

use super::{Page, version_mismatch::UPDATE_COMMAND};
use crate::sidebar::layout_tests::{fixture_window, full_draw};
use herdr_client::VersionMismatch;

fn outdated(version: Option<&str>) -> VersionMismatch {
    VersionMismatch::DaemonOutdated {
        server_version: version.map(Into::into),
    }
}

#[gpui::test]
fn a_refused_handshake_is_announced_once_and_copies_the_command(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // Nothing to announce for an endpoint that was never refused.
            view.announce_version_mismatch(window, cx);
            assert_eq!(view.menu.page, None);
            let selected = view.selected_endpoint;
            view.endpoints[selected].version_mismatch = Some(outdated(Some("0.8.2")));
            view.announce_version_mismatch(window, cx);
            assert_eq!(view.menu.page, Some(Page::VersionMismatch));
            assert_eq!(
                view.menu.version_notice.as_ref().map(|n| &n.mismatch),
                Some(&outdated(Some("0.8.2")))
            );
        });
        full_draw(window, cx).clear(cx);
    });
    for selector in [
        "version-mismatch-title",
        "version-mismatch-body",
        "version-mismatch-command",
        "version-mismatch-action",
        "version-mismatch-dismiss",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing {selector}");
    }
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(UPDATE_COMMAND.into())
        );
        view.update(cx, |view, cx| {
            assert_eq!(view.menu.page, None);
            assert_eq!(view.menu.version_notice, None);
            // Retries refused again do not reopen it.
            view.announce_version_mismatch(window, cx);
            assert_eq!(view.menu.page, None);
            // An accepted handshake rearms it for a later refusal.
            let selected = view.selected_endpoint;
            view.endpoints[selected].version_mismatch = None;
            view.announce_version_mismatch(window, cx);
            view.endpoints[selected].version_mismatch = Some(outdated(None));
            view.announce_version_mismatch(window, cx);
            assert_eq!(view.menu.page, Some(Page::VersionMismatch));
        });
    });
}

#[gpui::test]
fn a_newer_daemon_offers_the_app_update(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let selected = view.selected_endpoint;
            view.endpoints[selected].version_mismatch = Some(VersionMismatch::ClientOutdated {
                server_version: Some("2.0.0".into()),
            });
            view.announce_version_mismatch(window, cx);
        });
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("version-mismatch-command").is_none());
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::AppUpdate)));
}
