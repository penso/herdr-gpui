//! The activity bar: an icon column at the window's left edge that picks
//! what the window shows. Workspaces is the sidebar beside the terminals, as
//! it was; every other section takes the window's body, keeping the bar,
//! title bar, and status bar: Overview what needs the user on every host
//! (`crate::overview`), Orchestrator, Pull requests, and Issues the focused
//! workspace's orchestrator on that list (see `orchestrator::section`), and
//! Usage the plan limits of every host (`crate::usage::section`).
//! `[sidebar] activity_bar = false` turns the bar off, and the collapsed
//! rail of the Workspaces section never shows it.

use super::{agents::Indicators, line_height};
use crate::fonts::StyledFont;
use crate::{
    Command, HerdrWindow,
    orchestrator::{List, OrchestratorView, ViewId},
};
use gpui::{prelude::*, *};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod counts;

#[cfg(test)]
mod tests;

/// The bar's width: a 32px button with a margin either side.
pub(crate) const BAR_WIDTH: f32 = 44.;
const BUTTON: f32 = 32.;
const ICON: f32 = 17.;
const TITLE_HEIGHT: f32 = 32.;
/// One breath of a busy icon's pulse.
const BUSY_PULSE: std::time::Duration = std::time::Duration::from_millis(1600);

/// What the window shows, picked on the bar. The bar lists them in
/// declaration order, so this order is what users see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityPanel {
    #[default]
    Workspaces,
    Overview,
    PullRequests,
    Issues,
    Usage,
}

impl ActivityPanel {
    pub(crate) const ALL: &[Self] = &[
        Self::Workspaces,
        Self::Overview,
        Self::PullRequests,
        Self::Issues,
        Self::Usage,
    ];

    /// The name the CLI and the preferences file use.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Workspaces => "workspaces",
            Self::PullRequests => "pull-requests",
            Self::Issues => "issues",
            Self::Usage => "usage",
        }
    }

    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|panel| panel.name() == name)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Workspaces => "Workspaces",
            Self::PullRequests => "Pull requests",
            Self::Issues => "Issues",
            Self::Usage => "Usage",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Overview => "icons/pulse.svg",
            Self::Workspaces => "icons/terminal.svg",
            Self::PullRequests => "icons/pull-request.svg",
            Self::Issues => "icons/issue.svg",
            Self::Usage => "icons/chart.svg",
        }
    }
}

/// The window's bar: the section it shows, and the orchestrator view a
/// section shows.
#[derive(Debug, Default)]
pub(crate) struct Activity {
    pub(crate) panel: ActivityPanel,
    /// Picked in this window, so a stored section arriving later yields to it.
    picked: bool,
    /// The orchestrator view the shown section draws.
    pub(crate) section_view: Option<ViewId>,
    /// The project the next section shown narrows to, as a menu or the
    /// `orchestrator` command asked.
    pub(crate) section_project: Option<String>,
    /// The list last picked on that tab's view, so the user's own choice of
    /// list there stands until another section is picked.
    pub(crate) section_list: Option<List>,
    /// The section the window left for a pane it opened, which Back
    /// returns to before walking the focus trail.
    pub(crate) return_to: Option<ActivityPanel>,
    /// What holds the keyboard while Overview or Usage is shown, so typing
    /// never reaches a terminal the section hides.
    pub(crate) section_focus: Option<FocusHandle>,
    /// Each orchestrator view's lists and live agents as last counted, by
    /// identity, so a new snapshot or a live status change redraws the bar
    /// and nothing else does.
    sources: Vec<(ViewId, [usize; 3])>,
}

impl Activity {
    /// Shows `panel`, as picked by the user; whether it changed.
    pub(crate) fn pick(&mut self, panel: ActivityPanel) -> bool {
        if panel == self.panel {
            return false;
        }
        self.panel = panel;
        self.picked = true;
        self.section_list = None;
        true
    }

    /// The section to store: only one the user picked.
    pub(crate) fn chosen(&self) -> Option<ActivityPanel> {
        self.picked.then_some(self.panel)
    }

    /// Applies the stored section unless one was picked before it loaded.
    pub(crate) fn restore(&mut self, stored: Option<ActivityPanel>) {
        if let Some(panel) = stored.filter(|_| !self.picked) {
            self.panel = panel;
            self.picked = true;
        }
    }

    /// Records the views' lists; whether they changed since last time.
    pub(crate) fn observe_sources(
        &mut self,
        views: &[(ViewId, Entity<OrchestratorView>)],
        cx: &App,
    ) -> bool {
        let mut sources: Vec<_> = views
            .iter()
            .map(|(id, view)| {
                let view = view.read(cx);
                (
                    *id,
                    [
                        usize::try_from(view.revision()).unwrap_or(usize::MAX),
                        Arc::as_ptr(view.live()) as usize,
                        0,
                    ],
                )
            })
            .collect();
        sources.sort_unstable();
        if sources == self.sources {
            return false;
        }
        self.sources = sources;
        true
    }
}

impl HerdrWindow {
    /// Whether the window leads with the bar.
    pub(crate) fn activity_bar_shown(&self) -> bool {
        self.config.sidebar_style.activity_bar
    }

    /// The height the Workspaces title takes over the sidebar, or 0 with
    /// no bar.
    pub(crate) fn activity_title_height(&self) -> f32 {
        if self.activity_bar_shown() {
            TITLE_HEIGHT.max(line_height(&self.config.sidebar) + 8.)
        } else {
            0.
        }
    }

    /// The section that takes the window's body, if one other than
    /// Workspaces is shown. Zen mode keeps the terminals.
    pub(crate) fn activity_section(&self) -> Option<ActivityPanel> {
        (self.activity_bar_shown() && !self.zen_layout())
            .then_some(self.activity.panel)
            .filter(|panel| *panel != ActivityPanel::Workspaces)
    }

    /// The left column's width: the bar alone beside a section, else the
    /// sidebar as its mode lays it out with the bar's room added beside an
    /// expanded one.
    pub(crate) fn sidebar_column_width(&self, window_width: f32) -> Option<f32> {
        if self.activity_section().is_some() {
            return Some(BAR_WIDTH);
        }
        let mode = self.sidebar_mode();
        let width = mode.width(self.sidebar_width, window_width)?;
        let bar = mode == super::SidebarMode::Expanded && self.activity_bar_shown();
        Some(if bar { width + BAR_WIDTH } else { width })
    }

    /// Shows `panel`. Picking Workspaces while it is shown collapses the
    /// sidebar instead, as a second click on a tab bar's selected icon does.
    pub(crate) fn select_activity_panel(
        &mut self,
        panel: ActivityPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if panel == ActivityPanel::Workspaces
            && self.activity.panel == panel
            && self.sidebar_visible
        {
            self.toggle_sidebar();
            cx.notify();
        } else {
            self.show_activity_panel(panel, window, cx);
        }
    }

    /// Shows `panel`, whatever was shown before: Workspaces with its sidebar
    /// expanded, a section with its orchestrator holding the keyboard.
    pub(crate) fn show_activity_panel(
        &mut self,
        panel: ActivityPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if panel == ActivityPanel::Workspaces && !self.sidebar_visible {
            self.toggle_sidebar();
        }
        // A section picked is where the user is now, not a place to go back to.
        self.activity.return_to = None;
        if self.activity.pick(panel) {
            self.save_chrome();
        }
        self.sync_activity_section(cx);
        match self.activity_section() {
            Some(ActivityPanel::Overview | ActivityPanel::Usage) => {
                let focus = self
                    .activity
                    .section_focus
                    .get_or_insert_with(|| cx.focus_handle())
                    .clone();
                window.focus(&focus, cx);
            }
            Some(_) => self.focus_activity_section(window, cx),
            None => {}
        }
        cx.notify();
    }

    /// The shown section across the window's body, or `None` in Workspaces.
    pub(crate) fn render_activity_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let section = match self.activity_section()? {
            ActivityPanel::Workspaces => return None,
            ActivityPanel::Overview => self.render_overview(cx),
            ActivityPanel::Usage => self.render_usage_section(cx),
            ActivityPanel::PullRequests | ActivityPanel::Issues => {
                return self.render_orchestrator_section();
            }
        };
        let focus = self.activity.section_focus.clone();
        Some(
            div()
                .flex()
                .flex_1()
                .size_full()
                .min_w_0()
                .min_h_0()
                .when_some(focus, |section, focus| section.track_focus(&focus))
                .child(section)
                .into_any_element(),
        )
    }

    /// The left column with the bar: beside a section, the bar alone; in
    /// Workspaces, the bar and the sidebar under the section's name.
    pub(crate) fn render_activity(
        &self,
        indicators: Indicators,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let column = div()
            .id("activity")
            .debug_selector(|| "activity".into())
            .flex()
            .flex_none()
            .h_full()
            .min_h_0()
            .child(self.render_activity_bar(cx));
        if self.activity_section().is_some() {
            return column;
        }
        column.child(
            div()
                .flex()
                .flex_col()
                .h_full()
                .min_h_0()
                .child(self.panel_title(ActivityPanel::Workspaces))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_sidebar(indicators, window, cx)),
                ),
        )
    }

    fn render_activity_bar(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let counts = self.activity_counts(cx);
        let mut bar = div()
            .debug_selector(|| "activity-bar".into())
            .w(px(BAR_WIDTH))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .py(px(6.))
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_r_1()
            .border_color(rgb(theme.active));
        for &panel in ActivityPanel::ALL {
            let state = if panel == self.activity.panel {
                BarState::Selected
            } else {
                BarState::Idle
            };
            // Overview's icon pulses while agents work, as the sidebar's
            // working dots do.
            let busy = panel == ActivityPanel::Overview && self.overview_working() > 0;
            let button = self
                .bar_button(
                    panel_selector(panel),
                    panel.icon(),
                    panel.label(),
                    (state, busy),
                    counts.of(panel),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_activity_panel(panel, window, cx);
                }));
            bar = bar.child(button);
        }
        bar.child(div().flex_1()).child(
            self.bar_button(
                "activity-settings",
                "icons/settings.svg",
                "Settings",
                (BarState::Idle, false),
                None,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.command(Command::Settings, window, cx);
            })),
        )
    }

    fn bar_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: &'static str,
        (state, busy): (BarState, bool),
        count: Option<usize>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let (foreground, surface, hover) = (theme.foreground, theme.surface, theme.active);
        let ink = match state {
            BarState::Selected => theme.foreground,
            BarState::Idle => theme.muted,
        };
        let icon = svg()
            .path(icon)
            .size(px(ICON))
            .flex_none()
            .text_color(rgb(ink));
        let icon = if busy {
            icon.with_animation(
                SharedString::from(format!("{id}-busy")),
                Animation::new(BUSY_PULSE).repeat(),
                |icon, delta| icon.opacity(0.35 + 0.65 * (delta * std::f32::consts::PI).sin()),
            )
            .into_any_element()
        } else {
            icon.into_any_element()
        };
        let glyph = div()
            .debug_selector(move || format!("{id}-glyph"))
            .relative()
            .child(icon)
            .when_some(count.filter(|count| *count > 0), |glyph, count| {
                glyph.child(
                    div()
                        .absolute()
                        .top(px(-8.))
                        .left(px(10.))
                        .child(self.count_bubble(count)),
                )
            });
        div()
            .id(id)
            .debug_selector(move || id.into())
            .relative()
            .size(px(BUTTON))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .tooltip(move |_, cx| {
                cx.new(|_| crate::menu::SettingsHint {
                    text: label.into(),
                    foreground,
                    surface,
                })
                .into()
            })
            .when(state == BarState::Selected, |button| {
                button.bg(rgb(theme.primary_wash())).child(
                    div()
                        .absolute()
                        .left(px(-6.))
                        .top(px(6.))
                        .bottom(px(6.))
                        .w(px(2.))
                        .rounded_full()
                        .bg(rgb(theme.primary())),
                )
            })
            .when(state == BarState::Idle, |button| {
                button
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
            })
            .child(glyph)
    }

    fn count_bubble(&self, count: usize) -> Div {
        let theme = &self.theme;
        let text = if count > 99 {
            "99+".to_owned()
        } else {
            count.to_string()
        };
        div()
            .min_w(px(14.))
            .h(px(14.))
            .px(px(3.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(rgb(theme.sidebar_background()))
            .bg(rgb(theme.primary()))
            .text_color(rgb(theme.text_on(theme.primary())))
            .text_size(px(9.))
            .line_height(px(12.))
            .child(text)
    }

    /// The section's name over the sidebar, across its width.
    fn panel_title(&self, panel: ActivityPanel) -> Div {
        let theme = &self.theme;
        let font = &self.config.sidebar;
        div()
            .debug_selector(|| "activity-title".into())
            .flex_none()
            .h(px(self.activity_title_height()))
            .px(px(12.))
            .flex()
            .items_center()
            .text_font(font)
            .text_size(px(font.size - 1.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(theme.subtext()))
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_r_1()
            .border_color(rgb(theme.active))
            .child(panel.label().to_uppercase())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BarState {
    Selected,
    Idle,
}

fn panel_selector(panel: ActivityPanel) -> &'static str {
    match panel {
        ActivityPanel::Overview => "activity-overview",
        ActivityPanel::Workspaces => "activity-workspaces",
        ActivityPanel::PullRequests => "activity-pull-requests",
        ActivityPanel::Issues => "activity-issues",
        ActivityPanel::Usage => "activity-usage",
    }
}
