//! Sidebar: layout preset, selection marks, spacing and host colours. Every
//! change reaches the open windows at once; see `settings_window::sidebar`.
// i18n: migrated
use super::{SettingsWindow, on_value, ui};
use crate::{
    config::{
        AgentsPlacement, ChangeCounts, Config, DeviceFooter, LayoutMode, SelectMode,
        SidebarOverrides, preferences::Preference,
    },
    i18n::{self, fl},
    settings_window::{
        inputs::{DropdownField, NumberField},
        sidebar::SidebarIntent,
    },
    sidebar::SidebarMetrics,
};
use gpui::{prelude::*, *};
use std::ops::RangeInclusive;

/// The four spacing numbers the page edits. The old sidebar gap became the
/// terminal's left padding, on the Theme page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::settings_window) enum Spacing {
    Indent,
    RowPadding,
    RowGap,
    HostGap,
}

impl Spacing {
    pub(in crate::settings_window) const ALL: [Self; 4] =
        [Self::Indent, Self::RowPadding, Self::RowGap, Self::HostGap];

    fn row(self) -> &'static str {
        match self {
            Self::Indent => "sidebar-indent",
            Self::RowPadding => "sidebar-row-padding",
            Self::RowGap => "sidebar-row-gap",
            Self::HostGap => "sidebar-host-gap",
        }
    }

    pub(in crate::settings_window) fn control(self) -> &'static str {
        match self {
            Self::Indent => "sidebar-indent-control",
            Self::RowPadding => "sidebar-row-padding-control",
            Self::RowGap => "sidebar-row-gap-control",
            Self::HostGap => "sidebar-host-gap-control",
        }
    }

    fn reset(self) -> &'static str {
        match self {
            Self::Indent => "sidebar-indent-reset",
            Self::RowPadding => "sidebar-row-padding-reset",
            Self::RowGap => "sidebar-row-gap-reset",
            Self::HostGap => "sidebar-host-gap-reset",
        }
    }

    /// The messages titling and describing the row, the catalog's.
    pub(in crate::settings_window) fn messages(self) -> (&'static str, &'static str) {
        match self {
            Self::Indent => (
                "settings-sidebar-indent",
                "settings-sidebar-indent-description",
            ),
            Self::RowPadding => (
                "settings-sidebar-row-padding",
                "settings-sidebar-row-padding-description",
            ),
            Self::RowGap => (
                "settings-sidebar-row-gap",
                "settings-sidebar-row-gap-description",
            ),
            Self::HostGap => (
                "settings-sidebar-host-gap",
                "settings-sidebar-host-gap-description",
            ),
        }
    }

    /// The `[sidebar]` key the parser bands.
    fn band(self) -> &'static str {
        match self {
            Self::Indent => "indent",
            Self::RowPadding => "row_padding",
            Self::RowGap => "gap",
            Self::HostGap => "host_gap",
        }
    }

    pub(in crate::settings_window) fn range(self) -> RangeInclusive<f32> {
        let key = self.band();
        let max = SidebarOverrides::BANDS
            .iter()
            .find_map(|(name, max)| (*name == key).then_some(*max))
            .unwrap_or(0.);
        0.0..=max
    }

    fn slot(self, overrides: &mut SidebarOverrides) -> &mut Option<f32> {
        match self {
            Self::Indent => &mut overrides.indent,
            Self::RowPadding => &mut overrides.row_padding,
            Self::RowGap => &mut overrides.gap,
            Self::HostGap => &mut overrides.host_gap,
        }
    }

    /// The configured override, or none when the preset applies.
    fn overridden(self, config: &Config) -> Option<f32> {
        let mut overrides = config.sidebar_style.overrides;
        *self.slot(&mut overrides)
    }

    /// What the sidebar uses now: the override, else the preset's value.
    fn value(self, config: &Config) -> f32 {
        let preset = SidebarMetrics::for_mode(config.layout.mode);
        self.overridden(config).unwrap_or(match self {
            // The card layouts step worktrees by their own padding.
            Self::Indent
                if matches!(config.layout.mode, LayoutMode::Superset | LayoutMode::Orca) =>
            {
                preset.card_indent
            }
            Self::Indent => preset.child_indent,
            Self::RowPadding => preset.row_padding,
            Self::RowGap => preset.gap,
            Self::HostGap => preset.host_gap,
        })
    }
}

impl SettingsWindow {
    /// Previews or commits one spacing number; `None` removes an override.
    pub(in crate::settings_window) fn set_spacing(
        &mut self,
        spacing: Spacing,
        value: Option<f32>,
        commit: bool,
        cx: &mut Context<Self>,
    ) {
        let mut intent = SidebarIntent::of(&self.config);
        *spacing.slot(&mut intent.style.overrides) = value;
        if commit {
            self.commit_sidebar(intent, cx);
        } else {
            self.preview_sidebar(intent, cx);
        }
    }

    pub(in crate::settings_window) fn render_sidebar(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let ready = !self.quitting && self.closing.is_none();
        let mut page = div()
            .relative()
            .flex()
            .flex_col()
            .child(ui::section(
                theme,
                &fl!("settings-sidebar-section-layout"),
                None,
            ))
            .child(
                ui::row(theme, "sidebar-layout", fl!("settings-sidebar-layout"))
                    .tag(ui::Source::HerdrUi)
                    .description(fl!("settings-sidebar-layout-description"))
                    .control(self.layout_dropdown(ready, cx)),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-activity-bar",
                    fl!("settings-sidebar-activity-bar-title"),
                )
                .description(fl!("settings-sidebar-activity-bar-hint"))
                .switch(
                    ui::toggle(
                        theme,
                        "sidebar-activity-bar-toggle",
                        self.config.sidebar_style.activity_bar,
                        ready,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        let mut intent = SidebarIntent::of(&this.config);
                        intent.style.activity_bar = !intent.style.activity_bar;
                        this.commit_sidebar(intent, cx);
                    })),
                ),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-automations",
                    fl!("settings-sidebar-automations-title"),
                )
                .description(fl!("settings-sidebar-automations-hint"))
                .switch(
                    ui::toggle(
                        theme,
                        "sidebar-automations-toggle",
                        self.config.sidebar_style.automations,
                        ready,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        let mut intent = SidebarIntent::of(&this.config);
                        intent.style.automations = !intent.style.automations;
                        this.commit_sidebar(intent, cx);
                    })),
                ),
            )
            .child(
                ui::row(theme, "sidebar-select", fl!("settings-sidebar-select"))
                    .description(fl!("settings-sidebar-select-description"))
                    .control(ui::segmented(
                        theme,
                        "sidebar-select-control",
                        &[
                            (SelectMode::Row, fl!("settings-sidebar-select-row")),
                            (SelectMode::Group, fl!("settings-sidebar-select-group")),
                            (
                                SelectMode::GroupDim,
                                fl!("settings-sidebar-select-group-dim"),
                            ),
                        ],
                        Some(self.config.sidebar_style.select),
                        ready,
                        on_value(cx, |this, select, _, cx| {
                            let mut intent = SidebarIntent::of(&this.config);
                            intent.style.select = select;
                            this.commit_sidebar(intent, cx);
                        }),
                    )),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-device-footer",
                    fl!("settings-sidebar-device-footer"),
                )
                .description(fl!("settings-sidebar-device-footer-description"))
                .control(ui::segmented(
                    theme,
                    "sidebar-device-footer-control",
                    &[
                        (DeviceFooter::Always, fl!("settings-sidebar-footer-always")),
                        (DeviceFooter::Auto, fl!("settings-sidebar-footer-auto")),
                        (DeviceFooter::Never, fl!("settings-sidebar-footer-never")),
                    ],
                    Some(self.config.sidebar_style.device_footer),
                    ready,
                    on_value(cx, |this, footer, _, cx| {
                        let mut intent = SidebarIntent::of(&this.config);
                        intent.style.device_footer = footer;
                        this.commit_sidebar(intent, cx);
                    }),
                )),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-change-counts",
                    fl!("settings-sidebar-change-counts"),
                )
                .description(fl!("settings-sidebar-change-counts-description"))
                .control(ui::segmented(
                    theme,
                    "sidebar-change-counts-control",
                    &[
                        (
                            ChangeCounts::PullRequest,
                            fl!("settings-sidebar-counts-pull-request"),
                        ),
                        (ChangeCounts::Local, fl!("settings-sidebar-counts-local")),
                    ],
                    Some(self.config.sidebar_style.change_counts),
                    ready,
                    on_value(cx, |this, counts, _, cx| {
                        let mut intent = SidebarIntent::of(&this.config);
                        intent.style.change_counts = counts;
                        this.commit_sidebar(intent, cx);
                    }),
                )),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-show-agents",
                    fl!("settings-sidebar-show-agents"),
                )
                .description(fl!("settings-sidebar-show-agents-description"))
                .switch(
                    ui::toggle(
                        theme,
                        "sidebar-show-agents-toggle",
                        self.config.show_agents,
                        true,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        let show = !this.config.show_agents;
                        this.save_native(move || Config::save_show_agents(show), cx);
                    })),
                ),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-show-agents-header",
                    fl!("settings-sidebar-show-agents-header"),
                )
                .description(fl!("settings-sidebar-agents-header-hint"))
                .switch(self.preference_toggle(
                    "sidebar-show-agents-header-toggle",
                    self.config.show_agents_header,
                    Preference::ShowAgentsHeader(!self.config.show_agents_header),
                    cx,
                )),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-agents-placement",
                    fl!("settings-sidebar-agents-placement"),
                )
                .description(fl!("settings-sidebar-agents-placement-description"))
                .control(ui::segmented(
                    theme,
                    "sidebar-agents-placement-control",
                    &[
                        (
                            AgentsPlacement::Panel,
                            fl!("settings-sidebar-placement-panel"),
                        ),
                        (
                            AgentsPlacement::Nested,
                            fl!("settings-sidebar-placement-nested"),
                        ),
                    ],
                    Some(self.config.agents_placement),
                    ready && self.config.show_agents,
                    on_value(cx, |this, placement, _, cx| {
                        this.save_preference(Preference::AgentsPlacement(placement), cx);
                    }),
                )),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-show-search",
                    fl!("settings-sidebar-show-search"),
                )
                .description(fl!("settings-sidebar-show-search-description"))
                .switch(self.preference_toggle(
                    "sidebar-show-search-toggle",
                    self.config.show_sidebar_search,
                    Preference::ShowSidebarSearch(!self.config.show_sidebar_search),
                    cx,
                )),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-repository-icons",
                    fl!("settings-sidebar-repository-icons"),
                )
                .description(fl!("settings-sidebar-repository-icons-description"))
                .switch(self.preference_toggle(
                    "sidebar-repository-icons-toggle",
                    self.config.show_repository_icons,
                    Preference::ShowRepositoryIcons(!self.config.show_repository_icons),
                    cx,
                )),
            )
            .child(
                ui::row(
                    theme,
                    "sidebar-device-icons",
                    fl!("settings-sidebar-device-icons"),
                )
                .description(fl!("settings-sidebar-device-icons-description"))
                .switch(self.preference_toggle(
                    "sidebar-device-icons-toggle",
                    self.config.show_device_icons,
                    Preference::ShowDeviceIcons(!self.config.show_device_icons),
                    cx,
                )),
            )
            .child(
                ui::row(theme, "sidebar-gpu-load", fl!("settings-sidebar-gpu-load"))
                    .description(fl!("settings-sidebar-gpu-load-description"))
                    .switch(self.preference_toggle(
                        "sidebar-gpu-load-toggle",
                        self.config.show_sidebar_gpu_load,
                        Preference::ShowSidebarGpuLoad(!self.config.show_sidebar_gpu_load),
                        cx,
                    )),
            )
            .child(ui::section(
                theme,
                &fl!("settings-sidebar-section-spacing"),
                None,
            ));
        for spacing in Spacing::ALL {
            page = page.child(self.spacing_row(spacing, ready, cx));
        }
        page = page.child(
            ui::section(theme, &fl!("settings-sidebar-host-colours"), None)
                .relative()
                .debug_selector(|| "sidebar-host-colours".into())
                .child(super::super::search::track_bounds(
                    "sidebar-host-colours".into(),
                )),
        );
        let rows = self.host_rows(cx);
        if rows.is_empty() {
            page = page.child(
                div()
                    .debug_selector(|| "sidebar-host-colours-empty".into())
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(rgb(theme.muted))
                    .child(fl!("settings-sidebar-hosts-empty")),
            );
        }
        let mut tinted = false;
        for (index, (host, color, hint)) in rows.into_iter().enumerate() {
            let hint = if hint.is_empty() && !tinted {
                tinted = true;
                "settings-sidebar-host-tint"
            } else {
                hint
            };
            let hint = if hint.is_empty() {
                SharedString::default()
            } else {
                i18n::message(hint, &[]).into()
            };
            let hex = self
                .hex_edit
                .as_ref()
                .filter(|edit| edit.host == host)
                .map(|edit| (edit.input.clone(), edit.invalid));
            let (changed, editing) = (host.clone(), host.clone());
            page = page.child(ui::swatch_row(
                theme,
                &host,
                index,
                color,
                &self.config.terminal,
                hint,
                hex,
                on_value(cx, move |this, change, _, cx| {
                    let color = match change {
                        ui::HostColorChange::Set(color) => Some(color),
                        ui::HostColorChange::Clear => None,
                    };
                    this.set_host_color(&changed, color, cx);
                }),
                {
                    let view = cx.entity().downgrade();
                    move |window, cx| {
                        let _ =
                            view.update(cx, |this, cx| this.begin_hex_edit(&editing, window, cx));
                    }
                },
            ));
        }
        #[cfg(all(feature = "integration-test", target_os = "macos"))]
        let page = page.child(super::super::native::probe(
            super::super::native::SIDEBAR_PAGE,
        ));
        page
    }

    fn layout_dropdown(&self, ready: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        const FIELD: DropdownField = DropdownField::Layout;
        let mode = self.config.layout.mode;
        let options: Vec<SharedString> = LayoutMode::ALL
            .iter()
            .map(|mode| SharedString::from(mode.label()))
            .collect();
        let selected = LayoutMode::ALL
            .iter()
            .position(|candidate| *candidate == mode)
            .unwrap_or(0);
        let view = cx.entity().downgrade();
        let (opener, picker, dismisser) = (view.clone(), view.clone(), view);
        ui::dropdown(
            &self.theme,
            FIELD.selector(),
            mode.label(),
            false,
            ready,
            self.dropdown.as_ref().filter(|open| open.field == FIELD),
            move |window, cx| {
                let options = options.clone();
                let _ = opener.update(cx, |this, cx| {
                    this.open_dropdown(FIELD, options, selected, window, cx)
                });
            },
            move |index, window, cx| {
                let _ = picker.update(cx, |this, cx| {
                    this.pick_dropdown(FIELD, index, cx);
                    window.focus(&this.focus, cx);
                });
            },
            move |window, cx| {
                let _ = dismisser.update(cx, |this, cx| this.close_dropdown(window, cx));
            },
        )
    }

    fn spacing_row(&self, spacing: Spacing, ready: bool, cx: &mut Context<Self>) -> ui::Row {
        let field = NumberField::Spacing(spacing);
        let value = spacing.value(&self.config);
        let view = cx.entity().downgrade();
        let (title, description) = spacing.messages();
        ui::row(&self.theme, spacing.row(), i18n::message(title, &[]))
            .description(i18n::message(description, &[]))
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: field.selector(),
                    value,
                    range: field.range(),
                    unit: ui::Unit::Pixels,
                    look: ui::Look::Slider,
                    enabled: ready,
                },
                self.number_edit_for(field),
                on_value(cx, move |this, change, _, cx| {
                    this.commit_number(field, change, cx)
                }),
                move |window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.begin_number_edit(field, value, window, cx)
                    });
                },
            ))
            .control(self.spacing_reset(spacing, ready, cx))
    }

    /// The reset glyph, shown only while an override is set; a fixed slot
    /// keeps the numbers aligned either way.
    fn spacing_reset(&self, spacing: Spacing, ready: bool, cx: &mut Context<Self>) -> Div {
        let slot = div().w(px(16.)).flex_none();
        if spacing.overridden(&self.config).is_none() {
            return slot;
        }
        let id = spacing.reset();
        slot.child(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .text_color(rgb(self.theme.muted))
                .when(ready, |reset| {
                    reset.cursor_pointer().on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.set_spacing(spacing, None, true, cx)
                        }),
                    )
                })
                .child("\u{27f2}"),
        )
    }
}
