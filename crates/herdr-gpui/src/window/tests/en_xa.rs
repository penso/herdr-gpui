//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the main window's
//! chrome: the sidebar in its layouts, search results, the title bar, the
//! tab strip, and the status bar draw only translated messages, apart from
//! names the snapshot carries. The collapsed rail draws marks, not text;
//! its tooltips are checked in `sidebar::rail::tests`.
use super::*;
use crate::config::LayoutMode;
use crate::i18n::leak_tests::{Recorder, assert_translated, draw, under_en_xa};
use crate::sidebar::layout_tests::{SidebarFixture, full_draw};
use crate::state::ConnectionStatus;
use gpui::{AppContext as _, Entity, VisualTestContext, px, size};

/// Every string the snapshot holds: workspace, branch, tab, and agent names.
fn snapshot_text(value: &serde_json::Value, text: &mut Vec<String>) {
    match value {
        serde_json::Value::String(string) => text.push(string.clone()),
        serde_json::Value::Array(values) => values.iter().for_each(|v| snapshot_text(v, text)),
        serde_json::Value::Object(map) => map.values().for_each(|v| snapshot_text(v, text)),
        _ => {}
    }
}

/// The lines in `shown` that are data rather than messages: the snapshot's
/// names, whole or cut short with an ellipsis, the app's version, the build's
/// branch on the worktree banner, each endpoint's label, and what is drawn
/// only to be measured.
fn data(view: &HerdrWindow, shown: &[String]) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(snapshot) = view.live.snapshot.as_deref() {
        snapshot_text(&serde_json::to_value(snapshot).unwrap(), &mut names);
    }
    names.extend(view.endpoints.iter().map(|endpoint| endpoint.label.clone()));
    names.push(herdr_updater::APP_VERSION.to_owned());
    names.push(env!("HERDR_BUILD_BRANCH").to_owned());
    // The device menu's footer, migrated with menu/.
    names.push("All Devices".to_owned());
    // The terminal measures its cell width on one shaped "M".
    names.push("M".to_owned());
    shown
        .iter()
        .map(|line| line.trim())
        .filter(|line| {
            let cut = line.trim_end_matches('…');
            names.iter().any(|name| name.contains(cut))
        })
        .map(str::to_owned)
        .collect()
}

fn check(
    surface: &str,
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    recorder: &Recorder,
) {
    cx.update(|window, _| window.refresh());
    draw(cx);
    let shown = recorder.take();
    let data = cx.update(|_, cx| data(view.read(cx), &shown));
    assert_translated(surface, &shown, &data);
}

#[test]
fn the_main_window_draws_only_messages() {
    under_en_xa("main window en-XA leak", |cx, recorder| {
        let (fixture, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut view = fixture_window(window, cx);
                view.live.snapshot = Some(Arc::new(snapshot(6)));
                view.live.status = ConnectionStatus::Connected;
                view.endpoints[0].live = view.live.clone();
                view
            });
            SidebarFixture(view)
        });
        let view = cx.update(|_, cx| fixture.read(cx).0.clone());
        cx.simulate_resize(size(px(1200.), px(800.)));
        check("Workspaces", &view, cx, recorder);

        view.update(cx, |view, cx| {
            view.config.layout.mode = LayoutMode::Devices;
            cx.notify();
        });
        check("Devices", &view, cx, recorder);

        view.update(cx, |view, cx| {
            view.config.layout.mode = LayoutMode::default();
            cx.notify();
        });
        cx.update(|window, cx| {
            let focus = view.read(cx).sidebar_search.focus(cx);
            window.focus(&focus, cx);
            full_draw(window, cx).clear(cx);
        });
        recorder.take();
        cx.simulate_input("sidebar");
        cx.run_until_parked();
        check("Search results", &view, cx, recorder);

        view.update(cx, |view, cx| {
            view.live.snapshot = None;
            view.live.status = ConnectionStatus::Disconnected;
            view.endpoints[0].live = view.live.clone();
            cx.notify();
        });
        check("Disconnected", &view, cx, recorder);
    });
}
