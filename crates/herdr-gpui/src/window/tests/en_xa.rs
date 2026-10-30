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
use herdr_client::protocol::ClientShellSnapshot;

/// What the search surface types into the sidebar's field.
const QUERY: &str = "sidebar";

/// The names the snapshot gives the window to show: workspaces, their
/// branches and repositories, tabs, panes, and agents. A linked worktree's
/// row names it by its branch's last segment.
fn snapshot_names(snapshot: &ClientShellSnapshot) -> Vec<String> {
    let mut names = Vec::new();
    for workspace in &snapshot.workspaces {
        names.push(workspace.label.clone());
        names.extend(
            workspace
                .worktree
                .iter()
                .map(|worktree| worktree.label.clone()),
        );
        if let Some(branch) = &workspace.branch {
            names.push(branch.clone());
            names.extend(branch.rsplit('/').next().map(str::to_owned));
        }
    }
    for tab in &snapshot.tabs {
        names.push(herdr_client::text::tab_label(snapshot, tab).to_owned());
    }
    names.extend(snapshot.panes.iter().filter_map(|pane| pane.label.clone()));
    for agent in &snapshot.agents {
        names.extend(
            [
                &agent.name,
                &agent.display_agent,
                &agent.agent,
                &agent.title,
            ]
            .into_iter()
            .flatten()
            .cloned(),
        );
    }
    names
}

/// The lines in `shown` that are data rather than messages: the snapshot's
/// names, whole, cut short with an ellipsis, or measured a character at a
/// time, the search query, the app's version, the build's
/// branch on the worktree banner, each endpoint's label, and what is drawn
/// only to be measured.
fn data(view: &HerdrWindow, shown: &[String]) -> Vec<String> {
    let mut names = view
        .live
        .snapshot
        .as_deref()
        .map(snapshot_names)
        .unwrap_or_default();
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
            // The search field echoes the query as it is typed.
            QUERY.starts_with(line)
                || names.iter().any(|name| match line.strip_suffix('…') {
                    Some(cut) => name.starts_with(cut),
                    // Truncation measures a name one character at a time.
                    None if line.chars().count() == 1 => name.contains(line),
                    None => name == line,
                })
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
        cx.simulate_input(QUERY);
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
