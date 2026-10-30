//! The activity bar: an icon column at the window's left edge that picks
//! what the window shows. Workspaces is the sidebar beside the terminals, as
//! it was; every other section takes the window's body, keeping the bar,
//! title bar, and status bar: Overview what needs the user on every host
//! (`crate::overview`), Orchestrator, Pull requests, and Issues the focused
//! workspace's orchestrator on that list (see `orchestrator::section`),
//! Security the Dependabot alerts of every repository (`crate::security`),
//! listed only once GitHub reports them on for one, Automations the
//! `herdr-automate` scripts and runs of every host (`crate::automations`),
//! and Usage the plan limits of every host (`crate::usage::section`).
//! `[sidebar] activity_bar = false` turns the bar off, and `[sidebar]
//! automations = false` leaves Automations off it. With the bar on, the
//! sidebar toggle sits at the end of the Workspaces title, and collapsing the
//! sidebar keeps the bar, with the compact rail and the toggle beside it.

use super::{SidebarMode, agents::Indicators, line_height};
use crate::direction::Logical;
use crate::fonts::StyledFont;
use crate::i18n::fl;
use crate::{
    Command, HerdrWindow,
    orchestrator::{List, OrchestratorView, ViewId},
};
use chrono::{DateTime, Utc};
use gpui::{prelude::*, *};
pub use herdr_control_protocol::ActivityPanel;
use std::{collections::HashMap, sync::Arc};

mod counts;

#[cfg(test)]
mod tests;

/// The bar's width: a 32px button with a margin either side.
pub(crate) const BAR_WIDTH: f32 = 44.;
pub(crate) const BUTTON: f32 = 32.;
pub(crate) const ICON: f32 = 17.;
/// The count bubble's right edge past the icon's: to 2.5px short of the
/// bar's edge, inside its 1px border, whatever the count's width.
const BUBBLE_END: f32 = -((BAR_WIDTH - ICON) / 2. - 2.5);
const TITLE_HEIGHT: f32 = 32.;
/// Room after the toggle at the title's end, so its glyph lines up with the
/// rows' trailing badges below.
const TITLE_END: f32 = 4.;
/// One breath of a busy icon's pulse.
const BUSY_PULSE: std::time::Duration = std::time::Duration::from_millis(1600);

fn label(panel: ActivityPanel) -> SharedString {
    match panel {
        ActivityPanel::Overview => fl!("sidebar-activity-overview"),
        ActivityPanel::Workspaces => fl!("sidebar-activity-workspaces"),
        ActivityPanel::PullRequests => fl!("sidebar-activity-pull-requests"),
        ActivityPanel::Issues => fl!("sidebar-activity-issues"),
        ActivityPanel::Security => fl!("sidebar-activity-security"),
        ActivityPanel::Automations => fl!("sidebar-activity-automations"),
        ActivityPanel::Usage => fl!("sidebar-activity-usage"),
    }
    .into()
}

fn icon(panel: ActivityPanel) -> &'static str {
    match panel {
        ActivityPanel::Overview => "icons/pulse.svg",
        ActivityPanel::Workspaces => "icons/terminal.svg",
        ActivityPanel::PullRequests => "icons/pull-request.svg",
        ActivityPanel::Issues => "icons/issue.svg",
        ActivityPanel::Security => "icons/shield.svg",
        ActivityPanel::Automations => "icons/automations.svg",
        ActivityPanel::Usage => "icons/chart.svg",
    }
}

/// The window's bar: the section it shows, and the orchestrator view a
/// section shows.
#[derive(Debug, Default)]
pub(crate) struct Activity {
    pub(crate) panel: ActivityPanel,
    /// Picked in this window, so a stored section arriving later yields to it.
    picked: bool,
    /// When each section was last open, so its count shows in the accent
    /// only for what changed since.
    last_opened: HashMap<ActivityPanel, DateTime<Utc>>,
    /// The orchestrator view the shown section draws.
    pub(crate) section_view: Option<ViewId>,
    /// A mod's app picked from the bar, shown over `panel` until a section
    /// is picked (`crate::mods`).
    pub(crate) app: Option<String>,
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
    /// Shows `panel`, as picked by the user; whether it changed. The section
    /// left and the one shown are both stamped as open now, since what the
    /// user looked at while it was shown counts as seen.
    pub(crate) fn pick(&mut self, panel: ActivityPanel) -> bool {
        if panel == self.panel {
            return false;
        }
        let now = Utc::now();
        self.opened(self.panel, now);
        self.opened(panel, now);
        self.panel = panel;
        self.picked = true;
        self.section_list = None;
        true
    }

    /// Records `panel` as open at `at`, never moving a stamp back.
    fn opened(&mut self, panel: ActivityPanel, at: DateTime<Utc>) {
        let stamp = self.last_opened.entry(panel).or_insert(at);
        *stamp = (*stamp).max(at);
    }

    /// When each section was last open, to store.
    pub(crate) fn last_opened(&self) -> &HashMap<ActivityPanel, DateTime<Utc>> {
        &self.last_opened
    }

    /// Whether `panel`'s count holds something the user has not seen: the
    /// section was never opened, or an item changed after it last was.
    /// The section on show is being looked at, so it is never unseen.
    pub(super) fn unseen(&self, panel: ActivityPanel, latest: Option<DateTime<Utc>>) -> bool {
        if panel == self.panel {
            return false;
        }
        match self.last_opened.get(&panel) {
            None => true,
            Some(opened) => latest.is_some_and(|latest| latest > *opened),
        }
    }

    /// The section to store: only one the user picked.
    pub(crate) fn chosen(&self) -> Option<ActivityPanel> {
        self.picked.then_some(self.panel)
    }

    /// Applies the stored section unless one was picked before it loaded,
    /// and the stored visits, keeping any later one made since. Whether the
    /// visits now differ from the stored ones: a save queued before they
    /// loaded wrote this run's alone, so the merge must be saved again.
    pub(crate) fn restore(
        &mut self,
        stored: Option<ActivityPanel>,
        last_opened: &HashMap<ActivityPanel, DateTime<Utc>>,
    ) -> bool {
        if let Some(panel) = stored.filter(|_| !self.picked) {
            self.panel = panel;
            self.picked = true;
        }
        for (&panel, &at) in last_opened {
            self.opened(panel, at);
        }
        self.last_opened != *last_opened
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

    /// Whether the bar lists `panel`. A section turned off in the settings
    /// is not listed, and a window left on it shows Workspaces.
    pub(crate) fn activity_panel_listed(&self, panel: ActivityPanel) -> bool {
        match panel {
            ActivityPanel::Automations => self.config.sidebar_style.automations,
            _ => true,
        }
    }

    /// The section the bar marks: the one picked, or Workspaces when that
    /// one is no longer listed.
    fn activity_panel(&self) -> ActivityPanel {
        Some(self.activity.panel)
            .filter(|panel| self.activity_panel_listed(*panel))
            .unwrap_or_default()
    }

    /// Whether the bar leads the left column, whatever the sidebar's state:
    /// on, and not set aside by zen mode. The sidebar toggle then lives in
    /// the column rather than the title bar.
    pub(crate) fn activity_bar_leads(&self) -> bool {
        self.activity_bar_shown() && !self.zen_layout()
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
        self.activity_bar_leads()
            .then(|| self.activity_panel())
            .filter(|panel| *panel != ActivityPanel::Workspaces)
    }

    /// The left column's width: the bar alone beside a section, else the
    /// sidebar as its mode lays it out, with the bar's room added whenever
    /// the bar leads, even beside a hidden sidebar.
    pub(crate) fn sidebar_column_width(&self, window_width: f32) -> Option<f32> {
        if self.activity_section().is_some() || self.mod_app().is_some() {
            return Some(BAR_WIDTH);
        }
        let width = self.sidebar_mode().width(self.sidebar_width, window_width);
        if !self.activity_bar_leads() {
            return width;
        }
        Some(width.unwrap_or(0.) + BAR_WIDTH)
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
            && self.activity_panel() == panel
            && self.sidebar_visible
            && self.mod_app().is_none()
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
        if !self.activity_panel_listed(panel) {
            return;
        }
        if panel == ActivityPanel::Workspaces && !self.sidebar_visible {
            self.toggle_sidebar();
        }
        // A section picked is where the user is now, not a place to go back to.
        self.activity.return_to = None;
        self.activity.app = None;
        if self.activity.pick(panel) {
            self.save_chrome();
        }
        self.sync_activity_section(cx);
        match self.activity_section() {
            Some(
                ActivityPanel::Overview
                | ActivityPanel::Security
                | ActivityPanel::Automations
                | ActivityPanel::Usage,
            ) => {
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
    pub(crate) fn render_activity_section(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.mod_app().is_some() {
            return self.render_mod_app(cx);
        }
        let section = match self.activity_section()? {
            ActivityPanel::Workspaces => return None,
            ActivityPanel::Overview => self.render_overview(cx),
            ActivityPanel::Security => self.render_security_section(window, cx),
            ActivityPanel::Automations => self.render_automations_section(cx),
            ActivityPanel::Usage => self.render_usage_section(cx),
            ActivityPanel::PullRequests | ActivityPanel::Issues => {
                return self.render_orchestrator_section();
            }
        };
        let focus = self.activity.section_focus.clone();
        Some(
            div()
                .row()
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
    /// Workspaces, the bar and the sidebar as its mode shows it, under a
    /// title row that holds the sidebar toggle.
    pub(crate) fn render_activity(
        &self,
        indicators: Indicators,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let column = div()
            .id("activity")
            .debug_selector(|| "activity".into())
            .row()
            .flex_none()
            .h_full()
            .min_h_0()
            .child(self.render_activity_bar(cx));
        if self.activity_section().is_some() || self.mod_app().is_some() {
            return column;
        }
        let sidebar = match self.sidebar_mode() {
            SidebarMode::Expanded => self.render_sidebar(indicators, window, cx),
            // A window too narrow for the rail keeps the bar alone, which
            // still brings the sidebar back.
            SidebarMode::Rail
                if SidebarMode::Rail
                    .width(None, f32::from(window.viewport_size().width))
                    .is_some() =>
            {
                self.render_rail(indicators, window, cx)
            }
            SidebarMode::Rail | SidebarMode::Hidden => return column,
        };
        let panel = div()
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .child(self.panel_title(ActivityPanel::Workspaces, cx))
            .child(div().row().flex_1().min_h_0().child(sidebar));
        let width = self
            .sidebar_mode()
            .width(self.sidebar_width, f32::from(window.viewport_size().width));
        match self.sidebar_slide_shown().zip(width) {
            Some((shown, width)) => column.child(crate::window::slide_start(panel, width, shown)),
            None => column.child(panel),
        }
    }

    fn render_activity_bar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let counts = self.activity_counts(cx);
        let bar = div()
            .id("activity-bar")
            .debug_selector(|| "activity-bar".into())
            // Each panel replaces the column beside the bar, as a tab does.
            .role(Role::TabList)
            .aria_label(fl!("sidebar-activity-bar"))
            .aria_orientation(Orientation::Vertical)
            .w(px(BAR_WIDTH))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .pb(px(6.))
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_e(px(1.))
            .border_color(rgb(theme.active));
        // The sections, then any mod apps, scroll in a short window so
        // Settings stays in reach under them. The top padding sits inside,
        // leaving a count bubble room above the first icon.
        let mut tabs = div()
            .id("activity-tabs")
            .debug_selector(|| "activity-tabs".into())
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .pt(px(6.))
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        let shown = self.activity_panel();
        for &panel in ActivityPanel::ALL {
            if !self.activity_panel_listed(panel) {
                continue;
            }
            let state = if panel == shown && self.mod_app().is_none() {
                BarState::Selected
            } else {
                BarState::Idle
            };
            // Overview's icon pulses while agents work, as the sidebar's
            // working dots do, and Automations' while a run goes on.
            let busy = match panel {
                ActivityPanel::Overview => self.overview_working() > 0,
                ActivityPanel::Automations => self.automations.running() > 0,
                _ => false,
            };
            let bubble = counts.of(panel).map(|count| Bubble {
                count,
                unseen: self.activity.unseen(panel, counts.latest(panel)),
            });
            let button = self
                .bar_button(
                    panel_selector(panel),
                    icon(panel),
                    label(panel),
                    (state, busy),
                    bubble,
                )
                .role(Role::Tab)
                .aria_selected(state == BarState::Selected)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.select_activity_panel(panel, window, cx);
                }));
            tabs = tabs.child(button);
        }
        bar.child(tabs.children(self.mod_activity_buttons(cx)))
            .child(
                self.bar_button(
                    "activity-settings",
                    "icons/settings.svg",
                    fl!("sidebar-activity-settings").into(),
                    (BarState::Idle, false),
                    None,
                )
                .role(Role::Button)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.command(Command::Settings, window, cx);
                })),
            )
    }

    fn bar_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: SharedString,
        (state, busy): (BarState, bool),
        bubble: Option<Bubble>,
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
                Animation::new(crate::motion::duration(BUSY_PULSE)).repeat(),
                |icon, delta| icon.opacity(0.35 + 0.65 * (delta * std::f32::consts::PI).sin()),
            )
            .into_any_element()
        } else {
            icon.into_any_element()
        };
        let bubble = bubble.filter(|bubble| bubble.count > 0);
        let glyph = div()
            .debug_selector(move || format!("{id}-glyph"))
            .relative()
            .child(icon)
            .when_some(bubble, |glyph, bubble| {
                // Held by its right edge, inside the bar's border, so a wider
                // count such as `99+` grows over the icon rather than past the
                // bar into the column beside it.
                glyph.child(
                    div()
                        .debug_selector(move || format!("{id}-bubble"))
                        .absolute()
                        .top(px(-8.))
                        .inset_e(px(BUBBLE_END))
                        .child(self.count_bubble(bubble)),
                )
            });
        let spoken = match bubble {
            Some(Bubble {
                count,
                unseen: true,
            }) => SharedString::from(fl!(
                "sidebar-activity-spoken-new",
                label = label.as_ref(),
                count = count
            )),
            Some(Bubble { count, .. }) => SharedString::from(fl!(
                "sidebar-activity-spoken",
                label = label.as_ref(),
                count = count
            )),
            None => label.clone(),
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .aria_label(spoken)
            .map(|button| self.chrome_stop(crate::window::chrome_focus::Region::Sidebar, button))
            .relative()
            .size(px(BUTTON))
            .flex_none()
            .row()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .tooltip(move |_, cx| {
                cx.new(|_| crate::menu::SettingsHint {
                    text: label.clone(),
                    foreground,
                    surface,
                })
                .into()
            })
            .when(state == BarState::Selected, |button| {
                button.bg(rgb(theme.primary_wash())).child(
                    div()
                        .absolute()
                        .inset_s(px(-6.))
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

    /// A count in the accent while it holds something unseen, else muted.
    fn count_bubble(&self, Bubble { count, unseen }: Bubble) -> Div {
        let theme = &self.theme;
        let text = if count > 99 {
            "99+".to_owned()
        } else {
            count.to_string()
        };
        let fill = if unseen { theme.primary() } else { theme.muted };
        div()
            .min_w(px(14.))
            .h(px(14.))
            .px(px(3.))
            .row()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(rgb(fill))
            .text_color(rgb(theme.text_on(fill)))
            .text_size(px(9.))
            .line_height(px(12.))
            .whitespace_nowrap()
            .child(text)
    }

    /// The section's name over the sidebar, across its width, ended by the
    /// sidebar toggle. Over the rail, too narrow for the name, the toggle
    /// stands alone.
    fn panel_title(&self, panel: ActivityPanel, cx: &mut Context<Self>) -> Div {
        let expanded = self.sidebar_visible;
        let theme = &self.theme;
        let font = &self.config.sidebar;
        div()
            .debug_selector(|| "activity-title".into())
            .flex_none()
            .h(px(self.activity_title_height()))
            .row()
            .items_center()
            .when(expanded, |title| title.ps(px(12.)).pe(px(TITLE_END)))
            .when(!expanded, |title| title.justify_center())
            .text_font(font)
            .text_size(px(font.size - 1.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(theme.subtext()))
            .bg(self.fill().panel(theme.sidebar_background()))
            .border_e(px(1.))
            .border_color(rgb(theme.active))
            .when(expanded, |title| {
                title.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(label(panel).to_uppercase()),
                )
            })
            .child(self.sidebar_toggle(cx))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BarState {
    Selected,
    Idle,
}

/// A section's count on the bar, and whether it holds anything changed since
/// the section was last open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Bubble {
    count: usize,
    unseen: bool,
}

fn panel_selector(panel: ActivityPanel) -> &'static str {
    match panel {
        ActivityPanel::Overview => "activity-overview",
        ActivityPanel::Workspaces => "activity-workspaces",
        ActivityPanel::PullRequests => "activity-pull-requests",
        ActivityPanel::Issues => "activity-issues",
        ActivityPanel::Security => "activity-security",
        ActivityPanel::Automations => "activity-automations",
        ActivityPanel::Usage => "activity-usage",
    }
}
