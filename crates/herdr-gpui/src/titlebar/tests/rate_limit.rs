use super::*;
use std::time::{Duration, Instant};

#[gpui::test]
fn rate_limit_dot_and_dialog_survive_narrow_layout_and_clear_after_reset(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(header_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.menu.github = crate::github::Auth::connected_fixture();
            view.menu.github.seed_quota(herdr_github::quota::Quota {
                limit: 5000,
                used: 5000,
                remaining: 0,
                reset: None,
            });
            view.menu.pr_cache.seed_rate_limit(
                "fixture-user",
                Instant::now() + Duration::from_secs(29 * 60),
            );
            assert!(view.github_rate_limit_message().unwrap().contains("29"));
            cx.notify();
        })
    });
    for width in [1200., 360.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let button = cx.debug_bounds("titlebar-avatar").unwrap();
        let dot = cx.debug_bounds("titlebar-avatar-attention").unwrap();
        assert!(button.contains(&dot.center()));
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_profile(false, window, cx)));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("github-rate-limit").is_some());
        assert!(cx.debug_bounds("github-quota").is_some());
        cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.menu
                .pr_cache
                .seed_rate_limit("fixture-user", Instant::now());
            assert!(view.github_rate_limit_message().is_none());
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("titlebar-avatar-attention").is_none());
}
