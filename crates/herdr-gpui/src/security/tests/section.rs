//! The Security icon is always in the bar, before any read finishes, and
//! the section draws what the last read found.
use super::*;
use crate::sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw};
use gpui::{AppContext as _, Entity, VisualTestContext, px, size};

pub(super) fn window(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx)
}

pub(super) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
    });
}

pub(super) fn install(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, repos: Vec<Repo>) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.security
                .install(repos, Instant::now(), SystemTime::now());
        });
    });
    draw(cx);
}

#[gpui::test]
fn the_icon_is_listed_before_any_read(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    assert!(cx.debug_bounds("activity-security").is_some());
    install(&view, cx, vec![repo("a/off", Ok(Alerts::Off))]);
    assert!(cx.debug_bounds("activity-security").is_some());
}

#[gpui::test]
fn asking_for_the_section_shows_its_icon_and_explains_itself(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Security, window, cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("activity-security").is_some());
    assert!(cx.debug_bounds("security-section").is_some());
    assert!(cx.debug_bounds("security-section-empty").is_some());
}

#[gpui::test]
fn the_section_lists_alerts_and_names_quiet_off_and_failed_repositories(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = window(cx);
    install(
        &view,
        cx,
        vec![
            repo(
                "a/alerts",
                Ok(open(vec![
                    alert(1, Severity::Critical),
                    alert(2, Severity::Low),
                ])),
            ),
            repo("a/quiet", Ok(open(Vec::new()))),
            repo("a/off", Ok(Alerts::Off)),
            repo("a/failed", Err(Error::Output)),
        ],
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Security, window, cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("security-repo-0").is_some());
    assert!(cx.debug_bounds("security-alert").is_some());
    assert!(cx.debug_bounds("security-section-quiet").is_some());
    assert!(cx.debug_bounds("security-section-off").is_some());
    assert!(cx.debug_bounds("security-section-failed").is_some());
    assert!(cx.debug_bounds("security-section-empty").is_none());
    // The section holds the keyboard, so typing never reaches a hidden pane.
    cx.update(|window, cx| {
        let focus = view.read(cx).activity.section_focus.clone().unwrap();
        assert!(focus.contains_focused(window, cx));
    });
}

#[gpui::test]
fn reports_lead_the_section_and_a_hidden_queue_is_named(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    install(
        &view,
        cx,
        vec![
            reported(
                "a/reported",
                vec![report(
                    "GHSA-aaaa-bbbb-cccc",
                    Some(Severity::High),
                    "2026-10-05T00:00:00Z",
                )],
            ),
            Repo {
                reports: Ok(Reports::Hidden),
                ..repo("a/hidden", Ok(open(Vec::new())))
            },
        ],
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Security, window, cx);
        });
    });
    draw(cx);
    let reported = cx.debug_bounds("security-reported").unwrap();
    assert!(cx.debug_bounds("security-report").is_some());
    let quiet = cx.debug_bounds("security-section-quiet").unwrap();
    assert!(reported.origin.y < quiet.origin.y);
    assert!(cx.debug_bounds("security-section-hidden").is_some());
}
