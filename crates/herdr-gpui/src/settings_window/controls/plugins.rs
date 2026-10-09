//! Settings > Plugins: the values hosts' plugins report, a switch per value
//! that edits the shared `[ui.sidebar]` rows, and a preview of those rows.
use super::*;
use crate::{
    config::{SidebarScope, Theme, mix},
    fonts::StyledFont,
    plugin_values::{
        HostReport, PreviewPart, PreviewRole, PreviewSource, SidebarValue, preview_agent,
        preview_space, sidebar_values,
    },
    search_input::{Changed, SearchInput},
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

const CONFIG_NOTE: &str = "A shown value becomes its own row in [ui.sidebar] of your Herdr \
config, so the terminal app shows it too.";

/// What Herdr's endpoint cannot do yet, so the section never implies otherwise.
const LIMITS: &str = "Herdr doesn't yet let this app list, enable, or disable plugins or read \
their logs; use `herdr plugin` on the host. Nothing is installed from here.";

const STATUS: &str = "state_text";

pub(in crate::settings_window) struct Plugins {
    search: Entity<SearchInput>,
    /// Records sidebar edits instead of writing the shared config.
    #[cfg(test)]
    edits: Option<Vec<Edit>>,
    _search_changed: Subscription,
}

impl Plugins {
    pub(in crate::settings_window) fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let search = cx.new(SearchInput::new);
        search.update(cx, |input, cx| {
            input.set_placeholder("Search plugin values...", cx)
        });
        let changed = cx.subscribe(&search, |this, _, _: &Changed, cx| {
            this.body_scroll.set_offset(Point::default());
            cx.notify();
        });
        Self {
            search,
            #[cfg(test)]
            edits: None,
            _search_changed: changed,
        }
    }

    pub(super) fn refresh_appearance(&self, config: &Config, theme: &Theme, cx: &mut App) {
        self.search.update(cx, |input, cx| {
            input.set_appearance(config.ui.clone(), theme.clone(), cx);
        });
    }
}

fn scope_label(scope: SidebarScope) -> &'static str {
    match scope {
        SidebarScope::Agents => "Agents",
        SidebarScope::Spaces => "Workspaces",
    }
}

fn detail(value: &SidebarValue) -> String {
    match &value.sample {
        Some(sample) => format!(
            "{} \u{b7} \u{201c}{sample}\u{201d} \u{b7} {}",
            scope_label(value.scope),
            value.hosts.join(", ")
        ),
        None => format!("{} \u{b7} not reported", scope_label(value.scope)),
    }
}

impl SettingsWindow {
    pub(in crate::settings_window) fn render_plugin_controls(&self, cx: &mut Context<Self>) -> Div {
        let query = self.plugins.search.read(cx).text().trim().to_lowercase();
        let body = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .min_w_0()
            .child(self.plugins.search.clone());
        let Some(source) = self.source.upgrade() else {
            return body.child(self.control_note(
                "Open a session window to see what your plugins report. Local preferences remain available.",
            ));
        };
        let reports = source.read(cx).plugin_reports();
        body.child(self.render_sidebar_values(&reports, &query, cx))
            .child(self.control_note(LIMITS))
    }

    fn render_sidebar_values(
        &self,
        reports: &[HostReport],
        query: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let ready = self.controls_shared_ready();
        let layout = &self.config.sidebar_layout;
        let values = sidebar_values(reports, layout);
        let label_sample = reports
            .iter()
            .find_map(|report| report.label_sample.as_deref());
        let mut card = self
            .control_card("Sidebar values")
            .child(self.control_note(CONFIG_NOTE));
        if !ready {
            card = card.child(self.control_note(if !cfg!(unix) {
                "Shared Herdr settings are read-only on this platform."
            } else {
                "Shared settings are unavailable. Reload from General to retry."
            }));
        }
        card = card.child(self.rows_mode(cx));
        // Both columns shrink, so a wide navigation never pushes them past
        // the card; they wrap once neither keeps its preferred width.
        let mut list = div()
            .debug_selector(|| "plugin-values".into())
            .flex()
            .flex_col()
            .gap(px(2.))
            .flex_grow(1.)
            .flex_shrink(1.)
            .flex_basis(px(240.))
            .min_w_0();
        if query.is_empty() || "status labels".contains(query) {
            let labelled: usize = reports.iter().map(|report| report.labelled_agents).sum();
            let detail = match label_sample {
                Some(sample) => format!(
                    "Agents \u{b7} {labelled} name their own status, such as \u{201c}{sample}\u{201d}"
                ),
                None => "Agents".to_owned(),
            };
            let title = div()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Status labels");
            list = list.child(self.value_switch(
                SidebarScope::Agents,
                STATUS,
                title,
                detail,
                layout.shows(SidebarScope::Agents, STATUS),
                ready,
                cx,
            ));
        }
        let mut matched = 0;
        for value in values.iter().filter(|value| value.matches(query)) {
            matched += 1;
            let title = div()
                .text_font(&self.config.terminal)
                .text_size(px(self.config.ui.size))
                .font_weight(FontWeight::SEMIBOLD)
                .child(value.token());
            list = list.child(self.value_switch(
                value.scope,
                &value.token(),
                title,
                detail(value),
                value.shown,
                ready,
                cx,
            ));
        }
        if matched == 0 {
            list = list.child(self.control_note(if values.is_empty() {
                "No plugin or hook reports custom values on a connected host yet."
            } else {
                "No matching values."
            }));
        }
        let source = PreviewSource {
            values: &values,
            label_sample,
            machine: (reports.len() > 1).then(|| reports[0].host.as_str()),
        };
        card.child(
            div()
                .flex()
                .flex_wrap()
                .items_start()
                .gap(px(20.))
                .child(list)
                .child(self.sidebar_preview(&source)),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn value_switch(
        &self,
        scope: SidebarScope,
        token: &str,
        title: Div,
        detail: String,
        shown: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let id = format!("plugin-value-{}-{token}", scope.key());
        let selector = id.clone();
        let row = div()
            .id(ElementId::Name(id.into()))
            .debug_selector(move || selector)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .py(px(4.))
            .min_w_0()
            .when(ready, |row| row.cursor_pointer())
            .when(!ready, |row| row.opacity(0.5))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(title)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(self.theme.muted))
                            .child(detail),
                    ),
            )
            .child(crate::toggles::switch(&self.theme, 22., shown));
        if !ready {
            return row;
        }
        let token = token.to_owned();
        row.on_click(cx.listener(move |this, _, _, cx| {
            let edit = Edit::SidebarToken {
                scope,
                token: token.clone(),
                shown: !shown,
            };
            // As in `save_shared`, a click during a load or save is dropped.
            if this.busy() {
                return;
            }
            #[cfg(test)]
            if let Some(edits) = &mut this.plugins.edits {
                edits.push(edit);
                return;
            }
            this.save_shared(edit, cx);
        }))
    }

    /// Whether this app draws Herdr's configured rows at all, and what the
    /// first switch changes while the rows are still Herdr's defaults.
    fn rows_mode(&self, cx: &mut Context<Self>) -> Div {
        let inline = self.config.usage.inline;
        if !inline {
            return div()
                .debug_selector(|| "plugin-rows-native".into())
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(self.control_note(
                    "This app draws its own sidebar rows ([usage] inline = false), so these switches only change the terminal app.",
                ))
                .child(self.preference_switch(
                    "plugin-use-herdr-rows",
                    "Use Herdr's sidebar rows in this app",
                    false,
                    crate::config::preferences::Preference::UsageInline(true),
                    cx,
                ));
        }
        let layout = &self.config.sidebar_layout;
        if layout.agents == crate::config::AgentLayout::default()
            && layout.spaces == crate::config::SpaceLayout::default()
        {
            return div().debug_selector(|| "plugin-rows-default".into()).child(self.control_note(
                "The sidebar uses this app's own rows while Herdr's are the defaults. Showing a value switches it to Herdr's configured rows, in your row style; the preview shows how they look.",
            ));
        }
        div()
    }

    /// An example workspace and agent drawn with the configured rows, so a
    /// switch's effect shows here once the edit is saved and reloaded.
    fn sidebar_preview(&self, source: &PreviewSource<'_>) -> Div {
        let theme = &self.theme;
        let layout = &self.config.sidebar_layout;
        let font = &self.config.sidebar;
        let block = |rows: Vec<Vec<PreviewPart>>, selected: bool| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .p(px(8.))
                .min_w_0()
                .rounded(px(corners::CONTROL))
                .when(selected, |block| block.bg(rgb(theme.active)))
                .children(rows.into_iter().map(|row| self.preview_row(row)))
        };
        div()
            .debug_selector(|| "plugin-preview".into())
            .flex()
            .flex_col()
            .gap(px(8.))
            .flex_shrink(1.)
            .flex_basis(px(240.))
            .max_w(px(240.))
            .min_w_0()
            .p(px(10.))
            .rounded(px(corners::PANEL))
            .bg(rgb(mix(theme.surface, theme.background, 25)))
            .border_1()
            .border_color(rgb(theme.active))
            .text_font(font)
            .text_size(px(font.size))
            .child(div().text_color(rgb(theme.muted)).child("Preview"))
            .child(block(preview_space(&layout.spaces, source), false))
            .child(block(preview_agent(&layout.agents, source), true))
    }

    fn preview_row(&self, row: Vec<PreviewPart>) -> Div {
        let theme = &self.theme;
        let mut line = div().flex().items_center().gap(px(6.)).min_w_0();
        for part in row {
            line = line.child(match part {
                PreviewPart::StateIcon => div()
                    .flex_none()
                    .size(px(8.))
                    .rounded_full()
                    .bg(rgb(theme.ink(theme.palette[3]))),
                PreviewPart::Text(text, role, style) => {
                    let base = match role {
                        PreviewRole::Name => (theme.foreground, FontWeight::NORMAL),
                        PreviewRole::Detail => (theme.subtext(), FontWeight::NORMAL),
                        PreviewRole::Plugin => (theme.muted, FontWeight::NORMAL),
                    };
                    let (color, weight) = crate::sidebar::styled_token(base, style, theme);
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(color))
                        .font_weight(weight)
                        .child(text)
                }
            });
        }
        line
    }
}
