#![allow(clippy::unwrap_used)]

use super::*;
use crate::{
    config::{LayoutMode, Theme},
    contrast::ratio,
    herdr_settings::Settings,
    sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw, snapshot},
};
use core::prelude::v1::test;
use gpui::{
    Bounds, Hsla, Modifiers, Pixels, Rgba, ScaledPixels, TestAppContext, VisualTestContext, rgb,
    size,
};
use herdr_client::ConnectTarget;
use std::sync::Arc;

mod mutes;
mod omo;

fn rail_window(
    cx: &mut TestAppContext,
    hosts: usize,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot(6)));
            view.endpoints[0].live = view.live.clone();
            for host in 1..hosts {
                let mut remote = crate::endpoint::Endpoint::new(
                    format!("ssh:host{host}"),
                    format!("build{host}"),
                    ConnectTarget::Ssh {
                        target: "unused".into(),
                        session: "default".into(),
                    },
                    true,
                );
                remote.live.snapshot = Some(Arc::new(snapshot(2)));
                remote.never_connect();
                view.endpoints.push(remote);
            }
            view.sidebar_visible = false;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (view, cx)
}

fn set_shared(view: &Entity<HerdrWindow>, text: &str, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.settings.shared = Some(Settings::parse_text(text).unwrap());
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn collapsed_sidebar_is_a_rail_in_every_layout_that_navigates(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 1);
    for mode in LayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            view.pending_navigation = None;
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert!(cx.debug_bounds("sidebar").is_none(), "{mode:?}");
        let rail = cx.debug_bounds("sidebar-rail").unwrap();
        assert_eq!(rail.size.width, px(RAIL_WIDTH), "{mode:?}");
        let terminal = cx.debug_bounds("terminal").unwrap();
        assert!(terminal.left() >= rail.right(), "{mode:?}");
        // Every workspace, and every agent, keeps a mark inside the rail.
        for selector in [
            "rail-workspace-local-w0",
            "rail-workspace-local-w5",
            "rail-agent-local-p0",
            "rail-agent-local-p1",
            "rail-new-workspace",
        ] {
            let mark = cx.debug_bounds(selector).unwrap_or_else(|| {
                panic!("{mode:?}: missing {selector}");
            });
            assert!(
                mark.left() >= rail.left() && mark.right() <= rail.right(),
                "{mode:?} {selector}: {mark:?} outside {rail:?}"
            );
        }
        // No host header for a single host.
        assert!(cx.debug_bounds("rail-host-local").is_none());
        let workspace = cx.debug_bounds("rail-workspace-local-w3").unwrap();
        cx.simulate_click(workspace.center(), Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Workspace("w3".into())),
                "{mode:?}"
            );
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let agent = cx.debug_bounds("rail-agent-local-p1").unwrap();
        cx.simulate_click(agent.center(), Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Pane("p1".into())),
                "{mode:?}"
            );
            assert!(!view.sidebar_visible);
        });
    }
    // The titlebar's one sidebar toggle expands the sidebar again.
    assert!(cx.debug_bounds("rail-expand").is_none());
    let expand = cx.debug_bounds("toggle-sidebar").unwrap();
    let rail = cx.debug_bounds("sidebar-rail").unwrap();
    if cfg!(target_os = "macos") {
        assert!(expand.left() >= rail.right());
    } else {
        assert!(expand.left() >= rail.left() && expand.right() <= rail.right());
    }
    cx.simulate_click(expand.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(view.read_with(cx, |view, _| view.sidebar_visible));
    assert!(cx.debug_bounds("sidebar").is_some());
    assert!(cx.debug_bounds("sidebar-rail").is_none());
}

/// Host headers keep their own small box, centered on the grid the rows
/// beneath share. Workspace badges and agent rows use one borderless cell:
/// the same left edge, size, and dot offset, with one highlight width around
/// the glyph, the icon, and the dot.
#[gpui::test]
fn collapsed_agent_rows_line_up_with_workspace_badges(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 2);
    view.update(cx, |view, cx| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.agents[0].agent = Some("grok".into());
        snapshot.agents[0].focused = true;
        cx.notify();
    });
    cx.simulate_resize(size(px(900.), px(900.)));
    for mode in LayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let marks = [
            "rail-mark-rail-workspace-local-w0",
            "rail-mark-rail-workspace-local-w1",
            "rail-mark-rail-workspace-ssh:host1-w0",
            "rail-mark-rail-agent-local-p0",
            "rail-mark-rail-agent-local-p1",
            "rail-mark-rail-agent-ssh:host1-p0",
        ]
        .map(|selector| placed(cx, selector));
        let dots = [
            "rail-dot-rail-workspace-local-w0",
            "rail-dot-rail-workspace-local-w1",
            "rail-dot-rail-workspace-ssh:host1-w0",
            "rail-dot-rail-agent-local-p0",
            "rail-dot-rail-agent-local-p1",
            "rail-dot-rail-agent-ssh:host1-p0",
        ]
        .map(|selector| placed(cx, selector));
        let badge = marks[0];
        for mark in marks {
            assert!(
                near(mark.left(), badge.left()),
                "{mode:?}: mark {mark:?} left differs from badge {badge:?}"
            );
        }
        let dot = dots[0];
        for mark in dots {
            assert!(
                near(mark.left(), dot.left()),
                "{mode:?}: dot {mark:?} left differs from {dot:?}"
            );
        }
        let workspace = placed(cx, "highlight-rail-workspace-local-w0");
        let agent = placed(cx, "highlight-rail-agent-local-p0");
        let other = placed(cx, "highlight-rail-workspace-local-w1");
        assert!(
            near(agent.left(), workspace.left()) && near(agent.size.width, workspace.size.width),
            "{mode:?}: agent highlight {agent:?} != workspace {workspace:?}"
        );
        assert!(
            near(other.left(), workspace.left()) && near(other.size.width, workspace.size.width),
            "{mode:?}: highlight width drifted {other:?} vs {workspace:?}"
        );
        assert!(
            near(agent.size.height, workspace.size.height)
                && near(other.size.height, workspace.size.height),
            "{mode:?}: highlight height drifted {agent:?} {other:?} vs {workspace:?}"
        );
        let dot_offset = dots[0].left() - workspace.left();
        for (mark, cell) in [(dots[1], other), (dots[3], agent)] {
            assert!(
                near(mark.left() - cell.left(), dot_offset),
                "{mode:?}: dot offset {mark:?} in {cell:?} != {dot_offset:?}"
            );
        }
        let number = placed(cx, "rail-mark-rail-workspace-local-w0");
        for (name, child) in [("number", number), ("workspace dot", dots[0])] {
            assert!(
                contains(workspace, child),
                "{mode:?}: {name} {child:?} outside highlight {workspace:?}"
            );
        }
        // The header box is a smaller, separate shape, centered on the same
        // line as the selection highlight. It does not wear that highlight.
        assert!(cx.debug_bounds("highlight-rail-host-local").is_none());
        let header = placed(cx, "rail-host-box-local");
        let other_header = placed(cx, "rail-host-box-ssh:host1");
        let axis = center_x(workspace);
        assert!(
            near(center_x(header), axis) && near(center_x(other_header), axis),
            "{mode:?}: header {header:?} / {other_header:?} off the row axis {workspace:?}"
        );
        assert!(
            near(center_x(agent), axis),
            "{mode:?}: agent highlight off the header axis"
        );
        assert!(
            header.size.width + px(4.) < workspace.size.width
                && near(header.size.width, other_header.size.width)
                && near(header.size.height, other_header.size.height),
            "{mode:?}: header box {header:?} is not a smaller shared cell"
        );
        let letter = placed(cx, "rail-mark-rail-agent-local-p0");
        let icon = placed(cx, "rail-agent-icon-local-p0");
        let status = placed(cx, "rail-dot-rail-agent-local-p0");
        for (name, child) in [("letter", letter), ("icon", icon), ("dot", status)] {
            assert!(
                child.left() >= agent.left() - px(0.5)
                    && child.right() <= agent.right() + px(0.5)
                    && child.top() >= agent.top() - px(0.5)
                    && child.bottom() <= agent.bottom() + px(0.5),
                "{mode:?}: {name} {child:?} outside highlight {agent:?}"
            );
        }
        assert!(icon.left() + px(1.) >= letter.right(), "{mode:?}");
        assert!(status.left() + px(1.) >= icon.right(), "{mode:?}");
        let pads = [
            letter.left() - agent.left(),
            agent.right() - status.right(),
            letter.top().min(icon.top()).min(status.top()) - agent.top(),
            agent.bottom() - letter.bottom().max(icon.bottom()).max(status.bottom()),
        ];
        let (mut low, mut high) = (pads[0], pads[0]);
        for pad in pads {
            low = low.min(pad);
            high = high.max(pad);
        }
        assert!(
            low >= px(2.) && high - low <= px(1.5),
            "{mode:?}: padding {pads:?} is not even around {agent:?}"
        );
    }
}

fn placed(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"))
}

fn near(actual: Pixels, expected: Pixels) -> bool {
    (actual - expected).abs() <= px(1.)
}

fn center_x(bounds: Bounds<Pixels>) -> Pixels {
    bounds.left() + bounds.size.width / 2.
}

/// The host that holds the selected workspace wears that workspace badge's
/// chrome. The other hosts wear the same washes one step quieter, and the
/// chrome follows the workspace when the selection moves. Checked on every
/// bundled theme.
#[gpui::test]
fn the_selected_workspaces_host_stands_apart(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 2);
    let look = view.read_with(cx, |view, _| layout::for_config(&view.config, view.fill()));
    for name in Theme::BUILTIN_NAMES {
        let theme = Theme::builtin(name).unwrap_or_else(|| panic!("missing theme {name}"));
        view.update(cx, |view, cx| {
            view.theme = theme.clone();
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let local = host_paint(cx, "rail-host-box-local");
        let remote = host_paint(cx, "rail-host-box-ssh:host1");
        let highlight = host_paint(cx, "highlight-rail-workspace-local-w0");
        assert_header_apart(name, &theme, look, local, Some(remote), highlight);
    }

    // The loop leaves the last bundled theme on the view; the checks below
    // read Latte, so put Latte back before moving the selection.
    let latte = Theme::builtin("Catppuccin Latte").unwrap_or_else(|| panic!("missing Latte"));
    view.update(cx, |view, cx| {
        view.theme = latte;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    // Select the host directly. A click on its workspace row depends on that
    // row being visible in the spaces pane, which the platform's font metrics
    // decide, and this test is about the chrome, not about hit testing.
    view.update(cx, |view, cx| {
        assert!(view.select_endpoint("ssh:host1", cx));
    });
    // The made-up host never answers; its activation timer must not send the
    // window back to Local while the test looks at the host boxes.
    view.update(cx, |view, _| view.activation_deadline = None);
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.endpoints[view.selected_endpoint].id, "ssh:host1");
    });
    let theme = Theme::builtin("Catppuccin Latte").unwrap_or_else(|| panic!("missing Latte"));
    let local = host_paint(cx, "rail-host-box-local");
    let remote = host_paint(cx, "rail-host-box-ssh:host1");
    let highlight = host_paint(cx, "highlight-rail-workspace-ssh:host1-w0");
    assert_header_apart(
        "Catppuccin Latte",
        &theme,
        look,
        remote,
        Some(local),
        highlight,
    );
}

/// Every layout's host box uses [`SidebarLook::badge_chrome`], the helper
/// that paints the selected workspace number, and the active host stays
/// stronger than a resting one.
#[gpui::test]
fn host_box_wears_the_workspace_badge(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 2);
    for mode in LayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let (look, theme) = view.read_with(cx, |view, _| {
            (
                layout::for_config(&view.config, view.fill()),
                view.theme.clone(),
            )
        });
        let local = host_paint(cx, "rail-host-box-local");
        let highlight = host_paint(cx, "highlight-rail-workspace-local-w0");
        // The resting host can sit below the spaces pane once rows grow.
        // The theme check paints it; here its chrome is the shared helper.
        assert_header_apart(&format!("{mode:?}"), &theme, look, local, None, highlight);
    }
}

/// The letter box, its status dot, and the sidebar-colored ring around that
/// dot stay inside the rail for both hosts, at the real collapsed width.
/// The ring hangs just outside the box: tucked into the corner it is the
/// same color as the sidebar, so it reads as the box and the letter being
/// cut off. Every layout is checked because a compact row is only as tall
/// as the box and clips anything that drops below it.
#[gpui::test]
fn host_box_dot_and_ring_stay_inside_the_rail(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 2);
    for mode in LayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let rail = placed(cx, "sidebar-rail");
        assert_eq!(rail.size.width, px(RAIL_WIDTH), "{mode:?}");
        for (box_name, dot_name, ring_name) in [
            (
                "rail-host-box-local",
                "rail-host-status-local",
                "rail-host-ring-local",
            ),
            (
                "rail-host-box-ssh:host1",
                "rail-host-status-ssh:host1",
                "rail-host-ring-ssh:host1",
            ),
        ] {
            let header = placed(cx, box_name);
            let dot = placed(cx, dot_name);
            let ring = placed(cx, ring_name);
            assert!(
                contains(rail, header) && contains(rail, dot) && contains(rail, ring),
                "{mode:?} {box_name} box {header:?} dot {dot:?} ring {ring:?} outside rail {rail:?}"
            );
            assert!(
                contains(ring, dot),
                "{mode:?} {box_name} dot {dot:?} leaves its ring {ring:?}"
            );
            assert!(
                ring.left() + px(0.5) >= header.right() + px(HOST_DOT_GAP)
                    && ring.top() >= header.top() - px(0.5)
                    && ring.bottom() <= header.bottom() + px(0.5),
                "{mode:?} {box_name} ring {ring:?} bites the box {header:?}"
            );
        }
    }
    // A window that squeezes the rail below the box-plus-dot cannot hang the
    // mark outside. It still has to stay inside the column, off the stroke.
    cx.simulate_resize(size(px(270.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let rail = placed(cx, "sidebar-rail");
    assert_eq!(rail.size.width, px(30.));
    for (box_name, dot_name, ring_name) in [
        (
            "rail-host-box-local",
            "rail-host-status-local",
            "rail-host-ring-local",
        ),
        (
            "rail-host-box-ssh:host1",
            "rail-host-status-ssh:host1",
            "rail-host-ring-ssh:host1",
        ),
    ] {
        let header = placed(cx, box_name);
        let dot = placed(cx, dot_name);
        let ring = placed(cx, ring_name);
        assert!(
            contains(rail, header) && contains(rail, dot) && contains(rail, ring),
            "{box_name} box {header:?} dot {dot:?} ring {ring:?} outside squeezed rail {rail:?}"
        );
        let radius = host_radius(&view, box_name == "rail-host-box-local", cx);
        assert!(
            clears_rounded_border(dot, header, radius),
            "{dot_name} {dot:?} meets the border of {header:?}"
        );
    }
}

/// With several hosts, a large sidebar font and symbol status icons, the
/// shared columns used to need more than the 64 px rail, so every row's
/// grid hung past its edges and the row clipped the status icon. The rail
/// steps the font down until the grid fits, in every layout.
#[gpui::test]
fn wide_marks_in_a_big_font_still_fit_the_rail(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 2);
    view.update(cx, |view, cx| {
        view.config.sidebar.size = 20.;
        view.config.status_icons = crate::config::StatusIcons::Solid;
        cx.notify();
    });
    for mode in LayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let rail = placed(cx, "sidebar-rail");
        assert_eq!(rail.size.width, px(RAIL_WIDTH), "{mode:?}");
        for key in [
            "rail-workspace-local-w0",
            "rail-workspace-ssh:host1-w0",
            "rail-agent-local-p0",
            "rail-agent-ssh:host1-p0",
        ] {
            let row = placed(cx, key);
            let mark = placed(cx, Box::leak(format!("rail-mark-{key}").into_boxed_str()));
            let dot = placed(cx, Box::leak(format!("rail-dot-{key}").into_boxed_str()));
            // Across only: how many rows fit the pane's height is up to the
            // platform's font metrics.
            assert!(
                row.left() >= rail.left() - px(0.5) && row.right() <= rail.right() + px(0.5),
                "{mode:?} {key} row {row:?} past {rail:?}"
            );
            assert!(
                contains(row, mark) && contains(row, dot) && mark.right() <= dot.left(),
                "{mode:?} {key}: mark {mark:?} and status {dot:?} are not inside the row {row:?}"
            );
        }
    }
}

/// The corner dot stays off the header stroke, on the accented host and on a
/// dimmed one. The stroke is the box's 1px rounded border.
#[gpui::test]
fn host_status_dots_clear_the_header_border(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 2);
    for (dot_name, box_name) in [
        ("rail-host-status-local", "rail-host-box-local"),
        ("rail-host-status-ssh:host1", "rail-host-box-ssh:host1"),
    ] {
        let dot = placed(cx, dot_name);
        let header = placed(cx, box_name);
        let radius = host_radius(&view, box_name == "rail-host-box-local", cx);
        assert!(
            clears_rounded_border(dot, header, radius),
            "{dot_name} {dot:?} meets the border of {header:?}"
        );
    }
}

/// `dot` stays off the header stroke. That holds when it sits fully inside
/// the rounded border, with a pixel of fill between them, and when it sits
/// fully outside the box with a gap. The border is a 1px stroke on the
/// badge chrome's radius.
fn clears_rounded_border(dot: Bounds<Pixels>, header: Bounds<Pixels>, radius: f32) -> bool {
    beside_the_box(dot, header) || inside_the_stroke(dot, header, radius)
}

/// The whole dot is to the right of the box and stays within its height, so
/// the stroke cannot cross it and the row cannot clip it.
fn beside_the_box(dot: Bounds<Pixels>, header: Bounds<Pixels>) -> bool {
    dot.left() >= header.right() + px(1.)
        && dot.top() >= header.top() - px(0.5)
        && dot.bottom() <= header.bottom() + px(0.5)
}

/// `radius` is the box's own corner radius, [`SidebarLook::badge_chrome`]'s:
/// `corners::CONTROL` in rounded layouts and zero in flat ones.
fn inside_the_stroke(dot: Bounds<Pixels>, header: Bounds<Pixels>, radius: f32) -> bool {
    let inset = 1. + 1.;
    let radius = (radius - inset).max(0.);
    let left = f32::from(header.left()) + inset;
    let top = f32::from(header.top()) + inset;
    let right = f32::from(header.right()) - inset;
    let bottom = f32::from(header.bottom()) - inset;
    [
        (f32::from(dot.left()), f32::from(dot.top())),
        (f32::from(dot.right()), f32::from(dot.top())),
        (f32::from(dot.left()), f32::from(dot.bottom())),
        (f32::from(dot.right()), f32::from(dot.bottom())),
    ]
    .into_iter()
    .all(|(x, y)| inside_rounded(x, y, left, top, right, bottom, radius))
}

fn inside_rounded(
    x: f32,
    y: f32,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
) -> bool {
    let radius = radius.min((right - left) / 2.).min((bottom - top) / 2.);
    let cx = x.clamp(left + radius, right - radius);
    let cy = y.clamp(top + radius, bottom - radius);
    let dx = x - cx;
    let dy = y - cy;
    dx * dx + dy * dy <= radius * radius + 0.01
}

/// The corner radius the host box of the active (`true`) or a resting host
/// is drawn with in the view's current layout.
fn host_radius(view: &Entity<HerdrWindow>, active: bool, cx: &mut VisualTestContext) -> f32 {
    view.read_with(cx, |view, _| {
        layout::for_config(&view.config, view.fill())
            .badge_chrome(active, &view.theme)
            .radius
    })
}

/// Fill and border painted for one box. A border is its own quad, and a
/// transparent fill paints nothing.
#[derive(Debug)]
struct HeaderPaint {
    fill: Option<Hsla>,
    border: Option<Hsla>,
}

/// `active` is the header of the host that holds the selected workspace,
/// `rest` is another host, and `highlight` is that workspace's selection box.
/// Both headers take their box from [`SidebarLook::badge_chrome`], and the
/// active one is that badge rather than a second palette.
fn assert_header_apart(
    name: &str,
    theme: &Theme,
    look: SidebarLook,
    active: HeaderPaint,
    rest: Option<HeaderPaint>,
    highlight: HeaderPaint,
) {
    let raised = look.badge_chrome(true, theme);
    let quiet = look.badge_chrome(false, theme);
    let chrome = host_chrome(true, look, theme);
    assert!(
        chrome.fill == raised.fill
            && chrome.border == raised.border
            && chrome.radius == raised.radius
            && chrome.letter == rgb(theme.foreground),
        "{name}: active host does not wear the workspace badge"
    );
    let dimmed = host_chrome(false, look, theme);
    assert!(
        dimmed.fill == quiet.fill
            && dimmed.border == quiet.border
            && dimmed.radius == quiet.radius
            && dimmed.letter == rgb(theme.muted),
        "{name}: resting host does not wear the quiet badge"
    );
    assert!(
        raised.fill != quiet.fill || raised.border != quiet.border,
        "{name}: active and resting badges are the same"
    );
    // The letter has to read on a solid active fill, and on the sidebar
    // under a wash.
    assert!(
        ratio(theme.foreground, theme.active) >= theme.contrast.mark_ratio(),
        "{name}: letter lacks contrast on the active fill"
    );
    assert!(
        ratio(theme.foreground, theme.sidebar_background()) >= theme.contrast.mark_ratio(),
        "{name}: letter lacks contrast on the sidebar"
    );

    let fill = active
        .fill
        .unwrap_or_else(|| panic!("{name}: active header has no fill"));
    assert!(
        matches_color(fill, Hsla::from(raised.fill)),
        "{name}: active fill is not the badge: {fill:?}"
    );
    let mark_fill = highlight
        .fill
        .unwrap_or_else(|| panic!("{name}: selected workspace has no badge"));
    assert!(
        matches_color(mark_fill, Hsla::from(raised.fill)),
        "{name}: workspace badge fill drifted from the host: {mark_fill:?}"
    );
    assert!(
        same_edge(active.border, highlight.border),
        "{name}: host stroke {active:?} is not the workspace badge {highlight:?}"
    );
    if let Some(rest) = rest {
        let rest_fill = rest
            .fill
            .unwrap_or_else(|| panic!("{name}: resting header has no fill"));
        assert!(
            matches_color(rest_fill, Hsla::from(quiet.fill)),
            "{name}: resting fill is not the quiet badge: {rest_fill:?}"
        );
        assert!(
            same_edge(rest.border, Some(Hsla::from(quiet.border))),
            "{name}: resting stroke is not the quiet badge: {rest:?}"
        );
    }
}

/// A missing edge and a transparent one are the same stroke. The fill
/// highlight draws no border; the host keeps a transparent one so its box
/// does not change size between layouts.
fn same_edge(host: Option<Hsla>, badge: Option<Hsla>) -> bool {
    match (host, badge) {
        (Some(host), Some(badge)) => {
            matches_color(host, badge) || (host.a < 0.05 && badge.a < 0.05)
        }
        (Some(host), None) | (None, Some(host)) => host.a < 0.05,
        (None, None) => true,
    }
}

/// Painted `color` is `want`, within a couple of 8-bit steps. Hue is a poor
/// comparison: the selection colors are close to gray.
fn matches_color(color: Hsla, want: Hsla) -> bool {
    let got = Rgba::from(color);
    let expected = Rgba::from(want);
    let near = |left: f32, right: f32| (left - right).abs() <= 2. / 255.;
    near(got.r, expected.r)
        && near(got.g, expected.g)
        && near(got.b, expected.b)
        && near(got.a, expected.a)
}

/// The background and border painted for `selector`.
fn host_paint(cx: &mut VisualTestContext, selector: &'static str) -> HeaderPaint {
    let bounds = placed(cx, selector);
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let wanted = bounds.scale(scale);
        let slack = px(1.5).scale(scale);
        let mut fill = None;
        let mut border = None;
        for quad in window.painted_quads() {
            if !same_box(quad.bounds, wanted, slack) {
                continue;
            }
            let bordered = quad.border_widths.top.0 > 0. || quad.border_widths.left.0 > 0.;
            if bordered {
                border = Some(quad.border_color);
            } else if let Some(color) = quad.background.as_solid() {
                fill = Some(color);
            }
        }
        assert!(fill.is_some() || border.is_some(), "no quad for {selector}");
        HeaderPaint { fill, border }
    })
}

fn same_box(
    actual: Bounds<ScaledPixels>,
    wanted: Bounds<ScaledPixels>,
    slack: ScaledPixels,
) -> bool {
    let close = |actual: ScaledPixels, wanted: ScaledPixels| (actual.0 - wanted.0).abs() <= slack.0;
    close(actual.origin.x, wanted.origin.x)
        && close(actual.origin.y, wanted.origin.y)
        && close(actual.size.width, wanted.size.width)
        && close(actual.size.height, wanted.size.height)
}

fn contains(outer: Bounds<Pixels>, inner: Bounds<Pixels>) -> bool {
    inner.left() >= outer.left() - px(0.5)
        && inner.right() <= outer.right() + px(0.5)
        && inner.top() >= outer.top() - px(0.5)
        && inner.bottom() <= outer.bottom() + px(0.5)
}

#[gpui::test]
fn rail_follows_shared_collapsed_mode(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 1);
    let rail_right = cx.debug_bounds("sidebar-rail").unwrap().right();
    set_shared(&view, "[ui]\nsidebar_collapsed_mode = 'hidden'\n", cx);
    assert!(cx.debug_bounds("sidebar-rail").is_none());
    assert!(cx.debug_bounds("sidebar").is_none());
    // Hidden gives the rail's room back to the terminal.
    let terminal = cx.debug_bounds("terminal").unwrap();
    assert!(terminal.left() < rail_right);
    set_shared(&view, "[ui]\nsidebar_collapsed_mode = 'compact'\n", cx);
    assert!(cx.debug_bounds("sidebar-rail").is_some());
    // Expanding ignores the collapsed mode.
    view.update(cx, |view, cx| {
        view.toggle_sidebar();
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("sidebar").is_some());
}

#[gpui::test]
fn rail_shows_a_header_per_host_and_skips_collapsed_hosts(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 3);
    let rail = cx.debug_bounds("sidebar-rail").unwrap();
    for selector in [
        "rail-host-local",
        "rail-host-ssh:host1",
        "rail-host-ssh:host2",
        "rail-host-status-ssh:host1",
        "rail-workspace-local-w0",
        "rail-workspace-ssh:host1-w0",
        "rail-workspace-ssh:host2-w1",
        "rail-agent-ssh:host2-p0",
    ] {
        let mark = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing {selector}"));
        assert!(mark.right() <= rail.right(), "{selector}");
    }
    // Hosts list in order, each above its own workspaces.
    let first = cx.debug_bounds("rail-host-ssh:host1").unwrap().top();
    let second = cx.debug_bounds("rail-host-ssh:host2").unwrap().top();
    let workspace = cx.debug_bounds("rail-workspace-ssh:host1-w1").unwrap();
    assert!(first < workspace.top() && workspace.top() < second);
    view.update(cx, |view, cx| {
        view.endpoints[1].collapsed = true;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("rail-host-ssh:host1").is_some());
    assert!(cx.debug_bounds("rail-workspace-ssh:host1-w0").is_none());
    assert!(cx.debug_bounds("rail-agent-ssh:host1-p0").is_none());
    assert!(cx.debug_bounds("rail-workspace-ssh:host2-w0").is_some());
    // A host header selects its host, as the expanded header does, even
    // while its rows are folded away.
    let host = cx.debug_bounds("rail-host-ssh:host1").unwrap();
    cx.simulate_click(host.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_eq!(view.endpoints[view.selected_endpoint].id, "ssh:host1");
    });
}

#[gpui::test]
fn narrow_windows_keep_the_terminal_reserve(cx: &mut TestAppContext) {
    let (_, cx) = rail_window(cx, 2);
    for (width, rail) in [(900., RAIL_WIDTH), (270., 30.), (200., 0.)] {
        cx.simulate_resize(size(px(width), px(400.)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("sidebar-rail")
                .map_or(px(0.), |bounds| bounds.size.width),
            px(rail),
            "{width}"
        );
    }
}

#[gpui::test]
fn start_collapsed_applies_once_and_yields_to_an_earlier_toggle(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 1);
    let collapsed = Settings::parse_text("[ui]\nsidebar_start_collapsed = true\n").unwrap();
    view.update(cx, |view, _| {
        // A fresh window stays expanded until the first shared load.
        view.sidebar_visible = true;
        view.sidebar_start_pending = true;
        view.settings.shared = Some(Settings::parse_text("").unwrap());
        view.apply_sidebar_start();
        assert!(view.sidebar_visible);
        // Only the first load counts; a later edit waits for a restart.
        view.settings.shared = Some(collapsed.clone());
        view.apply_sidebar_start();
        assert!(view.sidebar_visible);

        view.sidebar_start_pending = true;
        view.apply_sidebar_start();
        assert!(!view.sidebar_visible);
        // Expanding afterwards sticks across reloads.
        view.toggle_sidebar();
        view.apply_sidebar_start();
        assert!(view.sidebar_visible);

        // Toggling before the settings arrive keeps the user's choice.
        view.sidebar_start_pending = true;
        view.toggle_sidebar();
        view.toggle_sidebar();
        view.apply_sidebar_start();
        assert!(view.sidebar_visible);
    });
}

#[test]
fn tooltips_name_host_place_and_status() {
    assert_eq!(
        describe(
            Some("build"),
            "review",
            Some("herdr / tab 2"),
            status_word(AgentStatus::Blocked).as_deref()
        ),
        "\u{2068}build\u{2069}: \u{2068}\u{2068}\u{2068}review\u{2069} in \u{2068}herdr / tab 2\u{2069}\u{2069} (\u{2068}blocked\u{2069})\u{2069}"
    );
    assert_eq!(
        describe(
            None,
            "herdr",
            None,
            status_word(AgentStatus::Unknown).as_deref()
        ),
        "herdr"
    );
    let mut snapshot = snapshot(1);
    let agent = &mut snapshot.agents[0];
    assert_eq!(agent_status_word(agent).as_deref(), Some("working"));
    agent.state_labels = vec![("working".into(), "deep in the mines".into())];
    assert_eq!(
        agent_status_word(agent).as_deref(),
        Some("deep in the mines")
    );
    agent.agent_status = AgentStatus::Unknown;
    assert_eq!(agent_status_word(agent), None);
    agent.state_labels = vec![("unknown".into(), "resting".into())];
    assert_eq!(agent_status_word(agent).as_deref(), Some("resting"));
    assert_eq!(host_initial("  ssh box"), "S");
    assert_eq!(host_initial(""), "?");
    assert_eq!(host_initial("éclair"), "É");
}
