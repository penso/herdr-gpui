use super::*;
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

#[gpui::test]
fn account_poll_notifies_for_secondary_rate_limits_while_disconnected(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let now = Instant::now();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = crate::github::Auth::connected_fixture();
            view.live.status = crate::state::ConnectionStatus::Disconnected;
            view.menu
                .pr_cache
                .seed_rate_limit("fixture-user", now + Duration::from_secs(120));
            view.update_workspace_pr();
            assert_eq!(view.menu.github.quota(), None);
            view.poll_github_at(now, window, cx);
        })
    });
    let notifications = Rc::new(Cell::new(0));
    let seen = notifications.clone();
    let _subscription = cx.update(|_, cx| cx.observe(&view, move |_, _| seen.set(seen.get() + 1)));
    for (seconds, redraw) in [(60, true), (61, false), (120, true), (121, false)] {
        notifications.set(0);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert!(!view.live.status.is_connected());
                view.update_workspace_pr();
                view.poll_github_at(now + Duration::from_secs(seconds), window, cx);
            })
        });
        assert_eq!(
            notifications.get() > 0,
            redraw,
            "account poll at {seconds}s"
        );
    }
}
