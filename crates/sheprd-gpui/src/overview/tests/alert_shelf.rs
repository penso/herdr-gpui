//! The Recent alerts shelf and the palette's way to it.

use super::page::{bounds, click, draw, fixture};
use crate::{
    SheprdWindow,
    notifications::{Notice, tests::notification},
};
use gpui::{Entity, VisualTestContext};
use sheprd_client::protocol::SemanticNotificationKind as Kind;
use sheprd_commands::Command;
use std::time::Instant;

/// Delivers one agent alert about a tab the fixture does not show. Delivery is off
/// there, so it is only kept.
fn alert(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let selected = view.selected_endpoint;
            let snapshot = view.live.snapshot.clone().unwrap();
            view.endpoints[selected].live.snapshot = Some(snapshot.clone());
            let mut wire = notification("claude needs you");
            wire.kind = Kind::NeedsAttention;
            wire.tab_id = Some("t1".into());
            wire.workspace_id = Some("w0".into());
            wire.pane_id = None;
            let now = Instant::now();
            let notice = Notice::new(wire, now).with_snapshot(Some(&snapshot));
            view.endpoints[selected].toasts.receive([notice]);
            view.config.notifications.delay_seconds = 0;
            view.tick_toasts(false, now, cx);
            view.poll_overview(cx);
        })
    });
    draw(cx);
}

fn first_alert(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let listed = view.overview_alerts();
        format!(
            "overview-alert-{}-{}",
            listed[0].endpoint, listed[0].alert.id
        )
    })
}

#[gpui::test]
fn no_alerts_show_no_shelf(cx: &mut gpui::TestAppContext) {
    let (_, cx) = fixture(cx);
    assert!(cx.debug_bounds("overview-alerts").is_none());
}

#[gpui::test]
fn a_kept_alert_waits_in_a_shelf_that_folds(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    alert(&view, cx);
    assert!(cx.debug_bounds("overview-alerts").is_some());
    let selector = first_alert(&view, cx);
    assert!(bounds(cx, selector.clone()).is_none(), "it starts folded");
    click(cx, "overview-alerts");
    assert!(bounds(cx, selector).is_some());
}

#[gpui::test]
fn the_palette_command_opens_the_page_on_its_alerts(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    alert(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::NotificationHistory, window, cx)
        })
    });
    draw(cx);
    let selector = first_alert(&view, cx);
    assert!(bounds(cx, selector).is_some());
}

#[gpui::test]
fn an_alert_whose_pane_closed_says_so_and_stays_put(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    alert(&view, cx);
    click(cx, "overview-alerts");
    let selector = first_alert(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.boot_id = "restarted".into();
            view.live.snapshot = Some(std::sync::Arc::new(snapshot));
        })
    });
    click(cx, selector);
    let panel = cx.update(|_, cx| view.read(cx).activity.panel);
    assert_eq!(panel, crate::sidebar::ActivityPanel::Overview);
}

#[gpui::test]
fn clicking_an_alert_goes_to_its_pane(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    alert(&view, cx);
    click(cx, "overview-alerts");
    let selector = first_alert(&view, cx);
    click(cx, selector);
    let panel = cx.update(|_, cx| view.read(cx).activity.panel);
    assert_eq!(panel, crate::sidebar::ActivityPanel::Workspaces);
}

#[gpui::test]
fn alerts_stay_reachable_once_every_agent_has_gone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    alert(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut snapshot = (**view.live.snapshot.as_ref().unwrap()).clone();
            snapshot.agents.clear();
            view.live.snapshot = Some(std::sync::Arc::new(snapshot));
            view.poll_overview(cx);
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("overview-empty").is_none());
    assert!(cx.debug_bounds("overview-alerts").is_some());
}
