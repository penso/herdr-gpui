//! Paint-phase probes shared by the headless layout and native full-window tests.
//! Headless NoopTextSystem ignores font-run lengths, so only the native smoke
//! test can catch GPUI's stale truncation runs. Keep headless checks for geometry.
#![allow(clippy::unwrap_used)]
use crate::HerdrWindow;
#[cfg(test)]
use crate::{LiveState, WheelAccumulator};
use anyhow::{Context as _, Result, ensure};
use gpui::{
    App, Bounds, ElementId, Global, GlobalElementId, InspectorElementId, LayoutId, Pixels,
    SharedString, TextLayout, Window, prelude::*, px,
};
#[cfg(test)]
use gpui::{ArenaClearNeeded, Context, Entity, Modifiers, Task, point, size};
use herdr_client::protocol::*;
#[cfg(test)]
use herdr_client::{ConnectOptions, ConnectTarget};
#[cfg(test)]
use std::sync::Arc;

#[derive(Default)]
struct TextProbes(std::collections::BTreeMap<String, (Bounds<Pixels>, String, Pixels)>);
impl Global for TextProbes {}

#[derive(Default)]
pub(crate) struct PaintedProbes(
    pub std::collections::BTreeMap<String, PaintedText>,
    Option<anyhow::Error>,
);
impl Global for PaintedProbes {}

impl PaintedProbes {
    fn record(&mut self, text: String, result: Result<PaintedText>) {
        match result {
            Ok(probe) => {
                self.0.entry(text).or_insert(probe);
            }
            Err(error) if self.1.is_none() => {
                let error = error.context(format!("native paint probe {text:?}"));
                eprintln!("SIDEBAR native paint FAIL: {error:#}");
                self.1 = Some(error);
            }
            Err(_) => {}
        }
    }

    // Retain the first failure even when a later frame clears the paint cache.
    // The smoke driver consumes it before reporting success, outside paint/FFI.
    pub(crate) fn check(&mut self) -> Result<()> {
        self.1.take().map_or(Ok(()), Err)
    }
}

#[derive(Default)]
pub(crate) struct VerifyChildGeometry(pub bool);
impl Global for VerifyChildGeometry {}

#[derive(Debug)]
#[cfg_attr(not(feature = "integration-test"), allow(dead_code))]
pub(crate) struct PaintedText {
    pub bounds: Bounds<Pixels>,
    pub mask: Bounds<Pixels>,
    pub cached: String,
    pub glyph_text: String,
    pub width: Pixels,
    pub clipped: bool,
}

// Delegate every phase to the production SharedString element. Native checks
// inspect the glyph stream used by paint, not just the cached backing string.
pub(crate) struct ProbeText(pub SharedString);

impl IntoElement for ProbeText {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for ProbeText {
    type RequestLayoutState = TextLayout;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, TextLayout) {
        self.0.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut TextLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.prepaint(id, inspector_id, bounds, state, window, cx);
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut TextLayout,
        prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .paint(id, inspector_id, bounds, state, prepaint, window, cx);
        let Some(line) = state.line_layout_for_index(0) else {
            if cx.has_global::<PaintedProbes>() {
                cx.default_global::<PaintedProbes>().record(
                    self.0.to_string(),
                    Err(anyhow::anyhow!("missing first text line")),
                );
            }
            return;
        };
        let width = line.unwrapped_layout.width;
        cx.default_global::<TextProbes>().0.insert(
            self.0.to_string(),
            (state.bounds(), state.wrapped_text(), width),
        );
        if !cx.has_global::<PaintedProbes>() {
            return;
        }
        let result = self.inspect(bounds, state, window, cx);
        cx.default_global::<PaintedProbes>()
            .record(self.0.to_string(), result);
    }
}

impl ProbeText {
    fn inspect(
        &self,
        bounds: Bounds<Pixels>,
        state: &TextLayout,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<PaintedText> {
        // Inspect the actual native glyph stream consumed by WrappedLine::paint.
        // Its backing string can be longer than the shaped font runs (GPUI 0.2.2).
        let line = state
            .line_layout_for_index(0)
            .context("missing first text line")?;
        let layout = &line.unwrapped_layout;
        let text = state.text();
        let mask = window.content_mask().bounds;
        let mut glyph_text = String::new();
        let mut clipped = !line.wrap_boundaries.is_empty();
        let baseline = bounds.origin.y
            + (state.line_height() - layout.ascent - layout.descent) / 2.
            + layout.ascent;
        for run in &layout.runs {
            for glyph in &run.glyphs {
                let ch = text
                    .get(glyph.index..)
                    .and_then(|text| text.chars().next())
                    .with_context(|| format!("invalid glyph source index {}", glyph.index))?;
                let ink = cx
                    .text_system()
                    .typographic_bounds(run.font_id, layout.font_size, ch)?;
                let left = bounds.origin.x + glyph.position.x + ink.origin.x;
                let right = left + ink.size.width;
                let top = baseline + glyph.position.y - ink.bottom();
                let bottom = top + ink.size.height;
                if ink.size.width > px(0.) && ink.size.height > px(0.) {
                    clipped |= left < mask.left()
                        || right > mask.right()
                        || top < mask.top()
                        || bottom > mask.bottom();
                }
                // Resolve the ID independently, not merely its cached source index.
                let expected = window.text_system().shape_line(
                    ch.to_string().into(),
                    layout.font_size,
                    &[window.text_style().to_run(ch.len_utf8())],
                    None,
                );
                let expected = expected
                    .runs
                    .first()
                    .and_then(|run| run.glyphs.first())
                    .context("independent glyph shaping produced no glyph")?;
                ensure!(
                    expected.id == glyph.id,
                    "painted glyph ID for {ch:?}: actual {:?}, expected {:?}",
                    glyph.id,
                    expected.id
                );
                glyph_text.push(ch);
            }
        }
        let probe = PaintedText {
            bounds,
            mask,
            cached: state.wrapped_text(),
            glyph_text,
            width: layout.width,
            clipped,
        };
        // These extra fixture rows leave the original smoke/performance labels
        // untouched. Check their native glyphs whenever the whole row is visible.
        if cx.default_global::<VerifyChildGeometry>().0
            && matches!(
                self.0.as_ref(),
                "sidebar-child" | "sidebar-child-with-a-long-readable-branch-name"
            )
            && bounds.top() >= mask.top()
            && bounds.bottom() <= mask.bottom()
        {
            let mode = window
                .root::<HerdrWindow>()
                .flatten()
                .map(|view| view.read(cx).config.layout.mode)
                .unwrap_or_default();
            probe.verify_child(&self.0, mode)?;
            eprintln!("SIDEBAR child verified: {}", probe.glyph_text);
        }
        Ok(probe)
    }
}

impl PaintedText {
    fn verify_child(&self, input: &str, mode: crate::config::LayoutMode) -> Result<()> {
        let look = super::layout::for_mode(mode);
        let layout = look.density;
        // Menu labels share text keys, and retained paint probes can still
        // describe the previous layout. Compute the sidebar column directly.
        let left =
            px(look.content_x() + layout.child_indent() + super::STATUS_WIDTH + layout.gap());
        let width = px(look.content_width(super::SIDEBAR_WIDTH)
            - super::STATUS_WIDTH
            - layout.gap()
            - layout.child_indent()
            - super::ARROW_RESERVE);
        ensure!(
            self.bounds.left() == left,
            "child label left: actual {:?}, expected {left:?}",
            self.bounds.left()
        );
        ensure!(
            self.bounds.size.width == width,
            "child label width: actual {:?}, expected {width:?}",
            self.bounds.size.width
        );
        ensure!(
            self.mask.size.width == self.bounds.size.width,
            "child mask width: actual {:?}, expected {:?}",
            self.mask.size.width,
            self.bounds.size.width
        );
        ensure!(
            self.glyph_text == self.cached,
            "child glyphs {:?} differ from cached text {:?}",
            self.glyph_text,
            self.cached
        );
        ensure!(!self.clipped, "child glyphs clipped: {}", self.glyph_text);
        if input == "sidebar-child" {
            ensure!(
                self.glyph_text == input,
                "short child label changed: {:?}",
                self.glyph_text
            );
        } else {
            ensure!(
                self.glyph_text.starts_with("sidebar-child")
                    && self.glyph_text.ends_with('\u{2026}'),
                "long child label did not truncate correctly: {:?}",
                self.glyph_text
            );
            ensure!(
                self.width > self.bounds.size.width - px(12.),
                "child glyph width {:?} did not fill label width {:?}",
                self.width,
                self.bounds.size.width
            );
        }
        Ok(())
    }
}

#[cfg(test)]
struct SidebarFixture(Entity<HerdrWindow>);

#[test]
fn native_child_probe_reports_geometry_and_glyph_failures_without_panicking() {
    use crate::config::{Density, LayoutMode, Style};
    // Rounded rows move content in by the highlight's inset, the density's
    // gap, on both edges.
    for (mode, left, width) in [
        (LayoutMode::from(Density::Comfortable), 56., 145.),
        (LayoutMode::from(Density::Normal), 44., 161.),
        (LayoutMode::from(Density::Compact), 38., 169.),
        (
            LayoutMode::new(Density::Comfortable, Style::Rounded),
            64.,
            129.,
        ),
        (LayoutMode::new(Density::Normal, Style::Rounded), 50., 149.),
        (LayoutMode::new(Density::Compact, Style::Rounded), 42., 161.),
    ] {
        let bounds = Bounds::new(point(px(left), px(100.)), size(px(width), px(16.)));
        let mut probe = PaintedText {
            bounds,
            mask: bounds,
            cached: "sidebar-child".into(),
            glyph_text: "sidebar-child".into(),
            width: px(94.),
            clipped: false,
        };
        probe.verify_child("sidebar-child", mode).unwrap();
        probe.bounds.size.width -= px(1.);
        let error = probe.verify_child("sidebar-child", mode).unwrap_err();
        assert!(error.to_string().contains("child label width: actual"));
        assert!(error.to_string().contains("expected"));
        probe.bounds = bounds;
        probe.clipped = true;
        assert!(
            probe
                .verify_child("sidebar-child", mode)
                .unwrap_err()
                .to_string()
                .contains("clipped")
        );
        probe.clipped = false;
        probe.glyph_text = "sidebar-chil".into();
        assert!(
            probe
                .verify_child("sidebar-child", mode)
                .unwrap_err()
                .to_string()
                .contains("cached text")
        );
        probe.glyph_text = "sidebar-child-with-…".into();
        probe.cached.clone_from(&probe.glyph_text);
        probe.width = px(width - 5.);
        probe
            .verify_child("sidebar-child-with-a-long-readable-branch-name", mode)
            .unwrap();
        probe.width = px(50.);
        assert!(
            probe
                .verify_child("sidebar-child-with-a-long-readable-branch-name", mode)
                .unwrap_err()
                .to_string()
                .contains("did not fill")
        );
    }
}

#[test]
fn native_probe_failure_survives_later_frames_and_keeps_its_source() {
    let mut probes = PaintedProbes::default();
    probes.record(
        "first label".into(),
        Err(std::io::Error::from(std::io::ErrorKind::InvalidData).into()),
    );
    probes.0.clear();
    probes.record("later label".into(), Err(anyhow::anyhow!("later failure")));
    let error = probes.check().unwrap_err();
    assert!(error.to_string().contains("first label"));
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert!(probes.check().is_ok());
}

#[cfg(test)]
impl Render for SidebarFixture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

pub(crate) const REPO_KEY: &str = if cfg!(windows) {
    "C:/fixture/agent-launcher/.git"
} else {
    "/fixture/agent-launcher/.git"
};

pub(crate) fn snapshot(workspace_count: usize) -> ClientShellSnapshot {
    serde_json::from_value(serde_json::json!({
        "boot_id": "layout-test", "revision": 1,
        "update_install_command": "", "latest_release_notes_available": false,
        "integration_updates_available": false, "worktree_directory": "",
        "tab_bar_right": [], "tab_bar_right_separator": "", "agent_order": [],
        // A workspace always has at least one tab; two here so the agents panel
        // has a tab label to show, as it does against a live daemon.
        "tabs": (0..2).map(|i| serde_json::json!({
            "tab_id": format!("t{i}"), "workspace_id": "w0", "number": i + 1,
            "label": format!("tab {}", i + 1), "custom_label": false,
            "zoomed": false, "focused": i == 0, "agent_status": "working"
        })).collect::<Vec<_>>(),
        "panes": [], "commands": [],
        "workspaces": (0..workspace_count).map(|i| serde_json::json!({
            "workspace_id": format!("w{i}"), "active_tab_id": "t0", "new_workspace_cwd": "/tmp",
            "number": i + 1,
            "label": match i { 0 => "herdr", 1 => "herdr-gpui-sidebar-rendering-regression-investigation", 3..=5 => "agent-launcher", _ => "another workspace" },
            "custom_label": false,
            "branch": match i { 0 => "main", 2 => "1256789", 3 => "develop", 4 => "worktree/sidebar-child", 5 => "worktree/sidebar-child-with-a-long-readable-branch-name", _ => "fix/sidebar-label-width-and-overflow-regression" },
            "worktree": if (3..=5).contains(&i) { serde_json::json!({
                "key": REPO_KEY, "label": "agent-launcher", "is_linked_worktree": i != 3
            }) } else { serde_json::Value::Null },
            "tokens": [], "focused": i == 0, "agent_status": "working"
        })).collect::<Vec<_>>(),
        "agents": (["review", "Investigate sidebar rendering and verify long agent labels"].into_iter().enumerate().map(|(i, name)| serde_json::json!({
            "pane_id": format!("p{i}"), "workspace_id": if i == 0 { "w0" } else { "w1" },
            "tab_id": if i == 0 { "t0" } else { "none" },
            "name": name, "display_agent": if i == 0 { "Claude Code" } else { "agent" }, "agent": "claude",
            "agent_status": "working", "state_change_seq": 0, "state_labels": [],
            "tokens": [], "focused": false
        })).collect::<Vec<_>>())
    })).unwrap()
}

#[gpui::test]
fn terminal_redraws_reuse_the_cached_sidebar(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let renders = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| view.read(cx).sidebar_view.read(cx).renders)
    };
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let first = renders(cx);
    assert!(first > 0);

    // Terminal output: the window redraws, the rows do not rebuild.
    for _ in 0..3 {
        view.update(cx, |view, cx| view.redraw_terminal(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(renders(cx), first);

    // Anything else notifies the window, which rebuilds the rows as before.
    view.update(cx, |view, cx| {
        view.sidebar_width = Some(200.);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(renders(cx), first + 1);
    // Debug bounds are only recorded when painted, so read them from a full frame.
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(renders(cx), first + 2);
    assert_eq!(
        cx.debug_bounds("sidebar").map(|b| b.size.width),
        Some(px(200.))
    );

    // A hidden sidebar is not built, even for a full frame.
    view.update(cx, |view, cx| {
        view.sidebar_visible = false;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(renders(cx), first + 2);
}

#[gpui::test]
fn sidebar_allocates_text_width(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        // Deliberately do not call HerdrWindow::new: it connects and starts polling.
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let result = check_sidebar(fixture, cx);
    assert!(result.is_ok(), "sidebar layout failed: {result:#?}");
}

#[gpui::test]
fn agent_icons_follow_names_and_reserve_narrow_label_width(cx: &mut gpui::TestAppContext) {
    use crate::config::{Density, LayoutMode, Style};
    let label = "Custom agent name with a deliberately long label";
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut snapshot = snapshot(6);
        for agent in &mut snapshot.agents {
            agent.display_agent = Some(label.into());
        }
        snapshot.agents[1].workspace_id = "missing-workspace".into();
        view.live.snapshot = Some(Arc::new(snapshot));
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.run_until_parked();
    for mode in [Density::Compact, Density::Normal, Density::Comfortable]
        .into_iter()
        .flat_map(|density| {
            [Style::Flat, Style::Rounded].map(|style| LayoutMode::new(density, style))
        })
    {
        for width in [160., 232., 480.] {
            for identity in [
                Some("opencode"),
                Some("claude"),
                Some("codex"),
                Some("gemini"),
                Some("cursor"),
                Some("copilot"),
                Some("unknown"),
                None,
            ] {
                view.update(cx, |view, cx| {
                    view.config.layout.mode = mode;
                    view.sidebar_width = Some(width);
                    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                    for agent in &mut snapshot.agents {
                        agent.agent = identity.map(str::to_owned);
                    }
                    cx.notify();
                });
                cx.update(|window, cx| {
                    cx.default_global::<TextProbes>().0.clear();
                    full_draw(window, cx).clear(cx);
                    let (bounds, rendered, glyphs) = &cx.global::<TextProbes>().0[label];
                    assert!(*glyphs <= bounds.size.width);
                    if width == 160. {
                        assert!(rendered.ends_with('…'));
                    }
                });
                for (icon, name, column) in [
                    ("agent-icon-agent-p0", "detail-agent-p0", "column-agent-p0"),
                    ("agent-icon-agent-p1", "name-agent-p1", "column-agent-p1"),
                ] {
                    let icon = cx.debug_bounds(icon).unwrap();
                    let name = cx.debug_bounds(name).unwrap();
                    let column = cx.debug_bounds(column).unwrap();
                    assert_eq!(icon.size, size(px(12.), px(12.)));
                    assert_eq!(icon.left(), column.left());
                    assert_eq!(name.left(), icon.right() + px(4.));
                    assert_eq!(name.right(), column.right());
                    assert_eq!(icon.center().y, name.center().y);
                    assert!(icon.right() <= column.right());
                }
                let location = cx.debug_bounds("name-agent-p0").unwrap();
                let column = cx.debug_bounds("column-agent-p0").unwrap();
                assert_eq!(location.left(), column.left());
            }
        }
    }
}

#[cfg(test)]
/// Every layout keeps its text inside the box it was measured for and its
/// rows inside the sidebar, and marks the focused row.
fn check_layouts(modes: &[crate::config::LayoutMode], cx: &mut gpui::TestAppContext) {
    use crate::config::LayoutMode;
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot(6)));
        let input = crate::pull_request::Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "worktree/sidebar-child".into(),
        };
        let now = std::time::Instant::now();
        let mut pr = crate::pull_request::fixture().unwrap();
        pr.number = 7;
        pr.additions = 234;
        pr.deletions = 567;
        view.menu.pr_cache.seed(input.clone(), pr, now);
        view.git.seed_probe(input, true, now);
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.run_until_parked();
    for font_size in [12., 18.] {
        for width in [160., 232., 480.] {
            for &mode in modes {
                view.update(cx, |view, cx| {
                    view.config.layout.mode = mode;
                    view.config.sidebar.size = font_size;
                    view.sidebar_width = Some(width);
                    cx.notify();
                });
                let context = format!("{mode} at {width}px, {font_size}pt");
                cx.update(|window, cx| {
                    cx.default_global::<TextProbes>().0.clear();
                    full_draw(window, cx).clear(cx);
                    let probes = &cx.global::<TextProbes>().0;
                    for text in ["herdr", "agent-launcher", "Claude Code"] {
                        assert!(probes.contains_key(text), "{context}: {text} missing");
                    }
                    for (text, (bounds, _, glyphs)) in probes {
                        // Headers and the device footer are not rows; the
                        // rows' own text must fit where it was placed.
                        // A box too narrow for an ellipsis clips instead;
                        // subpixel shaping may overhang a whole-pixel box.
                        assert!(
                            *glyphs <= bounds.size.width + px(1.)
                                || bounds.size.width < px(2. * font_size),
                            "{context}: {text:?} overflows {bounds:?} with {glyphs:?}"
                        );
                    }
                });
                let sidebar = cx.debug_bounds("sidebar").unwrap();
                for (row, name) in [
                    ("row-herdr", "name-herdr"),
                    ("row-agent-launcher", "name-agent-launcher"),
                    ("row-sidebar-child", "name-sidebar-child"),
                    ("row-agent-p0", "name-agent-p0"),
                ] {
                    let row_bounds = cx
                        .debug_bounds(row)
                        .unwrap_or_else(|| panic!("{context}: {row} missing"));
                    assert!(row_bounds.right() <= sidebar.right(), "{context}: {row}");
                    let name_bounds = cx.debug_bounds(name).unwrap();
                    assert!(
                        name_bounds.right() <= row_bounds.right(),
                        "{context}: {name}"
                    );
                    assert!(
                        name_bounds.bottom() <= row_bounds.bottom(),
                        "{context}: {name}"
                    );
                }
                // Minimal rows show no pull request or uncommitted work.
                if mode != LayoutMode::Minimal {
                    let row = cx.debug_bounds("row-sidebar-child").unwrap();
                    let badge = cx.debug_bounds("pr-sidebar-child").unwrap();
                    assert!(
                        badge.right() <= row.right(),
                        "{context}: badge {badge:?} {row:?}"
                    );
                    assert!(
                        badge.bottom() <= row.bottom(),
                        "{context}: badge {badge:?} {row:?}"
                    );
                    assert!(cx.debug_bounds("dirty-sidebar-child").is_some());
                }
                // Only the focused workspace draws a selection mark in the
                // layouts whose highlight exists only while selected.
                assert!(cx.debug_bounds("highlight-herdr").is_some() || mode == LayoutMode::Orca);
            }
        }
    }
}

#[gpui::test]
fn classic_layouts_fit_the_sidebar(cx: &mut gpui::TestAppContext) {
    check_layouts(&crate::config::LayoutMode::ALL[..6], cx);
}

#[gpui::test]
fn superset_layout_fits_the_sidebar(cx: &mut gpui::TestAppContext) {
    check_layouts(&[crate::config::LayoutMode::Superset], cx);
}

#[gpui::test]
fn orca_layout_fits_the_sidebar(cx: &mut gpui::TestAppContext) {
    check_layouts(&[crate::config::LayoutMode::Orca], cx);
}

#[gpui::test]
fn minimal_layout_fits_the_sidebar(cx: &mut gpui::TestAppContext) {
    check_layouts(&[crate::config::LayoutMode::Minimal], cx);
}

#[gpui::test]
fn sidebar_densities_keep_details_and_badges_within_their_rows(cx: &mut gpui::TestAppContext) {
    use crate::config::{Density, LayoutMode, Style};
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot(6)));
        let input = crate::pull_request::Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "worktree/sidebar-child".into(),
        };
        let now = std::time::Instant::now();
        let mut pr = crate::pull_request::fixture().unwrap();
        pr.number = 7;
        pr.additions = 234;
        pr.deletions = 567;
        view.menu.pr_cache.seed(input.clone(), pr, now);
        view.git.seed_probe(input, true, now);
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.run_until_parked();
    for font_size in [12., 18.] {
        for width in [160., 232.] {
            // Switching density must restore the corresponding details and spacing.
            for mode in [Density::Compact, Density::Normal, Density::Comfortable]
                .into_iter()
                .flat_map(|density| {
                    [Style::Flat, Style::Rounded].map(|style| LayoutMode::new(density, style))
                })
            {
                let compact = mode.density() == Density::Compact;
                let comfortable = mode.density() == Density::Comfortable;
                let rounded = mode.style() == Style::Rounded;
                view.update(cx, |view, cx| {
                    view.config.layout.mode = mode;
                    view.config.sidebar.size = font_size;
                    view.sidebar_width = Some(width);
                    cx.notify();
                });
                cx.update(|window, cx| {
                    cx.default_global::<TextProbes>().0.clear();
                    window.refresh();
                    full_draw(window, cx).clear(cx);
                    let probes = &cx.global::<TextProbes>().0;
                    assert_eq!(probes.contains_key("main"), !compact);
                    for text in ["worktree/sidebar-child", "+234", "-567"] {
                        assert_eq!(probes.contains_key(text), comfortable, "{text}");
                    }
                    for text in ["Claude Code", "#7"] {
                        assert!(probes.contains_key(text), "{text}");
                    }
                });
                let line = font_size * 4. / 3.;
                let density_padding = match mode.density() {
                    Density::Compact => 6.,
                    Density::Normal => 8.,
                    Density::Comfortable => 12.,
                };
                // Rounded rows sit inside a highlight inset by the density's
                // gap, with a third of that gap as padding and twice it as
                // spacing between rows.
                let (inset, trim) = match (mode.style(), mode.density()) {
                    (Style::Flat, _) => (0., 0.),
                    (Style::Rounded, Density::Compact) => (4., 1.),
                    (Style::Rounded, Density::Normal) => (6., 2.),
                    (Style::Rounded, Density::Comfortable) => (8., 3.),
                };
                let padding = inset + density_padding;
                let vertical_padding = if comfortable { 4. } else { 0. } + trim;
                let spacing = 2. * trim;
                for (row, name, detail, highlight) in [
                    ("row-herdr", "name-herdr", "detail-herdr", "highlight-herdr"),
                    (
                        "row-agent-launcher",
                        "name-agent-launcher",
                        "detail-agent-launcher",
                        "highlight-agent-launcher",
                    ),
                    (
                        "row-sidebar-child",
                        "name-sidebar-child",
                        "detail-sidebar-child",
                        "highlight-sidebar-child",
                    ),
                    (
                        "row-agent-p0",
                        "name-agent-p0",
                        "detail-agent-p0",
                        "highlight-agent-p0",
                    ),
                ] {
                    let show_detail = row == "row-agent-p0"
                        || (!compact && (comfortable || row != "row-sidebar-child"));
                    let highlight = cx.debug_bounds(highlight).unwrap();
                    let row = cx.debug_bounds(row).unwrap();
                    let name = cx.debug_bounds(name).unwrap();
                    assert_eq!(
                        row.size.height,
                        px(line * if show_detail { 2. } else { 1. }
                            + 2. * vertical_padding
                            + spacing)
                    );
                    assert_eq!(name.top(), row.top() + px(vertical_padding + spacing / 2.));
                    // The highlight is the row less its inset and spacing, so a
                    // click between highlights still lands on a row.
                    assert_eq!(highlight.left(), row.left() + px(inset));
                    assert_eq!(highlight.right(), row.right() - px(inset));
                    assert_eq!(highlight.top(), row.top() + px(spacing / 2.));
                    assert_eq!(highlight.bottom(), row.bottom() - px(spacing / 2.));
                    assert!(name.right() <= row.right() - px(padding));
                    if show_detail {
                        assert!(cx.debug_bounds(detail).is_some());
                    }
                }
                let agent = cx.debug_bounds("row-agent-p0").unwrap();
                assert_eq!(
                    agent.size.height,
                    px(2. * line + 2. * vertical_padding + spacing)
                );
                assert!(cx.debug_bounds("detail-agent-p0").is_some());
                let row = cx.debug_bounds("row-sidebar-child").unwrap();
                let badge = cx.debug_bounds("pr-sidebar-child").unwrap();
                assert_eq!(badge.right(), row.right() - px(padding));
                assert!(badge.bottom() <= row.bottom());
                assert!(cx.debug_bounds("name-sidebar-child").unwrap().right() <= badge.left());
                assert!(cx.debug_bounds("dirty-sidebar-child").is_some());
                // Debug bounds outlive the element that recorded them, so a
                // rounded frame cannot prove tree lines absent here; see
                // `rounded_rows_drop_tree_lines_and_title_headers`.
                if !rounded {
                    let gutter = cx.debug_bounds("tree-sidebar-child").unwrap();
                    assert_eq!(
                        gutter.left(),
                        cx.debug_bounds("column-agent-launcher").unwrap().left()
                    );
                }
                let arrow = cx.debug_bounds("collapse-3").unwrap();
                let parent = cx.debug_bounds("row-agent-launcher").unwrap();
                assert!(arrow.top() >= parent.top() && arrow.bottom() <= parent.bottom());
            }
        }
    }
}

#[gpui::test]
fn rounded_rows_drop_tree_lines_and_title_headers(cx: &mut gpui::TestAppContext) {
    use crate::config::{Density, LayoutMode, Style};
    // A fresh window per mode: GPUI keeps debug bounds from earlier frames,
    // so absence is only observable when the element never rendered.
    for (style, header) in [(Style::Rounded, "Spaces"), (Style::Flat, "spaces")] {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot(6)));
            view.config.layout.mode = LayoutMode::new(Density::Normal, style);
            view
        });
        cx.simulate_resize(size(px(800.), px(900.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
        });
        assert!(cx.debug_bounds("row-sidebar-child").is_some());
        assert_eq!(
            cx.debug_bounds("tree-sidebar-child").is_some(),
            style == Style::Flat
        );
        let heading = cx.debug_bounds("header-spaces").unwrap();
        let row = cx.debug_bounds("row-herdr").unwrap();
        let column = cx.debug_bounds("column-herdr").unwrap();
        // Headings start where rows' status dots do, inside the highlight.
        let label = cx.debug_bounds("header-label-spaces").unwrap();
        assert_eq!(label.left(), column.left() - px(8. + 6.));
        assert!(heading.left() <= row.left());
        cx.update(|window, cx| {
            cx.default_global::<TextProbes>().0.clear();
            full_draw(window, cx).clear(cx);
            assert!(
                cx.global::<TextProbes>().0.contains_key(header),
                "{header}: {:?}",
                cx.global::<TextProbes>().0.keys()
            );
        });
        // The row, not its highlight, is the click target: the inset beside a
        // rounded highlight still selects the row, so there are no dead zones.
        let row = cx.debug_bounds("row-agent-launcher").unwrap();
        let highlight = cx.debug_bounds("highlight-agent-launcher").unwrap();
        let margin = point(row.left() + px(2.), row.center().y);
        assert_eq!(highlight.contains(&margin), style == Style::Flat);
        view.read_with(cx, |view, _| assert!(view.pending_navigation.is_none()));
        cx.simulate_click(margin, Default::default());
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.pending_navigation,
                Some(crate::NavigationTarget::Workspace("w3".into()))
            );
        });
    }
}

#[gpui::test]
fn multi_host_rows_scope_duplicate_ids_and_keep_agents_when_host_collapses(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot(1)));
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Remote".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = view.live.snapshot.clone();
            let remote_snapshot = Arc::make_mut(remote.live.snapshot.as_mut().unwrap());
            remote_snapshot.workspaces[0].label = "remote workspace".into();
            remote_snapshot.workspaces[0].branch = Some("remote branch".into());
            view.endpoints.push(remote);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = full_draw(window, cx);
    });
    for selector in [
        "host-local",
        "host-ssh:test",
        "workspace-local-w0",
        "workspace-ssh:test-w0",
        "agent-local-p0",
        "agent-ssh:test-p0",
        "github-herdr",
        "github-remote workspace",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing {selector}");
    }
    for (icon, title) in [
        ("github-herdr", "name-herdr"),
        ("github-remote workspace", "name-remote workspace"),
    ] {
        let icon = cx.debug_bounds(icon).unwrap();
        let title = cx.debug_bounds(title).unwrap();
        assert_eq!(icon.size, size(px(12.), px(12.)));
        assert_eq!(title.left(), icon.right() + px(6.));
        assert_eq!(
            title.size.width,
            px(super::LABEL_WIDTH - super::ICON_RESERVE)
        );
    }
    fixture.update(cx, |fixture, cx| {
        fixture.0.update(cx, |view, cx| {
            view.endpoints[1].collapsed = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        window.refresh();
        let _ = full_draw(window, cx);
        // The remote workspace row folds away -- its branch goes with it -- while
        // its agent keeps naming the host it runs on.
        assert!(!cx.global::<TextProbes>().0.contains_key("remote branch"));
        for part in ["Remote", "remote workspace", "tab 1"] {
            assert!(
                cx.global::<TextProbes>().0.contains_key(part),
                "{part}: {:?}",
                cx.global::<TextProbes>().0.keys()
            );
        }
    });
    assert!(cx.debug_bounds("workspace-local-w0").is_some());
    assert!(cx.debug_bounds("agent-ssh:test-p0").is_some());
}

/// A frame that renders every view. The sidebar is a cached view, which GPUI
/// replays without recording debug bounds; these tests measure layout, so each
/// of their frames is a full one, as every frame was before the cache.
#[cfg(test)]
pub(crate) fn full_draw(window: &mut Window, cx: &mut App) -> ArenaClearNeeded {
    window.refresh();
    window.draw(cx)
}

#[cfg(test)]
pub(crate) fn fixture_window(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    HerdrWindow {
        sound: Default::default(),
        updater: crate::updater::Updater::default(),
        update_preview: None,
        removal: None,
        selection: None,
        flash: None,
        configured_terminal_size: crate::config::Config::default().terminal.size,
        // Keep the original geometry fixture explicit; density-switching tests
        // above exercise all three modes independently of the default.
        config: crate::config::Config {
            layout: crate::config::Layout {
                mode: crate::config::LayoutMode::from(crate::config::Density::Comfortable),
                ..Default::default()
            },
            ..Default::default()
        },
        theme: Default::default(),
        config_load: None,
        config_watch: None,
        config_load_revision: 0,
        git: Default::default(),
        usage: Default::default(),
        sidebar_visible: true,
        device_filter: None,
        endpoints: vec![crate::endpoint::Endpoint::new(
            crate::endpoint::LOCAL.into(),
            "Local".into(),
            ConnectTarget::Socket("/unused-layout-test.sock".into()),
            true,
        )],
        selected_endpoint: 0,
        selection_epoch: 0,
        catalog: crate::endpoint::Catalog::new(&ConnectTarget::Socket(
            "/unused-layout-test.sock".into(),
        )),
        sessions: Default::default(),
        sessions_anchor: Default::default(),
        activation_deadline: None,
        pending_navigation: None,
        pending_toast: None,
        toasts_hidden: false,
        pending_releases: Vec::new(),
        selected_generation: 0,
        live: {
            let mut live = LiveState::default();
            live.snapshot = Some(Arc::new(snapshot(40)));
            live
        },
        focus: cx.focus_handle(),
        options: ConnectOptions::default(),
        last_queued_options: None,
        pending_resize: None,
        active: false,
        sent_focus: None,
        bounds: Bounds::default(),
        title: crate::WINDOW_TITLE.to_owned(),
        cell_width: 9.,
        hovered_terminal_link: false,
        pressed_terminal_link: None,
        terminal_mouse: None,
        scrollbar_drag: None,
        split_drag: None,
        split_cursor: None,
        pending_images: Vec::new(),
        file_transfer: None,
        presentation: Default::default(),
        painter: Default::default(),
        marked: String::new(),
        hover: None,
        hover_menu: None,
        local_error: None,
        menu: crate::menu::MenuState::new(cx),
        install_warning_shown: false,
        collapsed_repos: Default::default(),
        wheel: WheelAccumulator::default(),
        sidebar_width: None,
        sidebar_drag: None,
        opacity_drag: false,
        blur_drag: false,
        background_save: None,
        pending_background_save: None,
        last_window_background: None,
        #[cfg(target_os = "macos")]
        native_blur: None,
        workspace_drag: None,
        sidebar_split: None,
        sidebar_split_modified: false,
        sidebar_preferences: None,
        sidebar_modified: false,
        agent_sort: Default::default(),
        agent_sort_modified: false,
        avatars: None,
        #[cfg(feature = "integration-test")]
        input_probe: crate::smoke::InputProbe::default(),
        sidebar_scroll: Default::default(),
        sidebar_revealed: Default::default(),
        _poll: Task::ready(()),
        _activation: cx.observe_window_activation(window, |_, _, _| {}),
        sidebar_view: {
            let weak = cx.weak_entity();
            cx.new(|_| crate::sidebar::SidebarView::new(weak))
        },
        surface_signal: cx.new(|_| crate::window::SurfaceSignal),
        _sidebar_invalidation: HerdrWindow::invalidate_sidebar(cx),
    }
}

#[gpui::test]
fn palette_rejects_changed_endpoint_epoch_or_generation(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    for reconnect in [false, true] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_palette(false, window, cx));
            full_draw(window, cx).clear(cx);
        });
        cx.simulate_input("toggle sidebar");
        cx.run_until_parked();
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        // Selection paints as a row: the fill spans the list, not the label.
        let row = cx.debug_bounds("palette-row-0").unwrap();
        let status = cx.debug_bounds("palette-status").unwrap();
        assert_eq!(row.size.width, status.size.width);
        assert_eq!(row.left(), status.left());
        view.update(cx, |view, _| {
            assert!(view.menu_target_current());
            if reconnect {
                view.endpoints[view.selected_endpoint].generation += 1;
            } else {
                view.selection_epoch += 1;
            }
            assert!(!view.menu_target_current());
        });
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            assert!(view.read(cx).sidebar_visible);
            assert!(view.read(cx).menu.page == Some(crate::menu::Page::Palette));
        });
        cx.simulate_keystrokes("escape");
    }
}

#[cfg(test)]
fn check_sidebar(fixture: Entity<SidebarFixture>, cx: &mut gpui::VisualTestContext) -> Result<()> {
    use anyhow::Context as _;
    use gpui::{Modifiers, MouseButton, MouseDownEvent, point};
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let _ = full_draw(window, cx);
    });

    cx.update(|_, cx| {
        for (input, (bounds, rendered, width)) in &cx.global::<TextProbes>().0 {
            eprintln!(
                "text {input:?}: bounds={bounds:?}, rendered={rendered:?}, glyph width={width:?}"
            );
            assert!(
                *width <= bounds.size.width,
                "glyphs must fit the allocation"
            );
        }
        // Each part of an agent's line is painted on its own, so the tab can
        // stay muted beside its workspace.
        for input in ["herdr", "main", "tab 1", "Claude Code"] {
            let (bounds, rendered, _) = &cx.global::<TextProbes>().0[input];
            assert_eq!(
                rendered, input,
                "short label must not ellipsize: {bounds:?}"
            );
        }
        for input in [
            "herdr-gpui-sidebar-rendering-regression-investigation",
            "fix/sidebar-label-width-and-overflow-regression",
        ] {
            let (bounds, rendered, width) = &cx.global::<TextProbes>().0[input];
            assert!(bounds.size.width > px(150.));
            assert!(*width > px(150.), "long labels must use available width");
            assert_eq!(bounds.size.height, px(16.));
            assert!(
                rendered.ends_with('\u{2026}'),
                "long label must ellipsize: {rendered:?}"
            );
            let prefix = rendered.trim_end_matches('\u{2026}');
            assert!(prefix.len() > 10 && input.starts_with(prefix));
            assert!(rendered.len() < input.len());
            assert!(!rendered.contains('\n'));
        }
    });

    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let spaces = cx.debug_bounds("spaces-scroll").unwrap();
    let agents = cx.debug_bounds("agents-scroll").unwrap();
    assert_eq!(sidebar.size.width, px(232.));
    let icon = cx.debug_bounds("github-herdr").unwrap();
    let title = cx.debug_bounds("name-herdr").unwrap();
    let detail = cx.debug_bounds("detail-herdr").unwrap();
    assert_eq!(icon.size, size(px(12.), px(12.)));
    assert_eq!(title.left(), icon.right() + px(6.));
    assert_eq!(icon.left(), detail.left());
    assert_eq!(title.right(), detail.right());
    assert!(cx.debug_bounds("github-agent-launcher").is_some());
    assert!(cx.debug_bounds("github-sidebar-child").is_none());
    assert!(cx.debug_bounds("github-review").is_none());
    let footer = cx.debug_bounds("device-footer").unwrap();
    assert!(spaces.size.height + footer.size.height / 2. > px(200.));
    assert!(agents.size.height + footer.size.height / 2. > px(200.));
    assert!(agents.bottom() <= footer.top());
    let parent = cx.debug_bounds("name-agent-launcher").unwrap();
    for (name, detail) in [
        ("name-sidebar-child", "detail-sidebar-child"),
        (
            "name-sidebar-child-with-a-long-readable-branch-name",
            "detail-sidebar-child-with-a-long-readable-branch-name",
        ),
    ] {
        let name = cx.debug_bounds(name).unwrap();
        let detail = cx.debug_bounds(detail).unwrap();
        assert_eq!(
            name.left(),
            parent.left() + px(super::CHILD_INDENT - super::ICON_RESERVE)
        );
        assert_eq!(
            name.size.width,
            px(super::LABEL_WIDTH - super::CHILD_INDENT - super::ARROW_RESERVE)
        );
        assert_eq!(name.right(), parent.right());
        assert_eq!(detail.size.width, name.size.width);
        assert_eq!(name.size.height, px(16.));
    }

    for (row, column, name, detail) in [
        ("row-herdr", "column-herdr", "name-herdr", "detail-herdr"),
        (
            "row-herdr-gpui-sidebar-rendering-regression-investigation",
            "column-herdr-gpui-sidebar-rendering-regression-investigation",
            "name-herdr-gpui-sidebar-rendering-regression-investigation",
            "detail-herdr-gpui-sidebar-rendering-regression-investigation",
        ),
        (
            "row-agent-p0",
            "column-agent-p0",
            "name-agent-p0",
            "detail-agent-p0",
        ),
        (
            "row-agent-p1",
            "column-agent-p1",
            "name-agent-p1",
            "detail-agent-p1",
        ),
    ] {
        let row_bounds = cx.debug_bounds(row).unwrap();
        let column_bounds = cx.debug_bounds(column).unwrap();
        let name_bounds = cx.debug_bounds(name).unwrap();
        let detail_bounds = cx.debug_bounds(detail).unwrap();
        eprintln!(
            "{row}: row={row_bounds:?}, column={column_bounds:?}, name={name_bounds:?}, detail={detail_bounds:?}"
        );
        assert!(name_bounds.size.width > px(150.), "{name}: {name_bounds:?}");
        assert!(
            detail_bounds.size.width > px(150.),
            "{detail}: {detail_bounds:?}"
        );
        assert_eq!(name_bounds.size.height, px(16.), "single-line name");
        assert_eq!(detail_bounds.size.height, px(16.), "single-line detail");
        assert_eq!(row_bounds.size.height, px(40.));
        assert!(name_bounds.right() <= sidebar.right() - px(12.));
        assert!(detail_bounds.right() <= sidebar.right() - px(12.));
        assert!(
            row_bounds.bottom() <= sidebar.bottom(),
            "visible initial rows"
        );
    }

    // Drag beyond the divider, then back to a narrower allocation. Text must
    // be remeasured in both directions rather than retaining truncated runs.
    for target in [400., 160., 480.] {
        let divider = cx.debug_bounds("sidebar-resize").unwrap();
        let start = divider.center();
        let old_width = cx.debug_bounds("sidebar").unwrap().size.width;
        let end = point(start.x + px(target) - old_width, start.y);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            fixture.update(cx, |_, cx| cx.notify());
            let _ = full_draw(window, cx);
        });
        assert_eq!(cx.debug_bounds("sidebar").unwrap().size.width, px(target));
        let label = cx.debug_bounds("name-herdr").unwrap();
        assert_eq!(
            label.size.width,
            px(super::LABEL_WIDTH + target - 232. - super::ICON_RESERVE)
        );
        let parent = cx.debug_bounds("name-agent-launcher").unwrap();
        let child = cx.debug_bounds("name-sidebar-child").unwrap();
        assert_eq!(
            parent.size.width,
            label.size.width - px(super::ARROW_RESERVE)
        );
        assert_eq!(
            child.size.width,
            parent.size.width - px(super::CHILD_INDENT) + px(super::ICON_RESERVE)
        );
        assert_eq!(child.right(), parent.right());
        cx.update(|_, cx| {
            for (text, (bounds, rendered, glyph_width)) in &cx.global::<TextProbes>().0 {
                // GPUI rounds available text width to physical pixels.
                assert!(*glyph_width <= bounds.size.width + px(1.), "width={target}, text={text:?}, rendered={rendered:?}, bounds={bounds:?}, glyphs={glyph_width:?}");
            }
        });
        cx.simulate_mouse_move(point(px(600.), start.y), None, Modifiers::default());
        cx.update(|window, cx| {
            fixture.update(cx, |_, cx| cx.notify());
            let _ = full_draw(window, cx);
        });
        assert_eq!(cx.debug_bounds("sidebar").unwrap().size.width, px(target));
    }
    cx.simulate_resize(size(px(640.), px(600.)));
    cx.update(|window, cx| {
        let _ = full_draw(window, cx);
    });
    assert_eq!(cx.debug_bounds("sidebar").unwrap().size.width, px(400.));
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        let _ = full_draw(window, cx);
    });
    assert_eq!(cx.debug_bounds("sidebar").unwrap().size.width, px(480.));

    let position = cx.debug_bounds("sidebar-resize").unwrap().center();
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.update(|window, cx| {
        fixture.update(cx, |_, cx| cx.notify());
        let _ = full_draw(window, cx);
    });
    assert_eq!(cx.debug_bounds("sidebar").unwrap().size.width, px(232.));

    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let before = cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_workspace_id = Some("w4".into());
            for workspace in &mut snapshot.workspaces {
                workspace.focused = workspace.workspace_id == "w4";
            }
            view.marked = "selection must survive toggle".into();
            snapshot.clone()
        })
    });
    for collapsed in [true, false] {
        let arrow = cx.debug_bounds("collapse-3").unwrap();
        cx.simulate_click(arrow.center(), Default::default());
        cx.update(|window, cx| {
            cx.default_global::<TextProbes>().0.clear();
            window.refresh();
            full_draw(window, cx).clear(cx);
            let view = view.read(cx);
            assert_eq!(view.live.snapshot.as_deref(), Some(&before));
            assert_eq!(view.marked, "selection must survive toggle");
            assert!(cx.global::<TextProbes>().0.contains_key(if collapsed {
                "\u{25b8}"
            } else {
                "\u{25be}"
            }));
            assert_eq!(view.collapsed_repos.contains(REPO_KEY), collapsed);
            assert_eq!(
                !cx.global::<TextProbes>().0.contains_key("sidebar-child"),
                collapsed
            );
        });
    }
    let menu = cx.debug_bounds("sidebar-menu").unwrap();
    cx.simulate_click(menu.center(), Default::default());
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("menu-panel").is_some());
    assert!(cx.debug_bounds("menu-reload GUI config").is_some());
    crate::menu::workspace_tests::check_menu_interactions(&view, cx);
    cx.simulate_keystrokes("down down enter");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Keybinds));
    });
    let panel = cx.debug_bounds("menu-panel").unwrap();
    assert_eq!(panel.size.width, px(480.));
    assert_eq!(panel.center(), point(px(400.), px(300.)));
    let first_description = cx.debug_bounds("description-New Workspace").unwrap();
    for (keys, label) in [
        ("keys-New Workspace", "description-New Workspace"),
        ("keys-New Tab", "description-New Tab"),
        ("keys-Split Right", "description-Split Right"),
        ("keys-Split Down", "description-Split Down"),
    ] {
        let keys = cx.debug_bounds(keys).unwrap();
        let label = cx.debug_bounds(label).unwrap();
        assert!(keys.right() < label.left());
        assert_eq!(label.left(), first_description.left());
        assert!(label.right() < panel.right());
    }
    cx.simulate_resize(size(px(360.), px(240.)));
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
    });
    let panel = cx.debug_bounds("menu-panel").unwrap();
    assert_eq!(panel.size.width, px(328.));
    assert!(panel.size.height <= px(208.));
    assert_eq!(panel.center(), point(px(180.), px(120.)));
    let header = cx.debug_bounds("keybinds-header").unwrap();
    let footer = cx.debug_bounds("keybinds-footer").unwrap();
    let body = cx.debug_bounds("keybinds-body").unwrap();
    assert!(body.size.height > px(0.));
    assert!(header.bottom() <= body.top());
    assert!(body.bottom() <= footer.top());
    assert!(footer.bottom() <= panel.bottom());
    let first_row = cx.debug_bounds("shortcut-New Workspace").unwrap();
    cx.simulate_keystrokes("pagedown");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("shortcut-New Workspace").unwrap().top() < first_row.top());
    assert_eq!(cx.debug_bounds("keybinds-header").unwrap(), header);
    assert_eq!(cx.debug_bounds("keybinds-footer").unwrap(), footer);
    let close = cx.debug_bounds("keybinds-close").unwrap();
    cx.simulate_click(close.center(), Default::default());
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).focus.is_focused(window));
        view.update(cx, |view, cx| view.open_keybinds(window, cx));
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(
        cx.debug_bounds("shortcut-New Workspace").unwrap(),
        first_row
    );
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page.is_none());
    });
    cx.simulate_click(menu.center(), Default::default());
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_click(point(px(700.), px(500.)), Default::default());
    cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));

    // Exercise the actual right-click overlay and platform text handler, without a daemon.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.live.status = crate::state::ConnectionStatus::Connected;
        })
    });
    let parent = cx.debug_bounds("row-agent-launcher").unwrap();
    cx.simulate_mouse_down(parent.center(), MouseButton::Right, Default::default());
    cx.simulate_mouse_up(parent.center(), MouseButton::Right, Default::default());
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Workspace));
        assert_eq!(view.read(cx).live.snapshot.as_deref(), Some(&before));
    });
    assert!(cx.debug_bounds("workspace-menu-Close group").is_some());
    assert!(cx.debug_bounds("workspace-menu-New worktree").is_some());
    // Every action is labelled and pictured, with the icon left of its label.
    for (row, icon) in [
        ("workspace-menu-Rename", "workspace-menu-icon-Rename"),
        (
            "workspace-menu-Close group",
            "workspace-menu-icon-Close group",
        ),
        (
            "workspace-menu-New worktree",
            "workspace-menu-icon-New worktree",
        ),
        (
            "workspace-menu-Open worktree...",
            "workspace-menu-icon-Open worktree...",
        ),
    ] {
        let label = row;
        let row = cx.debug_bounds(row).unwrap();
        let icon = cx.debug_bounds(icon).unwrap();
        assert_eq!(icon.size, size(px(14.), px(14.)), "{label}");
        assert!(icon.left() >= row.left(), "{label}");
        assert!(icon.right() <= row.right(), "{label}");
        assert!(
            (icon.center().y - row.center().y).abs() <= px(1.),
            "{label}"
        );
    }
    crate::menu::workspace_tests::check_menu_interactions(&view, cx);
    // PR data is fixture-only: no daemon, local Git, or GitHub calls in layout tests.
    crate::menu::workspace_tests::check_pr_fences(&view, cx);
    for width in [320., 800.] {
        cx.simulate_resize(size(px(width), px(600.)));
        for state in 0..5 {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.menu.pr.clear();
                    view.menu.pr.loading = state == 0;
                    if state >= 2 {
                        view.menu.github = crate::github::Auth::connected_fixture();
                        view.menu.pr.value = Some(crate::pull_request::fixture().unwrap());
                    }
                    if state == 3 {
                        view.menu.pr.message = Some("Authentication unavailable".into());
                    }
                    cx.notify();
                });
                full_draw(window, cx).clear(cx);
            });
            let panel = cx.debug_bounds("menu-panel").unwrap();
            assert!(panel.left() >= px(0.) && panel.right() <= px(width));
            assert!(panel.bottom() <= px(600.));
            let open_row = cx.debug_bounds("workspace-menu-Open worktree...").unwrap();
            // Preserve the content budget apart from the action row and target header.
            let row_height = cx.update(|_, cx| px(view.read(cx).config.ui.line_height() + 12.));
            let header_height = cx
                .debug_bounds("workspace-menu-header")
                .unwrap()
                .size
                .height
                + px(4.);
            assert!((open_row.size.height - row_height).abs() <= px(1.));
            assert!(
                panel.size.height < px(320.) + row_height + header_height,
                "PR menu should size to its content: {panel:?}"
            );
            assert!(cx.debug_bounds("workspace-pr").is_some());
            if state >= 2 {
                let title = cx.debug_bounds("workspace-pr-title").unwrap();
                assert!(title.left() >= panel.left() && title.right() <= panel.right());
            }
        }
    }
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.menu.pr.clear());
    });
    for dialog in [false, true] {
        if dialog {
            cx.simulate_keystrokes("down enter");
        }
        for anchor in [
            point(px(200.), px(400.)),
            point(px(795.), px(595.)),
            point(px(-10.), px(-20.)),
        ] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.menu.anchor = anchor;
                    cx.notify();
                });
                full_draw(window, cx).clear(cx);
            });
            let panel = cx.debug_bounds("menu-panel").unwrap();
            assert_eq!(panel.size.width, px(if dialog { 420. } else { 340. }));
            if dialog {
                // A dialog is a modal decision, so it centres on the window and
                // ignores the anchor the row menu was opened from.
                let offset = panel.center() - point(px(400.), px(300.));
                assert!(
                    offset.x.abs() <= px(1.) && offset.y.abs() <= px(1.),
                    "{anchor:?}: {panel:?}"
                );
            } else {
                let expected = |position: Pixels, extent: Pixels, viewport: Pixels| {
                    if position + extent > viewport {
                        (viewport - extent - px(12.)).round()
                    } else if position < px(0.) {
                        px(12.)
                    } else {
                        position.round()
                    }
                };
                assert_eq!(panel.left(), expected(anchor.x, panel.size.width, px(800.)));
                assert_eq!(panel.top(), expected(anchor.y, panel.size.height, px(600.)));
            }
            assert!(panel.right() <= px(800.) && panel.bottom() <= px(600.));
        }
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.anchor = parent.center();
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_input("\u{65e5}\u{672c}\u{1f600}");
    cx.update(|window, cx| {
        use gpui::EntityInputHandler;
        view.update(cx, |view, cx| {
            assert_eq!(
                view.menu.input.as_ref().unwrap().text,
                "\u{65e5}\u{672c}\u{1f600}"
            );
            view.replace_and_mark_text_in_range(Some(2..4), "\u{304b}", Some(1..1), window, cx);
            assert_eq!(view.marked_text_range(window, cx), Some(2..3));
            assert_eq!(
                view.selected_text_range(false, window, cx).unwrap().range,
                3..3
            );
            view.replace_text_in_range(None, "\u{6f22}", window, cx);
            assert_eq!(
                view.menu.input.as_ref().unwrap().text,
                "\u{65e5}\u{672c}\u{6f22}"
            );
            assert!(view.marked.is_empty());
            view.command(crate::controls::Command::Workspace, window, cx);
            assert!(view.local_error.is_none());
        });
        full_draw(window, cx).clear(cx);
        view.update(cx, |view, cx| {
            let bounds = view
                .bounds_for_range(3..3, Bounds::default(), window, cx)
                .unwrap();
            assert!(
                view.menu
                    .input
                    .as_ref()
                    .unwrap()
                    .bounds
                    .contains(&bounds.origin)
            );
        });
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        // No handle: a queue failure must preserve the draft, not claim success.
        assert!(
            view.read(cx).menu.page
                == Some(crate::menu::Page::Dialog(
                    crate::menu::WorkspaceAction::Rename
                ))
        );
    });
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("   ");
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(view.read(cx).menu.input.as_ref().unwrap().text, "   ");
        assert!(
            view.read(cx).menu.page
                == Some(crate::menu::Page::Dialog(
                    crate::menu::WorkspaceAction::Rename
                ))
        );
    });
    assert!(cx.debug_bounds("dialog-error").is_some());
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).menu.input.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar.size = 24.;
            view.marked = "composition".into();
            view.open_keybinds(window, cx);
            assert!(view.marked.is_empty());
        });
        full_draw(window, cx).clear(cx);
        assert!(!view.read(cx).focus.is_focused(window));
    });
    let line_height = cx.update(|_, cx| super::line_height(&view.read(cx).config.sidebar));
    assert_eq!(
        cx.debug_bounds("row-herdr").unwrap().size.height,
        px(2. * line_height + 8.)
    );
    assert_eq!(
        cx.debug_bounds("name-herdr").unwrap().size.height,
        px(line_height)
    );
    let title = cx.debug_bounds("name-herdr").unwrap();
    let detail = cx.debug_bounds("detail-herdr").unwrap();
    let icon = cx.debug_bounds("github-herdr").unwrap();
    assert_eq!(title.bottom(), detail.top());
    assert_eq!(detail.size.height, px(line_height));
    assert_eq!(
        title.size.width,
        px(super::LABEL_WIDTH - super::ICON_RESERVE)
    );
    assert_eq!(title.right(), detail.right());
    assert_eq!(icon.center().y, title.center().y);
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
    cx.simulate_keystrokes("cmd-/");
    let shortcut_search = cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let search = view.read(cx).menu.keybinds_search.as_ref().unwrap().clone();
        assert!(search.read(cx).focus.is_focused(window));
        search
    });
    cx.simulate_input("pane zoom");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(shortcut_search.read(cx).text(), "pane zoom");
    });
    assert!(cx.debug_bounds("shortcut-Toggle Pane Zoom").is_some());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("no-shortcut-matches-xyz");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("keybinds-empty").is_some());
    cx.simulate_keystrokes("cmd-w");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Keybinds)));
    cx.simulate_keystrokes("escape cmd-/");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let search = view.read(cx).menu.keybinds_search.as_ref().unwrap().clone();
        assert!(search.read(cx).text().is_empty());
        search.update(cx, |input, cx| {
            gpui::EntityInputHandler::replace_and_mark_text_in_range(
                input,
                None,
                "pane",
                Some(4..4),
                window,
                cx,
            )
        });
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Keybinds));
        let search = view.read(cx).menu.keybinds_search.as_ref().unwrap().clone();
        search.update(cx, |input, cx| {
            gpui::EntityInputHandler::unmark_text(input, window, cx)
        });
    });
    cx.simulate_keystrokes("escape cmd-,");
    cx.simulate_resize(size(px(360.), px(240.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let header = cx.debug_bounds("preferences-header").unwrap();
    let footer = cx.debug_bounds("preferences-footer").unwrap();
    let body = cx.debug_bounds("preferences-body").unwrap();
    let theme_row = cx.debug_bounds("preferences-theme").unwrap();
    assert!(body.size.height > px(0.));
    assert!(header.bottom() <= body.top());
    assert!(body.bottom() <= footer.top());
    cx.simulate_keystrokes("pagedown");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("preferences-theme").unwrap().top() < theme_row.top());
    assert_eq!(cx.debug_bounds("preferences-header").unwrap(), header);
    assert_eq!(cx.debug_bounds("preferences-footer").unwrap(), footer);
    let close = cx.debug_bounds("preferences-close").unwrap();
    cx.simulate_click(close.center(), Default::default());
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));
    for width in [320., 640., 1200.] {
        cx.simulate_resize(size(px(width), px(400.)));
        for state in 0..5 {
            cx.update(|window, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("unchanged".into()));
                cx.default_global::<PaintedProbes>().0.clear();
                view.update(cx, |view, cx| {
                    view.github_fixture(state == 1, window, cx);
                    if state == 2 {
                        view.menu.github.failed = true;
                        view.menu.github.message =
                            Some("GitHub code expired. Sign in again. ".repeat(40));
                    } else if state == 3 {
                        view.menu.github = crate::github::Auth::connected_fixture();
                    } else if state == 4 {
                        view.menu.github = crate::github::Auth::requesting_fixture();
                    }
                });
                full_draw(window, cx).clear(cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("unchanged")
                );
                assert_eq!(
                    cx.global::<PaintedProbes>().0.contains_key("Sign out (D)"),
                    state == 3
                );
            });
            let panel = cx.debug_bounds("menu-panel").unwrap();
            assert!(panel.left() >= px(0.) && panel.right() <= px(width));
            assert!(panel.bottom() <= px(400.));
            assert!(panel.size.width <= px(400.));
            assert!(cx.debug_bounds("github-close").is_none());
            let close = cx.debug_bounds("github-header-close").unwrap();
            assert!(close.top() >= panel.top() && close.bottom() <= panel.bottom());
            if state == 3 {
                assert!(panel.size.height <= px(230.));
            }
            let footer = cx.debug_bounds("github-footer");
            let body = cx.debug_bounds("github-body").unwrap();
            assert!(body.size.height > px(0.));
            // A pending request offers no footer actions, so none is drawn.
            assert_eq!(footer.is_none(), state == 4);
            if let Some(footer) = footer {
                // Content-sized layouts can round adjacent edges to half pixels.
                assert!(body.bottom() <= footer.top() + px(1.));
                assert!(footer.bottom() <= panel.bottom() + px(1.));
            } else {
                assert!(body.bottom() <= panel.bottom() + px(1.));
            }
            if state == 1 {
                let code = cx.debug_bounds("github-device-code").unwrap();
                assert!(code.left() >= panel.left() && code.right() <= panel.right());
                let copy = cx.debug_bounds("github-copy").unwrap();
                cx.simulate_click(copy.center(), Default::default());
                cx.update(|_, cx| {
                    assert_eq!(
                        cx.read_from_clipboard().unwrap().text().as_deref(),
                        Some("ABCD-1234")
                    );
                    assert!(view.read(cx).menu.github.copied());
                });
                cx.simulate_keystrokes("tab enter");
                cx.update(|_, cx| assert!(view.read(cx).menu.github.copied()));
                cx.simulate_keystrokes("cmd-c");
                let open = cx.debug_bounds("github-open").unwrap();
                cx.simulate_click(open.center(), Default::default());
                assert_eq!(cx.opened_url().as_deref(), Some(crate::github::VERIFY_URL));
            } else if state == 2 {
                let status = cx.debug_bounds("github-status").unwrap();
                cx.simulate_keystrokes("pagedown");
                cx.update(|window, cx| full_draw(window, cx).clear(cx));
                assert!(cx.debug_bounds("github-status").unwrap().top() < status.top());
                assert_eq!(cx.debug_bounds("github-footer"), footer);
            }
            cx.simulate_keystrokes("c escape");
            cx.update(|window, cx| {
                assert!(view.read(cx).menu.page.is_none());
                assert!(view.read(cx).focus.is_focused(window));
                assert!(view.read(cx).menu.github.code().is_none());
                assert!(!view.read(cx).menu.github.copied());
            });
        }
    }
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.simulate_keystrokes("cmd-,");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let choose_theme = cx.debug_bounds("preferences-choose-theme").unwrap();
    cx.simulate_click(choose_theme.center(), Default::default());
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Themes)));
    cx.simulate_keystrokes("escape");

    let search = cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_theme_picker(window, cx));
        full_draw(window, cx).clear(cx);
        let search = view.read(cx).menu.themes.as_ref().unwrap().search.clone();
        assert!(search.read(cx).focus.is_focused(window));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("catppuccin mocha".into()));
        search
    });
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(search.read(cx).text(), "catppuccin mocha");
        assert!(view.read(cx).marked.is_empty());
    });
    assert!(cx.debug_bounds("theme-name-Catppuccin Mocha").is_some());
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).menu.themes.as_ref().unwrap().filtered,
            ["Catppuccin Mocha"]
        );
    });
    cx.simulate_keystrokes("cmd-a n o r d");
    cx.run_until_parked();
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(search.read(cx).text(), "nord");
        assert!(
            view.read(cx)
                .menu
                .themes
                .as_ref()
                .unwrap()
                .filtered
                .iter()
                .all(|name| name.to_lowercase().contains("nord"))
        );
    });
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("no-such-theme-xyz".into()))
    });
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("theme-empty").is_some());
    // Enter with no results must neither write a config nor dismiss the picker.
    cx.simulate_keystrokes("down enter");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Themes)));
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).focus.is_focused(window));
        view.update(cx, |view, cx| view.open_theme_picker(window, cx));
        full_draw(window, cx).clear(cx);
        assert!(search.read(cx).text().is_empty());
    });
    cx.update(|window, cx| {
        search.update(cx, |search, cx| {
            gpui::EntityInputHandler::replace_and_mark_text_in_range(
                search,
                None,
                "Nord",
                Some(4..4),
                window,
                cx,
            );
        });
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert!(
            view.read(cx).menu.page == Some(crate::menu::Page::Themes),
            "IME confirmation must not apply a theme"
        );
    });
    cx.update(|window, cx| {
        search.update(cx, |search, cx| {
            gpui::EntityInputHandler::unmark_text(search, window, cx)
        });
    });
    cx.simulate_keystrokes("escape");

    cx.simulate_keystrokes("cmd-shift-p");
    let palette_search = cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Palette));
        view.read(cx).menu.palette.as_ref().unwrap().search.clone()
    });
    // Bound native commands must not fire while a search field has focus.
    cx.simulate_keystrokes("cmd-b");
    cx.update(|_, cx| assert!(view.read(cx).sidebar_visible));
    cx.simulate_input("toggle sidebar");
    cx.update(|_, cx| assert_eq!(palette_search.read(cx).text(), "toggle sidebar"));
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(!view.read(cx).sidebar_visible);
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });
    cx.simulate_keystrokes("cmd-b cmd-,");
    cx.update(|_, cx| {
        assert!(view.read(cx).sidebar_visible);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Preferences));
    });
    cx.simulate_keystrokes("escape cmd-p");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Palette));
        let search = &view.read(cx).menu.palette.as_ref().unwrap().search;
        assert!(search.read(cx).text().is_empty());
    });
    cx.simulate_input("no-workspace-matches-xyz");
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::Palette)));
    cx.simulate_keystrokes("escape");

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.live.snapshot = Some(Arc::new(
                serde_json::from_str(include_str!(
                    "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
                ))
                .unwrap(),
            ));
            cx.notify();
        });
    });
    cx.simulate_keystrokes("cmd-w");
    cx.update(|_, cx| assert!(view.read(cx).menu.page == Some(crate::menu::Page::ConfirmClose)));
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert!(
            view.read(cx).menu.page.is_none(),
            "Enter defaults to Cancel"
        )
    });
    cx.simulate_keystrokes("cmd-shift-w tab enter");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(
            view.read(cx).menu.page == Some(crate::menu::Page::ConfirmClose),
            "disconnected confirmation stays open with error"
        );
        let view = view.read(cx);
        assert!(
            view.endpoints[view.selected_endpoint]
                .connection
                .handle
                .is_none()
        );
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| assert!(view.read(cx).focus.is_focused(window)));

    let before_install = cx.update(|_, cx| view.read(cx).live.snapshot.clone());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.show_install_modal(window, cx));
    });
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let view = view.read(cx);
        assert!(view.menu.page == Some(crate::menu::Page::Install));
        assert!(!view.live.missing_installation);
        assert_eq!(view.live.snapshot, before_install);
    });
    assert!(cx.debug_bounds("menu-install").is_some());
    assert!(cx.debug_bounds("menu-dismiss").is_some());
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));

    // Fixtures have no updater worker, and unavailable updates use the shared panel.
    let updater_before = cx.update(|_, cx| view.read(cx).updater.state().clone());
    assert!(matches!(updater_before, crate::updater::State::Disabled(_)));
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::CheckForUpdates), cx));
    assert!(cx.pending_prompt().is_none());
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::AppUpdate));
        assert_eq!(view.read(cx).live.snapshot, before_install);
    });
    assert!(cx.debug_bounds("app-update-action").is_none());
    let releases = cx
        .debug_bounds("app-update-releases")
        .context("update releases bounds")?;
    cx.simulate_click(releases.center(), Default::default());
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/penso/herdr-gpui/releases")
    );
    let close = cx
        .debug_bounds("app-update-close")
        .context("update close bounds")?;
    cx.simulate_click(close.center(), Default::default());
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.focus.is_focused(window));
        assert_eq!(view.updater.state(), &updater_before);
    });
    for (width, height) in [(320., 360.), (320., 600.), (480., 600.), (800., 600.)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.update(|window, cx| window.dispatch_action(Box::new(crate::ShowUpdatePreview), cx));
        for ready in [false, true] {
            cx.update(|window, cx| {
                full_draw(window, cx).clear(cx);
                let view = view.read(cx);
                assert_eq!(view.updater.state(), &updater_before);
                assert_eq!(view.live.snapshot, before_install);
                assert_eq!(
                    view.update_preview,
                    Some(if ready {
                        crate::updater::State::Ready {
                            version: "9999.0.0".into(),
                        }
                    } else {
                        crate::updater::State::Available {
                            version: "9999.0.0".into(),
                        }
                    })
                );
            });
            let panel = cx
                .debug_bounds("app-update-panel")
                .context("update panel bounds")?;
            let action = cx
                .debug_bounds("app-update-action")
                .context("update action bounds")?;
            let header = cx
                .debug_bounds("app-update-header")
                .context("update header bounds")?;
            let close = cx
                .debug_bounds("app-update-close")
                .context("update close bounds")?;
            assert_eq!(close.right(), header.right() - px(16.));
            assert!(close.left() > header.center().x);
            assert!(close.top() >= header.top() && close.bottom() <= header.bottom());
            assert!(header.bottom() < action.top());
            let body = cx
                .debug_bounds("app-update-body")
                .context("update body bounds")?;
            let footer = cx
                .debug_bounds("app-update-footer")
                .context("update footer bounds")?;
            let current = cx
                .debug_bounds("app-update-current-version")
                .context("current version bounds")?;
            let latest = cx
                .debug_bounds("app-update-latest-version")
                .context("latest version bounds")?;
            assert_eq!(current.left(), latest.left());
            assert_eq!(current.right(), latest.right());
            assert!(current.bottom() < latest.top());
            assert_eq!(header.left(), panel.left());
            assert_eq!(header.right(), panel.right());
            assert!(body.top() >= header.bottom());
            assert!((footer.top() - body.bottom()).abs() <= px(1.));
            assert!(panel.top() >= px(0.) && panel.bottom() <= px(height));
            assert!(action.top() >= footer.top() && action.bottom() <= footer.bottom());
            assert!(panel.left() >= px(0.) && panel.right() <= px(width));
            assert!(action.left() >= panel.left() && action.right() <= panel.right());
            assert!(action.top() >= panel.top() && action.bottom() <= panel.bottom());
            cx.simulate_click(action.center(), Default::default());
        }
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.menu.page.is_none());
            assert!(view.update_preview.is_none());
            assert_eq!(view.updater.state(), &updater_before);
        });
        assert!(cx.pending_prompt().is_none());
    }
    // The same panel is reachable without native menus, including on Linux.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_menu(window, cx));
        full_draw(window, cx).clear(cx);
    });
    let updates = cx
        .debug_bounds("menu-app updates")
        .context("app updates menu bounds")?;
    assert!(cx.debug_bounds("menu-preview app update").is_some());
    cx.simulate_click(updates.center(), Default::default());
    cx.update(|_, cx| {
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::AppUpdate));
        assert!(view.read(cx).update_preview.is_none());
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::ShowUpdatePreview), cx));
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.update_preview.is_none());
        assert!(view.focus.is_focused(window));
        assert_eq!(view.updater.state(), &updater_before);
    });
    // Exercise the real status bar without starting a daemon connection.
    view.update(cx, |view, cx| {
        view.marked = "composition ".repeat(100);
        view.local_error = Some("long connection error ".repeat(100));
        cx.notify();
    });
    for width in [480., 800.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let status = cx.debug_bounds("connection-status").unwrap();
        let report = cx.debug_bounds("report-issue").unwrap();
        assert!(report.size.width >= px(33.));
        assert!(report.left() >= status.left());
        assert!(report.right() <= status.right());
        assert!(report.top() >= status.top());
        assert!(report.bottom() <= status.bottom());
        let version = cx
            .debug_bounds("status-version")
            .context("status version bounds")?;
        assert!(version.size.width > px(0.));
        assert!(version.left() >= report.right());
        assert!(version.right() <= status.right());
        assert!(version.top() >= status.top());
        assert!(version.bottom() <= status.bottom());
        let theme = cx.debug_bounds("status-theme").unwrap();
        let keybinds = cx.debug_bounds("status-keybinds").unwrap();
        assert!(theme.left() >= status.left());
        assert!(theme.right() <= keybinds.left());
        assert!(keybinds.right() <= report.left());
        for button in [theme, keybinds] {
            assert!(button.size.width > px(0.));
            assert!(button.top() >= status.top());
            assert!(button.bottom() <= status.bottom());
        }
        cx.simulate_click(report.center(), Default::default());
        assert_eq!(
            cx.opened_url().as_deref(),
            Some(
                format!(
                    "https://github.com/penso/herdr-gpui/issues/new?template=bug_report.yml&version={}",
                    crate::APP_VERSION.replace('+', "%2B"),
                )
                .as_str()
            )
        );
    }
    cx.update(|_, cx| cx.default_global::<PaintedProbes>().check())
}

#[gpui::test]
fn the_sidebar_follows_the_selection_without_undoing_manual_scrolling(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    // The window paints while the connection is still awaiting its first snapshot.
    let mut snapshot = cx
        .update(|_, cx| view.update(cx, |view, _| view.live.snapshot.take()))
        .unwrap();
    // Reserve the new footer while retaining this test's original list viewport.
    cx.simulate_resize(size(px(800.), px(640.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    // The fixture's grouped worktrees stay contiguous, so w30 is the 31st row.
    const ROW: usize = 30;
    {
        let snapshot = Arc::make_mut(&mut snapshot);
        snapshot.focused_workspace_id = Some("w30".into());
        for workspace in &mut snapshot.workspaces {
            workspace.focused = workspace.workspace_id == "w30";
        }
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.live.snapshot = Some(snapshot);
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        let spaces = &view.sidebar_scroll[0];
        let offset = spaces.offset().y;
        let row = spaces.bounds_for_item(ROW).unwrap();
        assert!(offset < px(0.), "focused workspace must scroll into view");
        assert!(row.top() + offset >= spaces.bounds().top(), "{row:?}");
        assert!(row.bottom() + offset <= spaces.bounds().bottom(), "{row:?}");
        // The fixture focuses no agent, so that list must stay where it was.
        assert_eq!(view.sidebar_scroll[1].offset().y, px(0.));
    });
    // While the selection holds, later frames must not fight manual scrolling.
    cx.update(|window, cx| {
        view.read(cx).sidebar_scroll[0].set_offset(point(px(0.), px(0.)));
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    cx.update(|_, cx| {
        assert_eq!(view.read(cx).sidebar_scroll[0].offset().y, px(0.));
    });
    // A new selection is revealed in turn, from wherever the list now sits.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_workspace_id = Some("w20".into());
            for workspace in &mut snapshot.workspaces {
                workspace.focused = workspace.workspace_id == "w20";
            }
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        let spaces = &view.sidebar_scroll[0];
        let offset = spaces.offset().y;
        let row = spaces.bounds_for_item(20).unwrap();
        assert!(offset < px(0.), "a new selection must scroll into view");
        assert!(row.top() + offset >= spaces.bounds().top(), "{row:?}");
        assert!(row.bottom() + offset <= spaces.bounds().bottom(), "{row:?}");
    });

    // Selecting a visible neighbor must not move the list. A selection above or
    // below the viewport should land at the nearest edge, not always the bottom.
    for (id, row, edge) in [
        ("w19", 19, None),
        ("w0", 0, Some(false)),
        ("w4", 4, None),
        ("w5", 5, None),
        ("w30", 30, Some(true)),
        ("w4", 4, Some(false)),
        ("w5", 5, None),
    ] {
        let before = cx.update(|_, cx| view.read(cx).sidebar_scroll[0].offset());
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.focused_workspace_id = Some(id.into());
                for workspace in &mut snapshot.workspaces {
                    workspace.focused = workspace.workspace_id == id;
                }
                cx.notify();
            });
        });
        cx.update(|window, cx| {
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        cx.update(|_, cx| {
            let spaces = &view.read(cx).sidebar_scroll[0];
            let bounds = spaces.bounds_for_item(row).unwrap();
            match edge {
                None => assert_eq!(spaces.offset(), before, "visible {id} must not scroll"),
                Some(false) => assert_eq!(bounds.top() + spaces.offset().y, spaces.bounds().top()),
                Some(true) => assert_eq!(
                    bounds.bottom() + spaces.offset().y,
                    spaces.bounds().bottom()
                ),
            }
        });
    }
}

#[gpui::test]
fn worktree_rows_wear_their_cached_pull_request(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    // Rows w3..w5 are the fixture's worktree group; w4 is a linked checkout.
    let bare = cx.debug_bounds("name-sidebar-child").unwrap();
    assert!(cx.debug_bounds("pr-sidebar-child").is_none());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            // A standalone checkout too, to compare with a collapsible group row.
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let solo_key = if cfg!(windows) {
                "C:/fixture/solo/.git"
            } else {
                "/fixture/solo/.git"
            };
            snapshot.workspaces[0].worktree = Some(ClientShellWorktree {
                key: solo_key.into(),
                label: "solo".into(),
                is_linked_worktree: false,
            });
            let now = std::time::Instant::now();
            for (key, branch, number, state, additions, deletions) in [
                (REPO_KEY, "worktree/sidebar-child", 7, "MERGED", 23, 342),
                (solo_key, "main", 9, "OPEN", 4, 5),
                // The group's own head, so a row carries arrow and badge both.
                (REPO_KEY, "develop", 11, "OPEN", 1, 2),
            ] {
                let mut value = crate::pull_request::fixture().unwrap();
                value.number = number;
                value.state = crate::pull_request::State::from(state.to_owned());
                value.additions = additions;
                value.deletions = deletions;
                view.menu.pr_cache.seed(
                    crate::pull_request::Input {
                        checkout: None,
                        repo_key: key.into(),
                        branch: branch.into(),
                    },
                    value,
                    now,
                );
            }
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    let badge = cx.debug_bounds("pr-sidebar-child").unwrap();
    let row = cx.debug_bounds("row-sidebar-child").unwrap();
    let name = cx.debug_bounds("name-sidebar-child").unwrap();
    // The badge takes its column from the label, inside the row.
    assert!(badge.right() <= row.right());
    assert!(name.right() <= badge.left());
    assert!(name.size.width < bare.size.width);
    // The tree gutter sits under the parent's label and stops at the child's
    // own dot: lines never reach the text on either side.
    let gutter = cx.debug_bounds("tree-sidebar-child").unwrap();
    let parent_column = cx.debug_bounds("column-agent-launcher").unwrap();
    let child_column = cx.debug_bounds("column-sidebar-child").unwrap();
    assert_eq!(gutter.left(), parent_column.left());
    assert!(gutter.right() <= child_column.left() - px(super::STATUS_WIDTH));
    // Badges hug the row's inner edge, whether or not the row can collapse and
    // whether or not an arrow is drawn in front of them.
    let solo = cx.debug_bounds("pr-herdr").unwrap();
    let head = cx.debug_bounds("pr-agent-launcher").unwrap();
    let arrow = cx.debug_bounds("collapse-3").unwrap();
    for right in [solo.right(), head.right(), badge.right()] {
        assert_eq!(right, row.right() - px(12.), "badges must be flush right");
    }
    assert!(arrow.right() <= head.left(), "{arrow:?} {head:?}");
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        for text in ["#7", "+23", "-342", "#9", "+4", "-5"] {
            assert!(
                probes.contains_key(text),
                "missing {text}: {:?}",
                probes.keys()
            );
        }
    });
}

#[gpui::test]
fn worktree_rows_mark_uncommitted_work(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let clean_label = cx.debug_bounds("name-sidebar-child").unwrap();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let now = std::time::Instant::now();
            let input = |branch: &str| crate::pull_request::Input {
                checkout: None,
                repo_key: REPO_KEY.into(),
                branch: branch.into(),
            };
            // A checkout with a pull request and uncommitted work, and one that
            // only has uncommitted work.
            view.menu.pr_cache.seed(
                input("worktree/sidebar-child"),
                crate::pull_request::fixture().unwrap(),
                now,
            );
            view.git
                .seed_probe(input("worktree/sidebar-child"), true, now);
            view.git.seed_probe(input("develop"), true, now);
            // Answered and clean: no mark, and no column reserved for one.
            view.git.seed_probe(
                input("worktree/sidebar-child-with-a-long-readable-branch-name"),
                false,
                now,
            );
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    let row = cx.debug_bounds("row-sidebar-child").unwrap();
    let badge = cx.debug_bounds("pr-sidebar-child").unwrap();
    let dot = cx.debug_bounds("dirty-sidebar-child").unwrap();
    assert_eq!(dot.size.width, px(12.));
    assert_eq!(dot.size.height, px(12.));
    // The mark leads the badge column, still flush against the row's edge.
    assert!(badge.left() <= dot.left() && dot.right() <= badge.right());
    assert_eq!(badge.right(), row.right() - px(12.));
    assert!(cx.debug_bounds("name-sidebar-child").unwrap().right() <= badge.left());
    assert!(clean_label.size.width > cx.debug_bounds("name-sidebar-child").unwrap().size.width);
    // A dirty checkout without a pull request still earns the column.
    let head = cx.debug_bounds("pr-agent-launcher").unwrap();
    let head_dot = cx.debug_bounds("dirty-agent-launcher").unwrap();
    assert_eq!(
        head.right(),
        cx.debug_bounds("row-agent-launcher").unwrap().right() - px(12.)
    );
    assert!(head.left() <= head_dot.left() && head_dot.right() <= head.right());
    assert!(
        cx.debug_bounds("dirty-sidebar-child-with-a-long-readable-branch-name")
            .is_none(),
        "a clean checkout is not marked"
    );
}

#[gpui::test]
fn workspace_right_click_survives_redraw_release_and_pointer_movement(
    cx: &mut gpui::TestAppContext,
) {
    use gpui::MouseButton;

    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    for (redraw, release) in [
        (true, MouseButton::Right),
        (false, MouseButton::Right),
        // macOS can deliver Left when Control is released before the mouse.
        (true, MouseButton::Left),
    ] {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let position = cx.debug_bounds("row-agent-launcher").unwrap().center();
        cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
        cx.update(|window, cx| {
            if redraw {
                full_draw(window, cx).clear(cx);
            }
            assert_eq!(view.read(cx).menu.page, Some(crate::menu::Page::Workspace));
        });
        // A second press before release must not dismiss the menu just opened.
        cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).menu.page, Some(crate::menu::Page::Workspace));
        });
        cx.simulate_mouse_up(position, release, Modifiers::default());
        cx.simulate_mouse_move(point(px(700.), px(500.)), None, Modifiers::default());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.update_workspace_dialog(window, cx);
                view.poll_hover_menu(std::time::Instant::now(), window, cx);
                view.poll_tab_rename(window, cx);
                view.poll_pane_rename(window, cx);
            });
            full_draw(window, cx).clear(cx);
            assert_eq!(view.read(cx).menu.page, Some(crate::menu::Page::Workspace));
            assert!(view.read(cx).menu.focus.is_focused(window));
            assert!(!view.read(cx).menu.opening_right_click);
        });
        cx.simulate_mouse_down(
            point(px(700.), px(500.)),
            MouseButton::Right,
            Modifiers::default(),
        );
        cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));
        cx.simulate_mouse_up(
            point(px(700.), px(500.)),
            MouseButton::Right,
            Modifiers::default(),
        );
    }
}

#[gpui::test]
fn workspace_popover_header_and_right_click_retargeting(cx: &mut gpui::TestAppContext) {
    use crate::menu::{Page, workspace_tests::target_id};
    use gpui::MouseButton;

    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    for (index, (selector, id, label, branch)) in [
        ("row-agent-launcher", "w3", "agent-launcher", "develop"),
        ("row-herdr", "w0", "herdr", "main"),
        (
            "row-sidebar-child",
            "w4",
            "agent-launcher",
            "worktree/sidebar-child",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let row = cx.debug_bounds(selector).unwrap();
        let position = point(px(200. - index as f32 * 80.), row.center().y);
        if let Some(panel) = cx.debug_bounds("menu-panel") {
            assert!(
                !panel.contains(&position),
                "{selector}: {panel:?} {position:?}"
            );
        }
        cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
            assert_eq!(view.read(cx).menu.page, Some(Page::Workspace));
            assert_eq!(target_id(view.read(cx)), Some(id));
            assert_eq!(
                view.read(cx).pending_navigation,
                Some(crate::NavigationTarget::Workspace(id.to_owned()))
            );
            assert!(view.read(cx).menu.focus.is_focused(window));
        });
        cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::default());
        let header = cx.debug_bounds("workspace-menu-header").unwrap();
        let name = cx.debug_bounds("workspace-menu-name").unwrap();
        let detail = cx.debug_bounds("workspace-menu-branch").unwrap();
        assert!(header.bottom() <= cx.debug_bounds("workspace-menu-Rename").unwrap().top());
        cx.update(|_, cx| {
            let probes = &cx.global::<TextProbes>().0;
            assert!(name.contains(&probes[label].0.center()));
            assert!(detail.contains(&probes[branch].0.center()));
        });
    }
    // The panel itself must not retarget to the row underneath it.
    let header = cx.debug_bounds("workspace-menu-header").unwrap();
    cx.simulate_mouse_down(header.center(), MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(header.center(), MouseButton::Right, Modifiers::default());
    cx.update(|_, cx| assert_eq!(target_id(view.read(cx)), Some("w4")));
    // Left clicks still dismiss instead of navigating or reopening.
    let row = cx.debug_bounds("row-herdr").unwrap();
    cx.simulate_click(point(px(5.), row.center().y), Modifiers::default());
    cx.update(|_, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert_eq!(
            view.read(cx).pending_navigation,
            Some(crate::NavigationTarget::Workspace("w4".into()))
        );
    });
    // Long labels and branch names stay inside a narrow popup.
    cx.simulate_resize(size(px(320.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_workspace_menu("w1", point(px(20.), px(100.)), window, cx)
        });
        full_draw(window, cx).clear(cx);
    });
    let panel = cx.debug_bounds("menu-panel").unwrap();
    for selector in ["workspace-menu-name", "workspace-menu-branch"] {
        let bounds = cx.debug_bounds(selector).unwrap();
        assert!(bounds.left() >= panel.left() && bounds.right() <= panel.right());
    }
    // Once an action opens a dialog, outside right-clicks only dismiss it.
    cx.simulate_keystrokes("down enter");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-herdr").unwrap();
    let position = point(px(5.), row.center().y);
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::default());
    cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));
    // Non-Git workspaces have a name-only header, not an empty second line.
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        view.update(cx, |view, cx| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).workspaces[0].branch = None;
            view.open_workspace_menu("w0", point(px(20.), px(100.)), window, cx);
        });
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    let header = cx.debug_bounds("workspace-menu-header").unwrap();
    let name = cx.debug_bounds("workspace-menu-name").unwrap();
    assert!(header.size.height < name.size.height * 2.);
    cx.update(|_, cx| {
        assert!(!cx.global::<TextProbes>().0.contains_key("main"));
        assert_eq!(target_id(view.read(cx)), Some("w0"));
    });
}

#[gpui::test]
fn the_workspace_menu_folds_and_unfolds_a_worktree_group(cx: &mut gpui::TestAppContext) {
    use gpui::{Modifiers, MouseButton};
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.live.status = crate::state::ConnectionStatus::Connected;
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    for (item, icon, collapsed) in [
        (
            "workspace-menu-Collapse group",
            "workspace-menu-icon-Collapse group",
            true,
        ),
        (
            "workspace-menu-Expand group",
            "workspace-menu-icon-Expand group",
            false,
        ),
    ] {
        let parent = cx.debug_bounds("row-agent-launcher").unwrap();
        cx.simulate_mouse_down(parent.center(), MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(parent.center(), MouseButton::Right, Modifiers::default());
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
            assert!(view.read(cx).menu.page == Some(crate::menu::Page::Workspace));
        });
        let row = cx
            .debug_bounds(item)
            .unwrap_or_else(|| panic!("missing {item}"));
        assert!(cx.debug_bounds(icon).is_some(), "missing {icon}");
        cx.simulate_click(row.center(), Modifiers::default());
        cx.update(|window, cx| {
            cx.default_global::<TextProbes>().0.clear();
            window.refresh();
            full_draw(window, cx).clear(cx);
            let view = view.read(cx);
            // Folding is the client's own view of the list, not a daemon request.
            assert!(view.menu.page.is_none(), "{item} left the menu open");
            assert_eq!(view.collapsed_repos.contains(REPO_KEY), collapsed);
            assert_eq!(
                !cx.global::<TextProbes>().0.contains_key("sidebar-child"),
                collapsed,
                "{item} did not change the visible children"
            );
        });
    }
}

#[test]
fn child_gutter_lines_land_on_whole_device_pixels() {
    use crate::sidebar::row::{RowTree, tree_lines};
    use gpui::{Bounds, point, size};
    let font = crate::config::FontConfig {
        family: "Menlo".into(),
        size: 12.,
        fallbacks: None,
    };
    for scale in [1., 2., 3.] {
        let row = Bounds::new(point(px(0.), px(244.)), size(px(231.), px(40.)));
        let device = |value: Pixels| f32::from(value) * scale;
        let whole = |value: Pixels| (device(value) - device(value).round()).abs() < 0.001;
        for tree in [RowTree::Child, RowTree::LastChild] {
            let [trunk, tick] = tree_lines(row, tree, &font, 4., scale);
            // Both lines carry the same weight and start on the device grid, so
            // neither is drawn thinner or blurrier than the other.
            assert!(
                (trunk.size.width - tick.size.height).abs() < px(0.01),
                "{scale}"
            );
            assert!(
                (device(trunk.size.width) - scale.round().max(1.)).abs() < 0.01,
                "{scale}"
            );
            for edge in [trunk.left(), trunk.top(), tick.left(), tick.top()] {
                assert!(whole(edge), "{scale}: {edge:?}");
            }
            // The trunk hugs the gutter's leading edge, the tick crosses to the
            // dot at its far edge; neither strays into the label beyond.
            assert_eq!(trunk.left(), tick.left(), "{scale}");
            assert_eq!(trunk.left(), row.left(), "{scale}");
            assert_eq!(tick.right(), row.right(), "{scale}");
            // The tick meets the status dot's middle row.
            let middle = row.top() + px(4. + super::line_height(&font) / 2.);
            assert!(
                (tick.center().y - middle).abs() <= px(1. / scale),
                "{scale}"
            );
            // Only a row with a sibling below carries the trunk to the bottom.
            match tree {
                RowTree::Child => assert_eq!(trunk.bottom(), row.bottom(), "{scale}"),
                _ => assert_eq!(trunk.bottom(), tick.bottom(), "{scale}"),
            }
            assert_eq!(trunk.top(), row.top(), "{scale}");
        }
    }
}

#[gpui::test]
fn the_sidebar_menu_stays_clear_of_the_window_chrome(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let chrome = px(crate::titlebar::HEIGHT
        + crate::worktree_banner::reserved(env!("HERDR_BUILD_WORKTREE") == "1"));
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    // An anchor near the top leaves no room above it, one near the footer
    // plenty; either way the panel stays between the chrome and the bottom.
    for anchor in [chrome + px(100.), px(560.)] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.menu.anchor = point(px(120.), anchor);
                view.open_menu(window, cx);
            });
            full_draw(window, cx).clear(cx);
            assert!(view.read(cx).menu.page == Some(crate::menu::Page::Menu));
        });
        let panel = cx.debug_bounds("menu-panel").unwrap();
        // A margin from the chrome and the bottom edge, so a clamped list is
        // visibly a list that scrolls rather than one cut off by the frame.
        assert!(
            panel.top() >= chrome + px(8.),
            "anchor {anchor:?}: {panel:?}"
        );
        assert!(panel.bottom() <= px(592.), "anchor {anchor:?}: {panel:?}");
        // Whatever the room, the list keeps enough height to scroll through.
        assert!(panel.size.height >= px(60.), "anchor {anchor:?}: {panel:?}");
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    }
}

#[gpui::test]
fn device_footer_filters_both_lists_and_opens_settings(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:fixture".into(),
                "A very long remote device label that must fit".into(),
                ConnectTarget::Ssh {
                    target: "example.invalid".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = view.live.snapshot.clone();
            view.endpoints.push(remote);
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("host-ssh:fixture").is_some());
    let all_counts = cx.update(|_, cx| {
        view.read(cx)
            .sidebar_scroll
            .each_ref()
            .map(|scroll| scroll.children_count())
    });
    let footer = cx.debug_bounds("device-footer").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(footer.bottom(), sidebar.bottom());
    let status = cx.debug_bounds("connection-status").unwrap();
    assert_eq!(sidebar.bottom(), px(600.));
    assert_eq!(status.bottom(), sidebar.bottom());
    assert_eq!(status.left(), sidebar.right());
    assert_eq!(footer.size.height, px(40.));
    assert!(cx.debug_bounds("agents-scroll").unwrap().bottom() <= footer.top());

    let picker_bounds = cx.debug_bounds("device-picker").unwrap();
    let picker = picker_bounds.center();
    for position in [
        picker_bounds.origin + point(px(2.), px(2.)),
        point(
            picker_bounds.right() - px(2.),
            picker_bounds.bottom() - px(2.),
        ),
    ] {
        cx.simulate_click(position, Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let menu = cx.debug_bounds("menu-panel").unwrap();
        assert_eq!(menu.size.width, px(280.));
        assert_eq!(menu.left(), picker_bounds.left());
        assert_eq!(picker_bounds.top() - menu.bottom(), px(12.));
        for (row, marker) in [
            ("device-row-0", "device-check-0"),
            ("device-row-1", "device-dot-1"),
        ] {
            let row = cx.debug_bounds(row).unwrap();
            let marker = cx.debug_bounds(marker).unwrap();
            assert!((row.center().y - marker.center().y).abs() <= px(0.5));
        }
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    }
    cx.simulate_click(picker, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("menu-panel").unwrap().bottom() <= footer.bottom());
    // All Devices -> Local. An explicit-socket fixture must not touch the catalog.
    cx.simulate_keystrokes("down enter");
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).device_filter.as_deref(), Some("local"));
        assert!(!view.read(cx).device_visible("ssh:fixture"));
        assert!(view.read(cx).focus.is_focused(window));
        full_draw(window, cx).clear(cx);
    });
    cx.update(|_, cx| {
        let local_counts = view
            .read(cx)
            .sidebar_scroll
            .each_ref()
            .map(|scroll| scroll.children_count());
        assert_eq!(local_counts.map(|count| count * 2), all_counts);
    });
    cx.simulate_click(picker, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        assert!(view.read(cx).device_filter.is_none());
        assert_eq!(view.read(cx).selected_endpoint, 0);
        full_draw(window, cx).clear(cx);
    });
    assert!(cx.debug_bounds("host-ssh:fixture").is_some());
    let settings = cx.debug_bounds("device-settings").unwrap().center();
    cx.simulate_click(settings, Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).menu.page,
            Some(crate::menu::Page::Preferences)
        )
    });
    cx.simulate_keystrokes("escape");
    // A narrow sidebar retains both controls without spilling into the terminal.
    cx.update(|window, cx| {
        view.update(cx, |view, _| view.sidebar_width = Some(140.));
        full_draw(window, cx).clear(cx);
    });
    let footer = cx.debug_bounds("device-footer").unwrap();
    assert!(cx.debug_bounds("device-settings").unwrap().right() <= footer.right());
    assert!(
        cx.debug_bounds("device-picker").unwrap().right()
            < cx.debug_bounds("device-settings").unwrap().left()
    );
}

#[gpui::test]
fn healthy_connection_status_is_quiet_but_diagnostics_remain(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("connection-message").is_none());
    assert!(cx.debug_bounds("status-theme").is_some());
    for status in [
        crate::state::ConnectionStatus::Connected,
        crate::state::ConnectionStatus::Disconnected,
    ] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.live.status = status;
                view.local_error = Some("Local operation failed".into());
                view.live.error = Some("Connection interrupted".into());
                cx.notify();
            });
            full_draw(window, cx).clear(cx);
        });
        assert!(cx.debug_bounds("connection-message").unwrap().size.width > px(0.));
    }
}

#[gpui::test]
fn add_device_form_keeps_input_local_and_validates_before_launch(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let picker = cx.debug_bounds("device-picker").unwrap().center();
    cx.simulate_click(picker, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.simulate_keystrokes("down down enter");
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(crate::menu::Page::Devices)));
    cx.simulate_keystrokes("escape");
    if cfg!(windows) {
        return;
    }
    // Enable the form without enabling the fixture's isolated catalog worker.
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.endpoints[0].connection.target = ConnectTarget::Local
        });
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_click(picker, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx.simulate_keystrokes("down down enter");
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).menu.page, Some(crate::menu::Page::AddDevice));
        full_draw(window, cx).clear(cx);
    });
    for (width, height) in [(320., 300.), (800., 600.)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let panel = cx.debug_bounds("menu-panel").unwrap();
        let header = cx.debug_bounds("device-setup-header").unwrap();
        let close = cx.debug_bounds("device-setup-close").unwrap();
        let body = cx.debug_bounds("device-setup-body").unwrap();
        let footer = cx.debug_bounds("device-setup-footer").unwrap();
        let submit = cx.debug_bounds("device-setup-submit").unwrap();
        assert!(close.top() >= header.top() && close.bottom() <= header.bottom());
        assert!(close.center().x > panel.center().x);
        assert!((body.top() - header.bottom()).abs() <= px(1.));
        assert!((body.bottom() - footer.top()).abs() <= px(1.));
        assert!(submit.top() >= footer.top() && submit.bottom() <= footer.bottom());
        assert!(footer.bottom() <= panel.bottom());
        assert!(panel.bottom() <= px(height));
    }
    cx.simulate_input("-invalid-host");
    cx.simulate_keystrokes("tab");
    cx.simulate_input("Test device");
    cx.simulate_keystrokes("tab enter");
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(crate::menu::Page::AddDevice)));
    // Native shortcuts cannot escape a device form into the underlying terminal.
    cx.simulate_keystrokes("cmd-b");
    cx.update(|_, cx| assert!(view.read(cx).sidebar_visible));
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });
}

#[gpui::test]
fn the_agents_header_toggles_between_grouped_and_priority(cx: &mut gpui::TestAppContext) {
    use crate::preferences::AgentSort;
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    // The second agent wants attention; only priority floats it to the top.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.agents[0].state_change_seq = 9;
            snapshot.agents[1].agent_status = AgentStatus::Blocked;
            snapshot.agents[1].state_change_seq = 1;
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let (first, second) = ("row-agent-p0", "row-agent-p1");
    let sort = cx.debug_bounds("agents-sort").unwrap();
    let header = cx.debug_bounds("sidebar").unwrap();
    // The label ends at the sidebar's inner edge, opposite the "agents" title.
    assert_eq!(sort.right(), header.right() - px(13.));
    for (expected, top) in [(AgentSort::Grouped, first), (AgentSort::Priority, second)] {
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).agent_sort, expected);
            let probes = &cx.global::<TextProbes>().0;
            assert!(
                probes.contains_key(expected.to_string().as_str()),
                "{:?}",
                probes.keys()
            );
        });
        let (a, b) = (
            cx.debug_bounds(first).unwrap(),
            cx.debug_bounds(second).unwrap(),
        );
        let ordered = if top == first {
            a.top() < b.top()
        } else {
            b.top() < a.top()
        };
        assert!(ordered, "{expected:?}: {a:?} {b:?}");
        cx.simulate_click(sort.center(), Default::default());
        cx.update(|window, cx| {
            cx.default_global::<TextProbes>().0.clear();
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
    }
    // Toggling twice returns to the stored default without a daemon request.
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(view.agent_sort, AgentSort::Grouped);
        assert!(view.agent_sort_modified);
    });
}

/// Resting the pointer on a workspace opens the menu its right click opens,
/// once, and only after the pointer has both moved and settled. The behavior
/// is opt-in, so the test turns its feature flag on.
#[gpui::test]
fn resting_on_a_workspace_opens_its_menu_once(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.active = true;
        view.config.features.sidebar_hover_menu = true;
        view
    });
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-herdr").unwrap().center();
    let settle = |view: &Entity<HerdrWindow>,
                  cx: &mut gpui::VisualTestContext,
                  elapsed: std::time::Duration| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.poll_hover_menu(std::time::Instant::now() + elapsed, window, cx);
            });
            full_draw(window, cx).clear(cx);
        });
    };

    // Entering the row alone is not a rest: the pointer has not moved yet.
    cx.simulate_mouse_move(row, None, Modifiers::default());
    assert!(
        view.read_with(cx, |view, _| view.hover.is_some()),
        "row armed"
    );
    settle(&view, cx, super::HOVER_MENU_DELAY);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

    // Drifting inside the row restarts the dwell rather than opening early.
    cx.simulate_mouse_move(row + point(px(8.), px(0.)), None, Modifiers::default());
    settle(&view, cx, std::time::Duration::ZERO);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    settle(&view, cx, super::HOVER_MENU_DELAY / 2);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

    settle(&view, cx, super::HOVER_MENU_DELAY);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
        assert_eq!(crate::menu::workspace_tests::target_id(view), Some("w0"));
        // The same one-shot intent, spent: nothing is left armed behind it.
        assert!(view.hover.is_none());
    });

    // Moving inside the popup keeps it: it is the menu the pointer asked for.
    let panel = cx.debug_bounds("menu-panel").unwrap();
    cx.simulate_mouse_move(panel.center(), None, Modifiers::default());
    settle(&view, cx, super::HOVER_MENU_DELAY);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
        assert!(view.hover_menu.as_ref().is_some_and(|open| open.inside));
    });

    // Leaving it closes it, with no click anywhere.
    cx.simulate_mouse_move(
        point(panel.right() + px(40.), panel.bottom() + px(40.)),
        None,
        Modifiers::default(),
    );
    settle(&view, cx, std::time::Duration::ZERO);
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(view.hover_menu.is_none());
    });

    // Leaving a menu for a row above it keeps that row's dwell, so the pointer
    // can walk up the list from one menu to the next without a click.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_workspace_menu("w0", point(px(20.), px(20.)), window, cx);
            view.hover_menu = Some(super::HoverMenu {
                position: window.mouse_position() + point(px(60.), px(60.)),
                inside: false,
            });
            view.hover_workspace("w1", true, window);
            view.poll_hover_menu(std::time::Instant::now(), window, cx);
            assert!(view.menu.page.is_none());
            assert!(view.hover.is_some(), "the next row keeps its dwell");
            assert!(view.hover_menu.is_none());
        });
        full_draw(window, cx).clear(cx);
    });

    // A menu opened any other way is not the pointer's to close.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_workspace_menu("w0", point(px(20.), px(20.)), window, cx);
        });
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_mouse_move(point(px(700.), px(600.)), None, Modifiers::default());
    settle(&view, cx, super::HOVER_MENU_DELAY);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
        assert!(view.hover_menu.is_none());
    });
    cx.simulate_keystrokes("escape");

    // Dismissing must not let a still pointer reopen the menu.
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    for _ in 0..3 {
        settle(&view, cx, super::HOVER_MENU_DELAY);
        assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    }

    // Scrolling slides another row under a still pointer, so the row it entered
    // can no longer speak for what it covers. GPUI may report the newly covered
    // row as hovered, but that fresh arm still waits for the pointer to move.
    let away = point(px(700.), px(400.));
    cx.simulate_mouse_move(away, None, Modifiers::default());
    cx.simulate_mouse_move(row, None, Modifiers::default());
    cx.simulate_mouse_move(row + point(px(4.), px(4.)), None, Modifiers::default());
    let armed = view.read_with(cx, |view, _| {
        view.hover.as_ref().map(|hover| hover.workspace.clone())
    });
    assert!(armed.is_some(), "row armed");
    cx.update(|_, cx| {
        view.read(cx).sidebar_scroll[0].set_offset(point(px(0.), px(-40.)));
    });
    settle(&view, cx, super::HOVER_MENU_DELAY);
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(
            view.hover
                .as_ref()
                .is_none_or(|hover| Some(&hover.workspace) != armed.as_ref() && !hover.moved)
        );
    });

    // An inactive window keeps its menus closed under the same pointer.
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.active = false);
        view.read(cx).sidebar_scroll[0].set_offset(point(px(0.), px(0.)));
    });
    cx.simulate_mouse_move(away, None, Modifiers::default());
    cx.simulate_mouse_move(row, None, Modifiers::default());
    cx.simulate_mouse_move(row + point(px(0.), px(6.)), None, Modifiers::default());
    assert!(
        view.read_with(cx, |view, _| view.hover.is_some()),
        "row armed"
    );
    settle(&view, cx, super::HOVER_MENU_DELAY);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}

/// Preferences lists every feature flag, in both states: the config file is
/// the only place a flag is turned on.
#[gpui::test]
fn preferences_list_feature_flags(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    cx.simulate_resize(size(px(900.), px(1200.)));
    for enabled in [false, true] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.config.features.sidebar_hover_menu = enabled;
                view.open_preferences(window, cx);
            });
            full_draw(window, cx).clear(cx);
        });
        let body = cx.debug_bounds("preferences-body").unwrap();
        for (id, label, _) in crate::preferences::feature_rows(&Default::default()) {
            let bounds = cx.debug_bounds(id).unwrap_or_else(|| panic!("{label} row"));
            // Other settings can place feature flags below the initial viewport.
            cx.update(|window, cx| {
                let scroll = &view.read(cx).menu.preferences_scroll;
                scroll.set_offset(
                    scroll.offset() + point(px(0.), body.center().y - bounds.center().y),
                );
                window.refresh();
                full_draw(window, cx).clear(cx);
            });
            let bounds = cx.debug_bounds(id).unwrap_or_else(|| panic!("{label} row"));
            assert!(
                body.contains(&bounds.center()),
                "{label} row outside the body"
            );
        }
    }
}

/// Without its feature flag, a resting pointer arms nothing and opens nothing:
/// only a right click still opens a space's menu.
#[gpui::test]
fn resting_on_a_workspace_opens_nothing_by_default(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.active = true;
        view
    });
    assert!(!crate::config::Config::default().features.sidebar_hover_menu);
    cx.simulate_resize(size(px(900.), px(700.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let row = cx.debug_bounds("row-herdr").unwrap().center();

    cx.simulate_mouse_move(row, None, Modifiers::default());
    cx.simulate_mouse_move(row + point(px(6.), px(0.)), None, Modifiers::default());
    assert!(
        view.read_with(cx, |view, _| view.hover.is_none()),
        "row armed"
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // A stale rest from before the flag was turned off still expires.
            view.hover_workspace("w0", true, window);
            view.poll_hover_menu(
                std::time::Instant::now() + super::HOVER_MENU_DELAY * 2,
                window,
                cx,
            );
            assert!(view.menu.page.is_none());
            assert!(view.hover.is_none());
            assert!(view.hover_menu.is_none());
        });
        full_draw(window, cx).clear(cx);
    });
}

/// Draw one preview state in a freshly opened panel and return its bounds.
#[cfg(test)]
fn draw_update_state(
    cx: &mut gpui::VisualTestContext,
    view: &Entity<HerdrWindow>,
    state: &crate::updater::State,
) -> (Bounds<Pixels>, Option<Bounds<Pixels>>) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_app_update(false, window, cx);
            view.update_preview = Some(state.clone());
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
    let panel = cx.debug_bounds("app-update-panel").unwrap();
    (panel, cx.debug_bounds("app-update-action"))
}

// `debug_bounds` keeps the last frame that drew an element, so a state that
// must show no button is only provable before any button has been drawn.
#[gpui::test]
fn a_homebrew_upgrade_in_progress_offers_nothing_to_interrupt(cx: &mut gpui::TestAppContext) {
    use crate::updater::State;
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    // Homebrew output is arbitrary length; a long line must not burst the panel.
    let states = [
        State::Upgrading {
            detail: "==> Downloading ".to_owned() + &"herdr".repeat(24),
        },
        State::Restarting,
    ];
    for (width, height) in [(320., 360.), (800., 600.)] {
        cx.simulate_resize(size(px(width), px(height)));
        for state in &states {
            let (panel, action) = draw_update_state(cx, &view, state);
            assert!(action.is_none(), "{state:?} cannot be interrupted");
            let progress = cx.debug_bounds("app-update-progress").unwrap();
            let fill = cx.debug_bounds("app-update-progress-fill").unwrap();
            assert_eq!(progress.size.height, px(6.));
            assert!((fill.size.width - progress.size.width * 0.3).abs() < px(1.));
            assert!(progress.left() >= panel.left() && progress.right() <= panel.right());
            assert!(
                panel.left() >= px(0.) && panel.right() <= px(width),
                "{state:?}: {panel:?}"
            );
            assert!(
                panel.top() >= px(0.) && panel.bottom() <= px(height),
                "{state:?}: {panel:?}"
            );
            cx.update(|window, cx| {
                view.update(cx, |view, cx| view.dismiss_menu(window, cx));
                full_draw(window, cx).clear(cx);
            });
        }
    }
}

#[gpui::test]
fn qa_update_progress_actions_are_isolated_and_dismissible(cx: &mut gpui::TestAppContext) {
    use crate::{
        actions::{ShowUpdateDownloadPreview, ShowUpdateHomebrewPreview},
        updater::State,
    };
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    let before = view.read_with(cx, |view, _| view.updater.state().clone());
    for homebrew in [false, true] {
        cx.update(|window, cx| {
            window.focus(&view.read(cx).focus.clone(), cx);
            full_draw(window, cx).clear(cx);
        });
        cx.update(|window, cx| {
            if homebrew {
                window.dispatch_action(Box::new(ShowUpdateHomebrewPreview), cx);
            } else {
                window.dispatch_action(Box::new(ShowUpdateDownloadPreview), cx);
            }
        });
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
            let view = view.read(cx);
            assert_eq!(view.menu.page, Some(crate::menu::Page::AppUpdate));
            assert_eq!(view.updater.state(), &before);
            if homebrew {
                assert!(matches!(view.update_preview, Some(State::Upgrading { .. })));
            } else {
                assert_eq!(
                    view.update_preview,
                    Some(State::Downloading {
                        received: 50_000_000,
                        total: 100_000_000
                    })
                );
            }
        });
        let bar = cx.debug_bounds("app-update-progress").unwrap();
        let fill = cx.debug_bounds("app-update-progress-fill").unwrap();
        assert!(
            (fill.size.width - bar.size.width * if homebrew { 0.3 } else { 0.5 }).abs() < px(1.)
        );
        if homebrew {
            let close = cx.debug_bounds("app-update-close").unwrap();
            cx.simulate_click(close.center(), Default::default());
        } else {
            // Even Cancel belongs to the preview, never to the real worker.
            let cancel = cx.debug_bounds("app-update-action").unwrap();
            cx.simulate_click(cancel.center(), Default::default());
            assert_eq!(
                view.read_with(cx, |view, _| view.update_preview.clone()),
                Some(State::Idle)
            );
            cx.simulate_keystrokes("escape");
        }
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert!(view.menu.page.is_none());
            assert!(view.update_preview.is_none());
            assert!(view.focus.is_focused(window));
            assert_eq!(view.updater.state(), &before);
        });
    }
}

#[gpui::test]
fn update_progress_tracks_downloads_and_keeps_verification_busy(cx: &mut gpui::TestAppContext) {
    use crate::updater::State;
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let states = [
        (
            State::Downloading {
                received: 0,
                total: 100,
            },
            0.,
        ),
        (
            State::Downloading {
                received: 25,
                total: 100,
            },
            0.25,
        ),
        (
            State::Downloading {
                received: 75,
                total: 100,
            },
            0.75,
        ),
        (
            State::Downloading {
                received: 100,
                total: 100,
            },
            0.3,
        ),
        (
            State::Downloading {
                received: u64::MAX,
                total: 100,
            },
            0.3,
        ),
        (
            State::Downloading {
                received: 25,
                total: 0,
            },
            0.3,
        ),
        (State::Checking, 0.3),
        (State::Installing, 0.3),
        (State::Cancelling, 0.3),
        (
            State::Ready {
                version: "9999.0.0".into(),
            },
            1.,
        ),
        (
            State::Restart {
                version: "9999.0.0".into(),
            },
            1.,
        ),
    ];
    for (width, height) in [(320., 360.), (800., 600.)] {
        cx.simulate_resize(size(px(width), px(height)));
        for (state, fraction) in &states {
            let (panel, _) = draw_update_state(cx, &view, state);
            let progress = cx.debug_bounds("app-update-progress").unwrap();
            let fill = cx.debug_bounds("app-update-progress-fill").unwrap();
            assert!(progress.size.width > px(0.));
            assert_eq!(progress.size.height, px(6.));
            assert!(
                (fill.size.width - progress.size.width * *fraction).abs() < px(1.),
                "{state:?}"
            );
            assert!(progress.left() >= panel.left() && progress.right() <= panel.right());
            assert!(panel.top() >= px(0.) && panel.bottom() <= px(height));
        }
    }
}

#[gpui::test]
fn the_homebrew_update_states_stay_inside_the_panel(cx: &mut gpui::TestAppContext) {
    use crate::updater::State;
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let disabled = cx.update(|_, cx| view.read(cx).updater.state().clone());
    let states = [
        State::Homebrew {
            version: "9999.0.0".into(),
        },
        State::Restart {
            version: "9999.0.0".into(),
        },
    ];
    for (width, height) in [(320., 360.), (320., 600.), (800., 600.)] {
        cx.simulate_resize(size(px(width), px(height)));
        for state in &states {
            let (panel, action) = draw_update_state(cx, &view, state);
            let action = action.unwrap();
            let footer = cx.debug_bounds("app-update-footer").unwrap();
            assert!(
                panel.left() >= px(0.) && panel.right() <= px(width),
                "{state:?}: {panel:?}"
            );
            assert!(
                panel.top() >= px(0.) && panel.bottom() <= px(height),
                "{state:?}: {panel:?}"
            );
            assert!(
                action.left() >= panel.left() && action.right() <= panel.right(),
                "{state:?}: {action:?}"
            );
            assert!(
                action.top() >= footer.top() && action.bottom() <= footer.bottom(),
                "{state:?}: {action:?}"
            );
            cx.simulate_click(action.center(), Default::default());
            // A preview click must never reach the real update service.
            cx.update(|_, cx| assert_eq!(view.read(cx).updater.state(), &disabled));
            cx.update(|window, cx| {
                view.update(cx, |view, cx| view.dismiss_menu(window, cx));
                full_draw(window, cx).clear(cx);
            });
        }
    }
}

#[gpui::test]
fn sidebar_split_drag_clamps_releases_outside_and_resets(cx: &mut gpui::TestAppContext) {
    use gpui::{MouseButton, MouseDownEvent};

    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let initial_spaces = cx.debug_bounds("spaces-section").unwrap();
    let initial_agents = cx.debug_bounds("agents-section").unwrap();
    let footer = cx.debug_bounds("device-footer").unwrap();
    assert!((initial_spaces.size.height - initial_agents.size.height).abs() <= px(1.));

    for (requested, expected) in [(0.7, 0.7), (0.3, 0.3), (-0.5, 0.1), (1.5, 0.9)] {
        let divider = cx.debug_bounds("sidebar-split-resize").unwrap();
        let available = sidebar.size.height - divider.size.height - footer.size.height;
        // Move and release outside the sidebar as well as outside the divider.
        let end = point(
            sidebar.right() + px(100.),
            sidebar.top() + divider.size.height / 2. + available * requested,
        );
        cx.simulate_mouse_down(divider.center(), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        view.read_with(cx, |view, _| {
            assert!(view.sidebar_drag.is_some());
            assert!((view.sidebar_split.unwrap() - expected).abs() < 0.0001);
            assert!(view.sidebar_split_modified);
            assert_eq!(view.sidebar_width, None);
        });
        let spaces = cx.debug_bounds("spaces-section").unwrap();
        let agents = cx.debug_bounds("agents-section").unwrap();
        let divider = cx.debug_bounds("sidebar-split-resize").unwrap();
        assert!((spaces.size.height - available * expected).abs() <= px(1.));
        assert!((agents.size.height - available * (1. - expected)).abs() <= px(1.));
        assert_eq!(spaces.bottom(), divider.top());
        assert_eq!(divider.bottom(), agents.top());
        assert_eq!(agents.bottom(), footer.top());

        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        let released = view.read_with(cx, |view, _| {
            assert!(view.sidebar_drag.is_none());
            view.sidebar_split
        });
        cx.simulate_mouse_move(sidebar.center(), None, Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert_eq!(view.read_with(cx, |view, _| view.sidebar_split), released);
        assert_eq!(cx.debug_bounds("spaces-section").unwrap(), spaces);
    }

    let position = cx.debug_bounds("sidebar-split-resize").unwrap().center();
    cx.simulate_event(MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_mouse_move(sidebar.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(sidebar.center(), MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_split, None);
        assert!(view.sidebar_drag.is_none());
        assert!(view.sidebar_split_modified);
    });
    assert_eq!(cx.debug_bounds("spaces-section").unwrap(), initial_spaces);
    assert_eq!(cx.debug_bounds("agents-section").unwrap(), initial_agents);
}

#[gpui::test]
fn sidebar_split_preserves_independent_scrolling_and_agents_toggle(cx: &mut gpui::TestAppContext) {
    use gpui::{MouseButton, ScrollDelta, ScrollWheelEvent};

    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.agents = (0..40)
            .map(|i| {
                let mut agent = snapshot.agents[0].clone();
                agent.pane_id = format!("p{i}");
                agent
            })
            .collect();
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let divider = cx.debug_bounds("sidebar-split-resize").unwrap();
    let end = divider.center() + point(px(0.), px(-80.));
    cx.simulate_mouse_down(divider.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let split = view.read_with(cx, |view, _| view.sidebar_split.unwrap());
    assert!(split < 0.5);
    let spaces = cx.debug_bounds("spaces-section").unwrap();
    let agents = cx.debug_bounds("agents-section").unwrap();

    for (index, selector) in [(0, "spaces-scroll"), (1, "agents-scroll")] {
        let before = view.read_with(cx, |view, _| {
            view.sidebar_scroll.each_ref().map(|scroll| scroll.offset())
        });
        let position = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
            ..Default::default()
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        view.read_with(cx, |view, _| {
            assert!(view.sidebar_scroll[index].offset().y < before[index].y);
            assert_eq!(view.sidebar_scroll[1 - index].offset(), before[1 - index]);
            assert_eq!(view.sidebar_split, Some(split));
        });
    }
    let offsets = view.read_with(cx, |view, _| {
        view.sidebar_scroll.each_ref().map(|scroll| scroll.offset())
    });
    for show_agents in [false, true] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.config.show_agents = show_agents;
                cx.notify();
            });
            cx.default_global::<TextProbes>().0.clear();
            window.refresh();
            full_draw(window, cx).clear(cx);
            assert_eq!(
                cx.global::<TextProbes>().0.contains_key("Claude Code"),
                show_agents
            );
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.sidebar_split, Some(split));
            assert_eq!(
                view.sidebar_scroll.each_ref().map(|scroll| scroll.offset()),
                offsets
            );
        });
        let current_spaces = cx.debug_bounds("spaces-section").unwrap();
        if show_agents {
            assert_eq!(current_spaces, spaces);
            assert_eq!(cx.debug_bounds("agents-section").unwrap(), agents);
        } else {
            assert_eq!(
                current_spaces.size.height,
                cx.debug_bounds("sidebar").unwrap().size.height
                    - cx.debug_bounds("device-footer").unwrap().size.height
            );
        }
    }
}

#[cfg(test)]
#[gpui::test]
fn hiding_agents_reclaims_sidebar_height(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    for width in [800., 360.] {
        cx.simulate_resize(size(px(width), px(600.)));
        let mut visible_height = px(0.);
        for show_agents in [true, false, true] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.config.show_agents = show_agents;
                    cx.notify();
                });
                cx.default_global::<TextProbes>().0.clear();
                window.refresh();
                full_draw(window, cx).clear(cx);
            });
            cx.update(|_, cx| {
                assert_eq!(
                    cx.global::<TextProbes>().0.contains_key("Claude Code"),
                    show_agents
                );
            });
            let spaces = cx.debug_bounds("spaces-scroll").unwrap();
            if show_agents {
                visible_height = spaces.size.height;
            } else {
                assert!(spaces.size.height > visible_height + px(100.));
            }
            assert!(cx.debug_bounds("sidebar-menu").is_some());
            assert!(cx.debug_bounds("sidebar-resize").is_some());
        }
    }
}

#[cfg(test)]
#[gpui::test]
fn hiding_agents_preserves_scrolled_multi_endpoint_lists(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.agents = (0..8)
            .map(|i| {
                let mut agent = snapshot.agents[0].clone();
                agent.pane_id = format!("p{i}");
                agent
            })
            .collect();
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            ConnectTarget::Socket("/unused-remote-layout-test.sock".into()),
            true,
        );
        remote.live.snapshot = view.live.snapshot.clone();
        for agent in &mut Arc::make_mut(remote.live.snapshot.as_mut().unwrap()).agents {
            agent.display_agent = Some("Remote Agent".into());
        }
        view.endpoints.push(remote);
        view
    });
    for width in [800., 360.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
            for scroll in &view.read(cx).sidebar_scroll {
                scroll.set_offset(point(px(0.), px(-40.)));
            }
            window.refresh();
            full_draw(window, cx).clear(cx);
        });
        let spaces_height = cx.debug_bounds("spaces-scroll").unwrap().size.height;
        for show_agents in [false, true] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.config.show_agents = show_agents;
                    cx.notify();
                });
                cx.default_global::<TextProbes>().0.clear();
                window.refresh();
                full_draw(window, cx).clear(cx);
                for label in ["Claude Code", "Remote Agent"] {
                    assert_eq!(
                        cx.global::<TextProbes>().0.contains_key(label),
                        show_agents,
                        "{label}"
                    );
                }
                let view = view.read(cx);
                for scroll in &view.sidebar_scroll {
                    assert_eq!(scroll.offset(), point(px(0.), px(-40.)));
                }
                assert_eq!(view.selected_endpoint, 0);
                assert_eq!(view.live.snapshot.as_ref().unwrap().agents.len(), 8);
                assert_eq!(
                    view.endpoints[1]
                        .live
                        .snapshot
                        .as_ref()
                        .unwrap()
                        .agents
                        .len(),
                    8
                );
            });
            let height = cx.debug_bounds("spaces-scroll").unwrap().size.height;
            if show_agents {
                assert_eq!(height, spaces_height);
                for selector in ["agent-local-p0", "agent-ssh:test-p0"] {
                    assert!(cx.debug_bounds(selector).is_some());
                }
            } else {
                assert!(height > spaces_height + px(100.));
            }
        }
    }
}

#[gpui::test]
fn holding_a_workspace_row_lifts_it_and_a_release_picks_the_gap(cx: &mut gpui::TestAppContext) {
    use gpui::{MouseButton, point};

    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let target = |view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let drag = view.workspace_drag.as_ref()?;
            assert!(drag.lifted);
            Some(drag.target.as_ref()?.params())
        })
    };

    // A quick click stays a click.
    let first = cx.debug_bounds("row-herdr").unwrap();
    let first_column = cx.debug_bounds("column-herdr").unwrap();
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(first.center(), MouseButton::Left, Modifiers::default());
    cx.executor().advance_clock(super::reorder::LIFT_DELAY * 2);
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.workspace_drag.is_none()));

    // Held in place, the row lifts without moving.
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(!view.workspace_drag.as_ref().unwrap().lifted)
    });
    cx.executor().advance_clock(super::reorder::LIFT_DELAY);
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| {
        assert!(view.workspace_drag.as_ref().unwrap().lifted)
    });
    // Over its own place it would move nothing, so nothing shifts.
    assert_eq!(target(&view, cx), None);
    let second = cx
        .debug_bounds("row-herdr-gpui-sidebar-rendering-regression-investigation")
        .unwrap();
    assert_eq!(second.top(), first.bottom());

    // Past the second row's middle, it lands before the third.
    let below = point(first.center().x, second.bottom() - px(2.));
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w2"}))
    );
    // The lifted row follows the pointer, and the line marks the gap.
    let lifted = cx.debug_bounds("row-herdr").unwrap();
    assert_eq!(lifted.top() - first.top(), below.y - first.center().y);
    // It lifts as a smaller card, its contents where they were.
    let card = cx.debug_bounds("highlight-herdr").unwrap();
    assert_eq!(card.left() - lifted.left(), px(6.));
    assert_eq!(lifted.right() - card.right(), px(6.));
    assert_eq!(lifted.left(), first.left());
    assert_eq!(
        cx.debug_bounds("column-herdr").unwrap().left(),
        first_column.left()
    );
    // The second row closes the lifted one's place, opening the gap it
    // would land in, and the gaps are still measured where rows rest.
    let passed = "row-herdr-gpui-sidebar-rendering-regression-investigation";
    assert_eq!(cx.debug_bounds(passed).unwrap().top(), first.top());
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w2"}))
    );
    assert_eq!(cx.debug_bounds(passed).unwrap().top(), first.top());
    // The card's bottom edge passing the second row's resting middle is
    // enough, though that row now paints higher.
    let past = point(below.x, first.center().y + second.size.height / 2. + px(2.));
    cx.simulate_mouse_move(past, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w2"}))
    );
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));

    // The release is the drop, not a click on the row under it.
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| assert!(view.workspace_drag.is_none()));
    // Nothing was sent without a daemon, so every row is back in place.
    assert_eq!(cx.debug_bounds("row-herdr").unwrap(), first);
    assert_eq!(cx.debug_bounds(passed).unwrap(), second);

    // A linked worktree moves among its siblings only, and a drag lifts it
    // without waiting. The gap resolves against the lifted frame's layout.
    let child = cx.debug_bounds("row-sidebar-child").unwrap();
    cx.simulate_mouse_down(child.center(), MouseButton::Left, Modifiers::default());
    for rows in [1., 5.] {
        cx.simulate_mouse_move(
            point(child.center().x, child.bottom() + child.size.height * rows),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    }
    assert_eq!(
        target(&view, cx),
        Some(serde_json::json!({"workspace_ids": ["w4"], "before_workspace_id": "w6"}))
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| assert!(view.workspace_drag.is_none()));
    assert_eq!(cx.debug_bounds("row-sidebar-child").unwrap(), child);
}

/// Dragging works the same in every row layout: the carried row follows the
/// pointer, the row it passes closes its place, and a release without a
/// daemon puts every row back.
#[cfg(test)]
fn check_row_drag(style: crate::config::LayoutMode, cx: &mut gpui::TestAppContext) {
    use gpui::{MouseButton, point};

    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.layout.mode = style;
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let passed = "row-herdr-gpui-sidebar-rendering-regression-investigation";
    let first = cx.debug_bounds("row-herdr").unwrap();
    let second = cx.debug_bounds(passed).unwrap();
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    cx.executor().advance_clock(super::reorder::LIFT_DELAY);
    cx.run_until_parked();
    let below = point(first.center().x, second.bottom() - px(2.));
    for _ in 0..2 {
        cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    }
    view.read_with(cx, |view, _| {
        let drag = view.workspace_drag.as_ref().unwrap();
        assert!(drag.lifted, "{style:?}");
        assert_eq!(
            drag.target.as_ref().map(|target| target.params()),
            Some(serde_json::json!({"workspace_ids": ["w0"], "before_workspace_id": "w2"})),
            "{style:?}"
        );
    });
    let lifted = cx.debug_bounds("row-herdr").unwrap();
    assert_eq!(
        lifted.top() - first.top(),
        below.y - first.center().y,
        "{style:?}"
    );
    assert_eq!(
        lifted.size, first.size,
        "{style:?}: lifting resized the row"
    );
    assert_eq!(
        cx.debug_bounds(passed).unwrap().top(),
        first.top(),
        "{style:?}"
    );
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    view.read_with(cx, |view, _| assert!(view.workspace_drag.is_none()));
    assert_eq!(cx.debug_bounds("row-herdr").unwrap(), first, "{style:?}");
    assert_eq!(cx.debug_bounds(passed).unwrap(), second, "{style:?}");
}

#[gpui::test]
fn superset_rows_lift_and_drop(cx: &mut gpui::TestAppContext) {
    check_row_drag(crate::config::LayoutMode::Superset, cx);
}

#[gpui::test]
fn orca_rows_lift_and_drop(cx: &mut gpui::TestAppContext) {
    check_row_drag(crate::config::LayoutMode::Orca, cx);
}

#[gpui::test]
fn minimal_rows_lift_and_drop(cx: &mut gpui::TestAppContext) {
    check_row_drag(crate::config::LayoutMode::Minimal, cx);
}

#[gpui::test]
fn choosing_a_layout_redraws_the_sidebar_and_saves_it(cx: &mut gpui::TestAppContext) {
    use crate::config::{Density, LayoutMode, Style};
    use std::sync::{Arc as SyncArc, Mutex};

    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(900.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("icon-herdr").is_none());
    let saved = SyncArc::new(Mutex::new(Vec::new()));
    let compact = LayoutMode::new(Density::Compact, Style::Rounded);
    let modes = [LayoutMode::Superset, LayoutMode::Superset, compact];
    for mode in modes {
        let record = saved.clone();
        view.update(cx, |view, cx| {
            view.set_layout_with(
                mode,
                move |mode| {
                    record.lock().unwrap().push(mode);
                    Ok(())
                },
                cx,
            )
        });
        cx.run_until_parked();
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        view.read_with(cx, |view, _| assert_eq!(view.config.layout.mode, mode));
        if mode == LayoutMode::Superset {
            assert!(cx.debug_bounds("icon-herdr").is_some());
        }
    }
    // Choosing the layout already in use saves nothing.
    assert_eq!(*saved.lock().unwrap(), vec![modes[0], modes[2]]);
}

/// Every entry under View > Layout draws the sidebar its own way: no two
/// share the same row heights, name placement, and highlight.
#[gpui::test]
fn every_layout_looks_different(cx: &mut gpui::TestAppContext) {
    use crate::config::LayoutMode;
    let mut seen: Vec<(LayoutMode, Vec<Pixels>)> = Vec::new();
    for mode in LayoutMode::ALL {
        // A fresh window per layout: debug bounds outlive their elements.
        let (_, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            view.config.layout.mode = mode;
            view
        });
        cx.simulate_resize(size(px(800.), px(900.)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let row = cx.debug_bounds("row-herdr").unwrap();
        let name = cx.debug_bounds("name-herdr").unwrap();
        let agent = cx.debug_bounds("row-agent-p0").unwrap();
        let highlight = cx
            .debug_bounds("highlight-herdr")
            .map_or(px(-1.), |h| h.left() - row.left());
        let signature = vec![
            row.size.height,
            agent.size.height,
            name.left() - row.left(),
            name.top() - row.top(),
            highlight,
        ];
        if let Some((other, _)) = seen.iter().find(|(_, other)| *other == signature) {
            panic!("{mode} draws the same as {other}: {signature:?}");
        }
        seen.push((mode, signature));
    }
}
