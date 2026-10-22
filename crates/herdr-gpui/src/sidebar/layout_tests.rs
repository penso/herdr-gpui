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

#[cfg(test)]
mod agent_menu;
#[cfg(test)]
mod agent_rows;
#[cfg(test)]
mod agents_header;
#[cfg(all(test, feature = "coder"))]
mod coder_dialog;
#[cfg(test)]
mod configured_rows;
#[cfg(test)]
mod device_footer;
#[cfg(test)]
mod device_footer_setting;
#[cfg(all(test, feature = "azure"))]
mod device_removing;
#[cfg(test)]
mod devices_layout;
#[cfg(test)]
mod host_agents;
#[cfg(test)]
mod host_groups;
#[cfg(test)]
mod host_nesting;
#[cfg(test)]
mod host_origin;
#[cfg(test)]
mod host_wash;
#[cfg(test)]
mod layouts;
#[cfg(test)]
mod listening_ports;
#[cfg(test)]
mod local_counts_chip;
#[cfg(test)]
mod nested_agents;
#[cfg(test)]
mod other_device_prs;
#[cfg(test)]
mod palette;
#[cfg(test)]
mod palette_header;
#[cfg(test)]
mod pane_names;
#[cfg(test)]
mod picker_scroll;
#[cfg(test)]
mod plain_folder_icon;
#[cfg(test)]
mod probe_bar;
#[cfg(test)]
mod probe_scale;
#[cfg(test)]
mod probes;
#[cfg(test)]
mod repository_icons;
#[cfg(test)]
mod row_drag;
#[cfg(test)]
mod search;
#[cfg(test)]
mod selection_scroll;
#[cfg(test)]
mod sidebar_cache;
#[cfg(test)]
mod spacing_overrides;
#[cfg(test)]
mod split_pane;
#[cfg(test)]
mod status_bar;
#[cfg(test)]
mod status_icons;
#[cfg(test)]
mod sticky_hosts;
#[cfg(test)]
mod superset_badges;
#[cfg(test)]
mod superset_dots;
#[cfg(test)]
mod text_width;
#[cfg(test)]
mod update_notes;
#[cfg(test)]
mod update_notes_scrollbar;
#[cfg(test)]
mod update_panel;
#[cfg(test)]
mod workspace_click_hover;
#[cfg(test)]
mod workspace_menu;
#[cfg(test)]
mod workspace_menu_name;
#[cfg(test)]
mod worktree_note_trunk;
#[cfg(test)]
mod worktree_notes;
#[cfg(test)]
mod worktree_rows;

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
            // The expected column assumes the window leaves the default
            // sidebar width alone. A frame painted mid-resize, such as the
            // return from a 320px check, can narrow it.
            && super::sidebar_width(None, f32::from(window.viewport_size().width))
                == super::SIDEBAR_WIDTH
        {
            // The activity bar, when shown, puts the sidebar beside it.
            let (mode, bar) = window
                .root::<HerdrWindow>()
                .flatten()
                .map(|view| {
                    let view = view.read(cx);
                    let bar = if view.activity_bar_shown() {
                        super::activity::BAR_WIDTH
                    } else {
                        0.
                    };
                    (view.config.layout.mode, bar)
                })
                .unwrap_or_default();
            probe.verify_child(&self.0, mode, bar, window.scale_factor())?;
            eprintln!("SIDEBAR child verified: {}", probe.glyph_text);
        }
        Ok(probe)
    }
}

/// Whether a painted edge or width is within one device pixel of its logical
/// value. Layout snaps edges to device pixels (`scale` per logical pixel), so
/// at a fractional scale such as Windows' default 150% a column may land a
/// fraction of a logical pixel from where it was laid out.
pub(crate) fn within_device_pixel(actual: Pixels, expected: Pixels, scale: f32) -> bool {
    (actual - expected).abs() < px(1. / scale.max(1.))
}

impl PaintedText {
    /// `bar` is the width of the activity bar left of the sidebar, if any;
    /// `scale` is the window's device pixels per logical pixel; see
    /// [`within_device_pixel`].
    fn verify_child(
        &self,
        input: &str,
        mode: crate::config::LayoutMode,
        bar: f32,
        scale: f32,
    ) -> Result<()> {
        let near = |actual, expected| within_device_pixel(actual, expected, scale);
        let look = super::layout::for_mode(mode);
        let layout = look.density;
        // Menu labels share text keys, and retained paint probes can still
        // describe the previous layout. Compute the sidebar column directly.
        // The child row draws no fold chevron, so its label also takes the
        // chevron's column.
        let left =
            px(bar + look.content_x() + layout.child_indent() + super::STATUS_WIDTH + layout.gap());
        // A worktree group's member draws no chevron, so its name also
        // takes the arrow column.
        let width = px(look.content_width(super::SIDEBAR_WIDTH)
            - super::STATUS_WIDTH
            - layout.gap()
            - layout.child_indent());
        ensure!(
            near(self.bounds.left(), left),
            "child label left: actual {:?}, expected {left:?}",
            self.bounds.left()
        );
        ensure!(
            near(self.bounds.size.width, width),
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
            // GPUI keeps the longest prefix that fits beside the ellipsis
            // and drops the glyph that would not, so a correct truncation
            // can fall one glyph plus the ellipsis short. Segoe UI on Windows
            // leaves 13px where macOS's system font left under 12.
            ensure!(
                self.width > self.bounds.size.width - px(16.),
                "child glyph width {:?} did not fill label width {:?}",
                self.width,
                self.bounds.size.width
            );
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) struct SidebarFixture(pub(crate) Entity<HerdrWindow>);

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
            "workspace_id": format!("w{i}"), "active_tab_id": "t0",
            // The main checkout's shell stands in its repository, as Herdr opens it.
            "new_workspace_cwd": if i == 3 { REPO_KEY.trim_end_matches("/.git") } else { "/tmp" },
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

#[cfg(test)]
/// Draw one preview state in a freshly opened panel and return its bounds.
pub(crate) fn draw_update_state(
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
    let sidebar_view = {
        let weak = cx.weak_entity();
        cx.new(|_| {
            crate::sidebar::SidebarView::new(
                weak,
                super::agents::Indicators::new(
                    None,
                    crate::config::StatusIcons::default(),
                    false,
                    false,
                    &crate::config::Theme::default(),
                ),
            )
        })
    };
    let hosts = vec![cx.entity_id(), sidebar_view.entity_id()];
    let mark_layer = cx.new(|cx| crate::mark_layer::MarkLayer::new(window, hosts, cx));
    let focus = cx.focus_handle();
    let terminal_focus = HerdrWindow::leave_region_with_terminal(&focus, window, cx);
    HerdrWindow {
        _terminal_focus: terminal_focus,
        chrome_region: None,
        sound: Default::default(),
        bell: Default::default(),
        updater: crate::updater::Updater::default(),
        update_preview: None,
        daemon_text: Default::default(),
        default_browser: crate::window::start_browser(),
        app_update_notes: Default::default(),
        app_update_notes_scroll: gpui::ScrollHandle::new(),
        removal: None,
        merged_cleanup: Default::default(),
        #[cfg(feature = "integration-test")]
        navigations: Default::default(),
        worktree_script: None,
        wsl_paste: None,
        editor_open: None,
        editor_panes: Vec::new(),
        teleport: None,
        teleport_marks: crate::teleport::Marks::detached(),
        teleport_follow: None,
        pane_carry: None,
        dispatch_job: None,
        dispatch_setups: Default::default(),
        fan_out: None,
        selection: None,
        selection_follow: Default::default(),
        find: None,
        copy_mode: None,
        find_memory: Default::default(),
        flash: None,
        configured_terminal_size: crate::config::Config::default().terminal.size,
        gui_config_diagnostic: Default::default(),
        icon_font_notice: Default::default(),
        // Keep the original geometry fixture explicit; density-switching tests
        // above exercise all three modes independently of the default.
        config: crate::config::Config {
            layout: crate::config::Layout {
                mode: crate::config::LayoutMode::from(crate::config::Density::Comfortable),
                ..Default::default()
            },
            // The plain sidebar, at the window's edge; the activity bar's
            // tests turn the bar on.
            sidebar_style: crate::config::SidebarStyle {
                activity_bar: false,
                ..Default::default()
            },
            // Disable window controls so the menu bar's movable handler doesn't
            // consume mouse events that tests expect to bubble.
            show_window_controls: false,
            ..Default::default()
        },
        theme: Default::default(),
        theme_light: crate::app::light_appearance(cx),
        config_load: None,
        config_watch: None,
        config_load_revision: 0,
        git: Default::default(),
        deliveries: Default::default(),
        terminal_notes: Default::default(),
        notes_width: crate::panel_resize::NOTES,
        notice_overflow: Default::default(),
        review_files_width: crate::panel_resize::REVIEW_FILES,
        reviews: Default::default(),
        code_views: Default::default(),
        code_indexes: Default::default(),
        orchestrators: Default::default(),
        orchestrator_live: Default::default(),
        next_orchestrator: 0,
        orchestrator_events: Vec::new(),
        orchestrator_sampling: false,
        overview: Default::default(),
        viewport_width: 0.,
        pr_actions: Default::default(),
        usage: Default::default(),
        system_load: Default::default(),
        network_quality_seen: 0,
        devices_overview: Default::default(),
        checkpoints: Default::default(),
        port_forwards: Default::default(),
        remote_browser: Vec::new(),
        remote_files: Default::default(),
        #[cfg(feature = "cloud")]
        cloud_jobs: Default::default(),
        #[cfg(feature = "cloud")]
        cloud_setup: None,
        listening_ports: Default::default(),
        tunnels: Default::default(),
        sidebar_visible: true,
        sidebar_start_pending: true,
        status_bar_visible: true,
        zen: Default::default(),
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
        arrival: None,
        strip_navigation_fits: true,
        pending_toast: None,
        toasts_hidden: false,
        pending_releases: Vec::new(),
        selected_generation: 0,
        live: {
            let mut live = LiveState::default();
            live.snapshot = Some(Arc::new(snapshot(40)));
            live
        },
        focus,
        options: ConnectOptions::default(),
        last_queued_options: None,
        pending_resize: None,
        last_input: None,
        last_pointer: None,
        active: false,
        activity: Default::default(),
        sent_focus: None,
        bounds: Bounds::default(),
        title: crate::WINDOW_TITLE.to_owned(),
        cell_width: 9.,
        hovered_terminal_link: false,
        pressed_terminal_link: None,
        links: Default::default(),
        terminal_mouse: None,
        scrollbar_drag: None,
        security: Default::default(),
        security_search: crate::security::SecuritySearch::new(cx),
        split_drag: None,
        split_cursor: None,
        pending_images: Vec::new(),
        pending_input: Default::default(),
        held_keys: Default::default(),
        // No AppKit event to read; a test that needs one supplies its own.
        key_press: || None,
        key_layout: |_| None,
        file_transfer: None,
        presentation: Default::default(),
        wake: crate::endpoint::WakeClock::new(
            std::time::Instant::now(),
            std::time::SystemTime::now(),
        ),
        painter: Default::default(),
        backdrop: Default::default(),
        device_themes: Default::default(),
        regions: Vec::new(),
        mark_layer,
        marked: String::new(),
        marked_selection: None,
        hover: None,
        hover_menu: None,
        local_error: None,
        menu: crate::menu::MenuState::new(cx),
        settings: Default::default(),
        integrations: Default::default(),
        install_warning_shown: false,
        version_notice_shown: false,
        collapsed_repos: Default::default(),
        wheel: WheelAccumulator::default(),
        sidebar_width: None,
        sidebar_drag: None,
        workspace_drag: None,
        device_drag: None,
        device_order: Vec::new(),
        tab_drag: None,
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
        sidebar_pin_reveal: Default::default(),
        sidebar_nested_reveal: Default::default(),
        sidebar_folded: Default::default(),
        sidebar_search: crate::sidebar::SidebarSearch::new(cx),
        _poll: Task::ready(()),
        _activation: cx.observe_window_activation(window, |_, _, _| {}),
        _window_closed: HerdrWindow::give_back_torn_tabs(cx),
        _appearance: cx.observe_window_appearance(window, |this, _, cx| {
            this.apply_shared_theme(cx);
            cx.notify();
        }),
        sidebar_view,
        surface_signal: cx.new(|_| crate::window::SurfaceSignal),
        _sidebar_invalidation: HerdrWindow::invalidate_sidebar(cx),
        _host_theme: HerdrWindow::observe_host_theme(cx),
        browser: crate::browser::Browser::new(cx),
        _browser_tabs: cx.observe_global::<crate::browser::Store>(|_, cx| cx.notify()),
        _worktree_notes: HerdrWindow::observe_worktree_notes(window, cx),
        prefix_armed: false,
        resize_mode: false,
        server_keys: None,
        shift_taps: Default::default(),
        _prefix_interceptor: HerdrWindow::intercept_prefix(window, cx),
    }
}
