//! Pane names in every sidebar layout: renamed and unnamed panes, blank
//! names, repeated agent names, and long names at narrow widths.
use super::*;
use crate::config::{AgentsPlacement, Density, LayoutMode};
use crate::preferences::AgentSort;
use std::collections::BTreeMap;

const SIDEBAR: f32 = 320.;
/// The narrowest sidebar `sidebar_width` allows.
const NARROWEST: f32 = 160.;

fn modes() -> impl IntoIterator<Item = LayoutMode> {
    [
        LayoutMode::from(Density::Comfortable),
        LayoutMode::Superset,
        LayoutMode::Orca,
        LayoutMode::Minimal,
    ]
}

/// Point every fixture agent at one workspace and one agent name, then give
/// each the pane label and title under test.
fn prepare(snapshot: &mut ClientShellSnapshot, labels: &[Option<&str>], titles: &[Option<&str>]) {
    for agent in &mut snapshot.agents {
        agent.workspace_id = "w0".into();
        agent.tab_id = "t0".into();
        agent.display_agent = Some("grok".into());
        agent.name = Some("grok".into());
        agent.agent = Some("grok".into());
        agent.title = None;
    }
    let ids: Vec<_> = snapshot
        .agents
        .iter()
        .map(|agent| agent.pane_id.clone())
        .collect();
    for (agent, title) in snapshot.agents.iter_mut().zip(titles) {
        agent.title = title.map(str::to_owned);
    }
    snapshot.panes = ids
        .into_iter()
        .zip(labels)
        .map(|(pane_id, label)| ClientShellPane {
            pane_id,
            workspace_id: "w0".into(),
            tab_id: "t0".into(),
            label: label.map(str::to_owned),
            cwd: None,
            foreground_cwd: None,
            focused: false,
            right_click_passthrough: false,
        })
        .collect();
}

fn window(
    cx: &mut gpui::TestAppContext,
    snapshot: ClientShellSnapshot,
) -> (Entity<SheprdWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot));
        view
    });
    cx.simulate_resize(size(px(800.), px(1400.)));
    cx.run_until_parked();
    (view, cx)
}

fn relabel(
    view: &Entity<SheprdWindow>,
    cx: &mut gpui::VisualTestContext,
    labels: &[Option<&str>],
    titles: &[Option<&str>],
) {
    view.update(cx, |view, cx| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        prepare(snapshot, labels, titles);
        cx.notify();
    });
}

/// `count` agents in one workspace, each with its own pane label.
fn crowded(count: usize, label: impl Fn(usize) -> Option<String>) -> ClientShellSnapshot {
    let mut snapshot = snapshot(1);
    let template = snapshot.agents[0].clone();
    snapshot.agents = (0..count)
        .map(|index| {
            let mut agent = template.clone();
            agent.pane_id = format!("p{index}");
            agent
        })
        .collect();
    let labels: Vec<Option<String>> = (0..count).map(label).collect();
    let labels: Vec<Option<&str>> = labels.iter().map(|label| label.as_deref()).collect();
    prepare(&mut snapshot, &labels, &vec![None; count]);
    snapshot
}

fn show(
    view: &Entity<SheprdWindow>,
    mode: LayoutMode,
    width: f32,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| {
        view.config.layout.mode = mode;
        view.config.show_agents = true;
        view.config.agents_placement = AgentsPlacement::Panel;
        view.config.usage.inline = false;
        view.agent_sort = AgentSort::Grouped;
        view.sidebar_width = Some(width);
        cx.notify();
    });
}

/// Every painted text with where it landed and what it rendered.
fn probes(cx: &mut gpui::VisualTestContext) -> BTreeMap<String, (Bounds<Pixels>, String, Pixels)> {
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        full_draw(window, cx).clear(cx);
        cx.global::<TextProbes>().0.clone()
    })
}

fn painted(cx: &mut gpui::VisualTestContext) -> Vec<String> {
    probes(cx).into_keys().collect()
}

fn mentions(painted: &[String], text: &str) -> bool {
    painted.iter().any(|label| label.contains(text))
}

fn row_bounds(cx: &mut gpui::VisualTestContext, index: usize) -> Option<Bounds<Pixels>> {
    // Debug selectors are static; these few leaked strings live for the test run.
    let column: &'static str = Box::leak(format!("column-agent-p{index}").into_boxed_str());
    let row: &'static str = Box::leak(format!("row-agent-p{index}").into_boxed_str());
    cx.debug_bounds(column).or_else(|| cx.debug_bounds(row))
}

#[gpui::test]
fn grouped_agent_rows_show_each_panes_name(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, snapshot(1));
    relabel(
        &view,
        cx,
        &[Some("api"), Some("web")],
        &[Some("from-title"), Some("from-title")],
    );
    for mode in modes() {
        show(&view, mode, SIDEBAR, cx);
        let painted = painted(cx);
        assert!(
            mentions(&painted, "api") && mentions(&painted, "web"),
            "{mode:?} should name both panes: {painted:?}"
        );
        assert!(
            !mentions(&painted, "from-title"),
            "{mode:?} should prefer the rename over the title: {painted:?}"
        );
    }

    relabel(
        &view,
        cx,
        &[None, None],
        &[Some("alpha-title"), Some("beta-title")],
    );
    for mode in modes() {
        show(&view, mode, SIDEBAR, cx);
        let painted = painted(cx);
        assert!(
            mentions(&painted, "alpha-title") && mentions(&painted, "beta-title"),
            "{mode:?} should use an unnamed pane's title: {painted:?}"
        );
    }
}

#[gpui::test]
fn blank_and_whitespace_names_add_nothing_in_every_layout(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, snapshot(1));
    let paint_with = |view: &Entity<SheprdWindow>,
                      cx: &mut gpui::VisualTestContext,
                      labels: [Option<&str>; 2],
                      titles: [Option<&str>; 2]| {
        relabel(view, cx, &labels, &titles);
        modes()
            .into_iter()
            .map(|mode| {
                show(view, mode, SIDEBAR, cx);
                let mut painted = painted(cx);
                painted.sort();
                painted
            })
            .collect::<Vec<_>>()
    };
    let none = paint_with(&view, cx, [None, None], [None, None]);
    let blank = paint_with(&view, cx, [Some(""), Some("   \t ")], [Some(" "), Some("")]);
    // Blank names must paint exactly what no names paint: no empty label, no
    // extra separator.
    assert_eq!(blank, none);
    for mode in modes() {
        show(&view, mode, SIDEBAR, cx);
        let painted = painted(cx);
        assert!(
            row_bounds(cx, 0).is_some() && row_bounds(cx, 1).is_some(),
            "{mode:?}"
        );
        assert!(
            painted
                .iter()
                .all(|text| !text.trim().is_empty() || text == " \u{b7} "),
            "{mode:?}"
        );
    }
}

#[gpui::test]
fn a_name_that_repeats_the_agent_is_painted_once_in_every_layout(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, snapshot(1));
    let paint_with = |view: &Entity<SheprdWindow>,
                      cx: &mut gpui::VisualTestContext,
                      labels: [Option<&str>; 2]| {
        relabel(view, cx, &labels, &[None, None]);
        modes()
            .into_iter()
            .map(|mode| {
                show(view, mode, SIDEBAR, cx);
                let probes = probes(cx);
                let panes = [
                    cx.debug_bounds("pane-agent-p0"),
                    cx.debug_bounds("pane-agent-p1"),
                ];
                (mode, probes, panes)
            })
            .collect::<Vec<_>>()
    };
    let unnamed = paint_with(&view, cx, [None, None]);
    let repeated = paint_with(&view, cx, [Some("grok"), Some(" grok ")]);
    for ((mode, probes, panes), (_, baseline, _)) in repeated.iter().zip(&unnamed) {
        assert!(
            panes.iter().all(Option::is_none),
            "{mode:?} painted a pane label: {panes:?}"
        );
        assert!(probes.contains_key("grok"), "{mode:?} lost the agent name");
        // Painted text is keyed by string, so a second "grok" would hide behind
        // the agent name. Herdr has no pane label of its own: a pane segment on
        // its first line would add a separator and move the segments around it,
        // so the repeated name must paint exactly what no name paints.
        assert_eq!(probes, baseline, "{mode:?} painted the agent name again");
    }
}

/// The single-line layouts give the agent name its room before the pane,
/// place and status labels share the rest, so a long pane name beside a long
/// workspace and status can cut them but not the name.
#[gpui::test]
fn the_agent_name_stays_whole_beside_long_labels(cx: &mut gpui::TestAppContext) {
    let pane = "api-service-with-a-deliberately-long-pane-name";
    let status = "s".repeat(60);
    let mut snapshot = crowded(1, |_| Some(pane.into()));
    snapshot.workspaces[0].label = "w".repeat(60);
    let agent = &mut snapshot.agents[0];
    let key = crate::sidebar::agents::status_text(agent.agent_status).to_owned();
    agent.state_labels.push((key, status.clone()));
    let (view, cx) = window(cx, snapshot);
    view.update(cx, |view, _| {
        view.config.sidebar_layout.agents.rows =
            vec![vec![crate::config::sidebar::ConfiguredToken::plain(
                crate::config::AgentToken::StateText,
            )]];
    });
    for mode in [LayoutMode::Minimal, LayoutMode::Orca, LayoutMode::Superset] {
        for width in [NARROWEST, 200., SIDEBAR] {
            show(&view, mode, width, cx);
            let probes = probes(cx);
            let (bounds, rendered, _) = probes
                .get("grok")
                .unwrap_or_else(|| panic!("{mode:?} {width}: no agent name in {probes:?}"));
            assert_eq!(rendered, "grok", "{mode:?} {width}: name was cut");
            assert!(bounds.size.width > px(0.), "{mode:?} {width}: no room");
            let (bounds, _, _) = probes
                .get(pane)
                .unwrap_or_else(|| panic!("{mode:?} {width}: no pane name in {probes:?}"));
            assert!(bounds.size.width > px(0.), "{mode:?} {width}: pane hidden");
        }
    }
}

/// The written state keeps its room ahead of the pane and place labels, so
/// a long pane name beside a long workspace cannot hide it.
#[gpui::test]
fn the_written_status_stays_beside_long_pane_and_place_labels(cx: &mut gpui::TestAppContext) {
    let pane = "api-service-with-a-deliberately-long-pane-name";
    let mut snapshot = crowded(1, |_| Some(pane.into()));
    snapshot.workspaces[0].label = "w".repeat(60);
    let agent = &mut snapshot.agents[0];
    let key = crate::sidebar::agents::status_text(agent.agent_status).to_owned();
    agent.state_labels.push((key, "working".into()));
    let (view, cx) = window(cx, snapshot);
    view.update(cx, |view, _| {
        view.config.sidebar_layout.agents.rows =
            vec![vec![crate::config::sidebar::ConfiguredToken::plain(
                crate::config::AgentToken::StateText,
            )]];
    });
    for mode in [LayoutMode::Minimal, LayoutMode::Orca, LayoutMode::Superset] {
        for width in [NARROWEST, 200., SIDEBAR] {
            show(&view, mode, width, cx);
            let probes = probes(cx);
            let (bounds, rendered, _) = probes
                .get("working")
                .unwrap_or_else(|| panic!("{mode:?} {width}: no status in {probes:?}"));
            assert!(
                bounds.size.width > px(0.),
                "{mode:?} {width}: status hidden"
            );
            assert!(!rendered.is_empty(), "{mode:?} {width}: status empty");
            if width == SIDEBAR {
                assert_eq!(rendered, "working", "{mode:?}: status was cut");
            }
        }
    }
}

#[gpui::test]
fn long_names_truncate_inside_the_row_at_narrow_widths(cx: &mut gpui::TestAppContext) {
    let wide = "\u{65e5}\u{672c}\u{8a9e}".repeat(20);
    let long = "x".repeat(120);
    let (view, cx) = window(
        cx,
        crowded(3, |index| match index {
            0 => Some(long.clone()),
            1 => Some(wide.clone()),
            _ => Some("short".into()),
        }),
    );
    for mode in modes() {
        for width in [NARROWEST, 200., 240., SIDEBAR] {
            show(&view, mode, width, cx);
            let probes = probes(cx);
            assert!(
                probes.keys().any(|text| text.contains("short")),
                "{mode:?} {width}: the short name vanished"
            );
            for (index, needle) in [(0, long.as_str()), (1, wide.as_str())] {
                let edge = row_bounds(cx, index).unwrap_or_else(|| panic!("{mode:?} {width}"));
                assert!(
                    edge.size.width <= px(width) + px(1.),
                    "{mode:?} {width} row {index} {edge:?}"
                );
                let captions: Vec<_> = probes
                    .iter()
                    .filter(|(text, _)| text.contains(needle))
                    .collect();
                assert!(
                    !captions.is_empty(),
                    "{mode:?} {width}: row {index} painted no caption"
                );
                for (text, (bounds, rendered, glyphs)) in captions {
                    assert!(
                        bounds.right() <= edge.right() + px(0.5),
                        "{mode:?} {width} {bounds:?} past {edge:?}"
                    );
                    assert!(
                        *glyphs <= bounds.size.width + px(0.5),
                        "{mode:?} {width} {text}"
                    );
                    if rendered.chars().count() < text.chars().count() {
                        assert!(
                            rendered.ends_with('\u{2026}'),
                            "{mode:?} {width} {rendered}"
                        );
                    }
                }
            }
        }
    }
}
