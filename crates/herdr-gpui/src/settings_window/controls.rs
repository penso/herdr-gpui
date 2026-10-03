//! Prepared controls for the standalone window; persistence belongs to its serial save path.
mod fonts;
mod preferences;

use super::{Section, SettingsWindow};
use crate::{
    agent_skill::{AgentSkill, Choice},
    config::{Config, FONT_SIZE_RANGE, FontFace, LayoutMode, corners},
    font_picker::FontTarget,
    herdr_settings::{Edit, IndicatorStyle, TabBarPosition, ToastDelivery},
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};

const FACES: [(FontFace, &str); 4] = [
    (FontFace::Terminal, "Terminal"),
    (FontFace::Sidebar, "Sidebar"),
    (FontFace::Tabs, "Tabs"),
    (FontFace::Ui, "Interface"),
];

pub(super) struct Controls {
    #[cfg(test)]
    preference_io: Option<preferences::PreferenceIo>,
    sidebar_preview: crate::sidebar::preview::Preview,
    search: Entity<SearchInput>,
    integration_search: Entity<SearchInput>,
    _integration_changed: Subscription,
    picker: Option<FontTarget>,
    active_face: FontFace,
    selected: usize,
    #[cfg(test)]
    family_io: Option<fonts::FamilyIo>,
    names: Vec<String>,
    filtered: Vec<usize>,
    scroll: UniformListScrollHandle,
    discovering: bool,
    initialized: bool,
    pending_sizes: Vec<(FontFace, f32)>,
    saving_sizes: Vec<(FontFace, f32)>,
    size_editor: Option<SizeEditor>,
    local_path: String,
    _search_changed: Subscription,
}

struct SizeEditor {
    face: FontFace,
    input: Entity<SearchInput>,
    invalid: bool,
    _blur: Subscription,
}

fn parse_size(text: &str) -> Option<f32> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = f32::from(text.parse::<u8>().ok()?);
    FONT_SIZE_RANGE.contains(&value).then_some(value)
}

impl Controls {
    pub(super) fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let search = cx.new(SearchInput::new);
        search.update(cx, |input, cx| {
            input.set_placeholder("Search installed fonts...", cx)
        });
        let subscription = cx.subscribe(&search, |this, search, _: &Changed, cx| {
            this.controls.filtered = filter_fonts(&this.controls.names, search.read(cx).text());
            this.controls.selected = 0;
            this.controls.scroll.scroll_to_item(0, ScrollStrategy::Top);
            cx.notify();
        });
        let integration_search = cx.new(SearchInput::new);
        integration_search.update(cx, |input, cx| {
            input.set_placeholder("Search integrations by name or ID...", cx)
        });
        let integration_changed = cx.subscribe(&integration_search, |this, _, _: &Changed, cx| {
            this.body_scroll.set_offset(Point::default());
            cx.notify();
        });
        Self {
            #[cfg(test)]
            preference_io: None,
            integration_search,
            _integration_changed: integration_changed,
            sidebar_preview: Default::default(),
            search,
            picker: None,
            active_face: FontFace::Terminal,
            selected: 0,
            #[cfg(test)]
            family_io: None,
            names: Vec::new(),
            filtered: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            discovering: true,
            initialized: false,
            pending_sizes: Vec::new(),
            saving_sizes: Vec::new(),
            size_editor: None,
            local_path: Config::local_path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| format!("Unavailable ({error})")),
            _search_changed: subscription,
        }
    }

    fn size(&self, face: FontFace, config: &Config) -> f32 {
        self.pending_sizes
            .iter()
            .chain(&self.saving_sizes)
            .find_map(|(candidate, size)| (*candidate == face).then_some(*size))
            .unwrap_or_else(|| face.size(config))
    }
}

fn filter_fonts(names: &[String], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| name.to_lowercase().contains(&query).then_some(index))
        .collect()
}

fn stepped_size(size: f32, step: f32) -> Option<f32> {
    if !size.is_finite() || !step.is_finite() {
        return None;
    }
    Some((size.round() + step.round()).clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end()))
}

fn queue_size(pending: &mut Vec<(FontFace, f32)>, face: FontFace, size: f32) {
    if let Some((_, desired)) = pending.iter_mut().find(|(candidate, _)| *candidate == face) {
        *desired = size;
    } else {
        pending.push((face, size));
    }
}

impl SettingsWindow {
    pub(super) fn initialize_controls(&mut self, cx: &mut Context<Self>) {
        self.sync_controls(cx);
        if self.controls.initialized {
            return;
        }
        self.controls.initialized = true;
        let text_system = cx.text_system().clone();
        let discovery = cx.background_executor().spawn(async move {
            let mut names: Vec<_> = text_system
                .all_font_names()
                .into_iter()
                .filter(|name| !name.trim().is_empty())
                .collect();
            names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
            names.dedup();
            names
        });
        cx.spawn(async move |this, cx| {
            let names = discovery.await;
            let _ = this.update(cx, |this, cx| {
                this.controls.filtered = filter_fonts(&names, this.controls.search.read(cx).text());
                this.controls.names = names;
                this.controls.discovering = false;
                cx.notify();
            });
        })
        .detach();
    }

    /// Called after a root load/save completes, with its busy flag already cleared.
    pub(super) fn sync_controls(&mut self, cx: &mut Context<Self>) {
        self.refresh_control_appearance(cx);
        if !self.busy() {
            self.controls.saving_sizes.clear();
            self.flush_control_sizes(cx);
        }
    }

    pub(super) fn refresh_control_appearance(&mut self, cx: &mut Context<Self>) {
        self.controls.integration_search.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        self.controls.search.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        self.plugins
            .refresh_appearance(&self.config, &self.theme, cx);
        if let Some(editor) = &self.controls.size_editor {
            editor.input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            });
        }
    }

    fn flush_control_sizes(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.controls.pending_sizes.is_empty() {
            return;
        }
        // Only touched faces are written. A reload cannot erase later clicks, and
        // we never persist a stale copy of the other faces' configuration.
        let sizes = std::mem::take(&mut self.controls.pending_sizes);
        self.controls.saving_sizes = sizes.clone();
        self.save_control_sizes(sizes, cx);
    }

    pub(super) fn take_pending_control_sizes(&mut self) -> Vec<(FontFace, f32)> {
        std::mem::take(&mut self.controls.pending_sizes)
    }

    fn step_control_size(&mut self, face: FontFace, step: f32, cx: &mut Context<Self>) {
        self.controls.active_face = face;
        cx.notify();
        if self.quitting {
            return;
        }
        let current = self.controls.size(face, &self.config);
        if let Some(size) = stepped_size(current, step)
            && size != current
        {
            self.accept_control_size(face, size, cx);
        }
    }

    pub(super) fn accept_control_size(
        &mut self,
        face: FontFace,
        size: f32,
        cx: &mut Context<Self>,
    ) {
        self.controls.active_face = face;
        if self.quitting || !FONT_SIZE_RANGE.contains(&size) || size.fract() != 0. {
            return;
        }
        if self.controls.size(face, &self.config) == size {
            return;
        }
        queue_size(&mut self.controls.pending_sizes, face, size);
        self.flush_control_sizes(cx);
        cx.notify();
    }

    fn begin_control_size_edit(
        &mut self,
        face: FontFace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.controls.active_face = face;
        if self.quitting {
            return;
        }
        if !self.finish_control_size_edit(true, cx) {
            self.finish_control_size_edit(false, cx);
        }
        let size = self.controls.size(face, &self.config);
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_text_selected(&size.to_string(), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        let focus = input.read(cx).focus.clone();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            if !this.finish_control_size_edit(true, cx) {
                this.finish_control_size_edit(false, cx);
            }
        });
        self.controls.size_editor = Some(SizeEditor {
            face,
            input,
            invalid: false,
            _blur: blur,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn finish_control_size_edit(&mut self, save: bool, cx: &mut Context<Self>) -> bool {
        let Some(editor) = &mut self.controls.size_editor else {
            return true;
        };
        let size = if save {
            if editor.input.read(cx).is_composing() {
                return false;
            }
            let Some(size) = parse_size(editor.input.read(cx).text()) else {
                editor.invalid = true;
                cx.notify();
                return false;
            };
            Some(size)
        } else {
            None
        };
        let face = editor.face;
        self.controls.size_editor = None;
        if let Some(size) = size {
            self.accept_control_size(face, size, cx);
        }
        cx.notify();
        true
    }

    fn control_size_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event.keystroke.key.as_str(), "enter" | "escape") {
            return;
        }
        let Some(editor) = &self.controls.size_editor else {
            return;
        };
        cx.stop_propagation();
        if editor.input.read(cx).is_composing() {
            return;
        }
        window.prevent_default();
        if self.finish_control_size_edit(event.keystroke.key == "enter", cx) {
            window.focus(&self.focus, cx);
        }
    }

    pub(super) fn control_card(&self, title: impl Into<SharedString>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .min_w_0()
            .p(px(24.))
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(self.theme.active))
            .bg(rgb(self.theme.surface))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title.into()))
    }

    pub(super) fn control_note(&self, text: impl Into<SharedString>) -> Div {
        div()
            .min_w_0()
            .text_color(rgb(self.theme.muted))
            .child(text.into())
    }

    fn control_row(&self, label: &'static str, value: impl Into<SharedString>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(div().text_color(rgb(self.theme.muted)).child(label))
            .child(div().min_w_0().child(value.into()))
    }

    pub(super) fn control_choice(
        &self,
        id: impl Into<SharedString>,
        label: impl IntoElement,
        selected: bool,
        enabled: bool,
    ) -> Stateful<Div> {
        div()
            .id(id.into())
            .px(px(14.))
            .py(px(10.))
            .min_w_0()
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(if selected {
                crate::menu::accent(&self.theme)
            } else {
                rgb(self.theme.active)
            })
            .bg(rgb(if selected {
                self.theme.active
            } else {
                self.theme.background
            }))
            .when(enabled, |item| {
                item.cursor_pointer()
                    .hover(|style| style.bg(rgb(self.theme.active)))
            })
            .when(!enabled, |item| item.opacity(0.5))
            .child(label)
    }

    pub(super) fn controls_shared_ready(&self) -> bool {
        cfg!(unix) && self.shared.is_some() && !self.busy() && self.error.is_none()
    }

    pub(super) fn render_controls(&self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let content = match self.section {
            Section::Fonts => self.render_font_controls(cx),
            Section::Indicators => self.render_indicator_controls(cx),
            Section::Sound => self.render_sound_controls(cx),
            Section::Notifications => self.render_notification_controls(cx),
            Section::General => self.render_general_controls(cx),
            Section::Appearance | Section::Integrations | Section::Plugins => div(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .min_w_0()
            .when(
                !cfg!(unix)
                    && matches!(
                        self.section,
                        Section::Indicators | Section::Sound | Section::Notifications
                    ),
                |body| {
                    body.child(
                        self.control_note("Shared Herdr settings are read-only on this platform."),
                    )
                },
            )
            .when(
                self.shared.is_none()
                    && matches!(
                        self.section,
                        Section::Indicators | Section::Sound | Section::Notifications
                    ),
                |body| {
                    body.child(self.control_note(
                        "Shared settings are unavailable. Reload from General to retry.",
                    ))
                },
            )
            .child(content)
    }

    fn render_indicator_controls(&self, cx: &mut Context<Self>) -> Div {
        use herdr_client::protocol::AgentStatus;
        let mut card = self.control_card("Agent status indicators");
        let ready = self.controls_shared_ready();
        let light = matches!(
            cx.window_appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        for (label, style) in [
            ("Dots", IndicatorStyle::Dots),
            ("Symbols", IndicatorStyle::Symbols),
        ] {
            let selected = self
                .shared
                .as_ref()
                .is_some_and(|shared| shared.indicators == style);
            let mut choice = self
                .control_choice(
                    format!("settings-indicators-{label}"),
                    label,
                    selected,
                    ready,
                )
                .flex()
                .flex_col()
                .gap(px(18.))
                .when(ready, |choice| {
                    choice.on_click(cx.listener(move |this, _, _, cx| {
                        this.save_shared(Edit::Indicators(style), cx)
                    }))
                });
            if let Some(shared) = &self.shared {
                let mut preview = div().flex().flex_wrap().gap(px(20.));
                for (status, name, symbol) in [
                    (AgentStatus::Working, "Working", "\u{25d0}"),
                    (AgentStatus::Blocked, "Blocked", "\u{d7}"),
                    (AgentStatus::Done, "Done", "\u{2713}"),
                    (AgentStatus::Idle, "Idle", "\u{25cb}"),
                    (AgentStatus::Unknown, "Unknown", "\u{b7}"),
                ] {
                    let color = rgb(self.theme.ink(shared.status_color(status, light)));
                    let mark = div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(self.config.ui.size))
                        .h(px(self.config.ui.line_height()))
                        .when(style == IndicatorStyle::Symbols, |mark| {
                            mark.text_color(color).child(symbol)
                        })
                        .when(style == IndicatorStyle::Dots, |mark| {
                            mark.child(
                                div()
                                    .size(px(if status == AgentStatus::Unknown {
                                        3.
                                    } else {
                                        7.
                                    }))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(color)
                                    .when(status != AgentStatus::Idle, |dot| dot.bg(color)),
                            )
                        });
                    preview = preview.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(mark)
                            .child(name),
                    );
                }
                choice = choice.child(preview);
            }
            card = card.child(choice);
        }
        card
    }

    fn render_sound_controls(&self, cx: &mut Context<Self>) -> Div {
        let ready = self.controls_shared_ready();
        let enabled = self
            .shared
            .as_ref()
            .is_some_and(|shared| shared.sound_enabled);
        let choices = self
            .control_switch("settings-sound", "Play agent sounds", enabled, ready)
            .when(ready, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    this.save_shared(Edit::Sound(!enabled), cx);
                }))
            });
        let source_alive = self.source.upgrade().is_some();
        self.control_card("Agent sounds").child(choices)
            .child(self.control_note("Uses shared sound paths and per-agent overrides. Missing custom sounds fall back to bundled sounds."))
            .child(self.control_choice("settings-sound-preview", "Play test sound", false, source_alive)
                .when(source_alive, |button| button.on_click(cx.listener(|this, _, _, cx| {
                    if let Some(source) = this.source.upgrade() {
                        source.read(cx).sound.preview();
                    }
                }))))
            .child(self.control_note(if source_alive { "Preview plays only when requested, even when sounds are off." } else { "Open a main Herdr window to preview audio." }))
    }

    fn render_notification_controls(&self, cx: &mut Context<Self>) -> Div {
        let ready = self.controls_shared_ready();
        let mut delivery = self.control_card("Notification delivery");
        for (label, mode, note) in [
            ("Off", ToastDelivery::Off, "Disable shared notifications"),
            ("Herdr", ToastDelivery::Herdr, "In-app notifications"),
            (
                "Terminal",
                ToastDelivery::Terminal,
                "Other clients only; not delivered by this GUI",
            ),
            (
                "System",
                ToastDelivery::System,
                "OS notifications; clicking one opens its pane",
            ),
        ] {
            let selected = self
                .shared
                .as_ref()
                .is_some_and(|shared| shared.toast_delivery == mode);
            delivery = delivery.child(
                self.control_choice(
                    format!("settings-delivery-{label}"),
                    div()
                        .debug_selector(move || format!("delivery-label-{label}"))
                        .w(px(self.config.ui.size * 5.))
                        .flex_none()
                        .child(label),
                    selected,
                    ready,
                )
                .debug_selector(move || format!("settings-delivery-{label}"))
                .flex()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .debug_selector(move || format!("delivery-radio-{label}"))
                        .size(px(14.))
                        .flex_none()
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(self.theme.muted))
                        .when(selected, |radio| radio.bg(crate::menu::accent(&self.theme))),
                )
                .child(
                    self.control_note(note)
                        .flex_1()
                        .debug_selector(move || format!("delivery-note-{label}")),
                )
                .when(ready, |button| {
                    button.on_click(
                        cx.listener(move |this, _, _, cx| this.save_shared(Edit::Toasts(mode), cx)),
                    )
                }),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(delivery)
            .child(self.native_notification_controls(cx))
    }

    fn save_skill(&mut self, choice: Choice, cx: &mut Context<Self>) {
        self.save_skill_with(
            choice,
            move || {
                let home = crate::config::home()?;
                match choice {
                    Choice::Installed => {
                        let text =
                            crate::agent_skill::text(std::env::current_exe().ok().as_deref());
                        crate::agent_skill::install(&home, &text)?;
                    }
                    Choice::Declined => {
                        crate::agent_skill::remove(&home)?;
                    }
                }
                Ok(())
            },
            Self::loader(cx),
            cx,
        );
    }

    fn save_skill_with(
        &mut self,
        choice: Choice,
        operation: impl FnOnce() -> crate::Result<()> + Send + 'static,
        load: impl FnOnce() -> crate::Result<super::Loaded> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_with_completion(
            operation,
            load,
            false,
            move |cx| AgentSkill::choose(choice, cx),
            cx,
        );
    }

    fn render_skill_controls(&self, cx: &mut Context<Self>) -> Div {
        let installed = AgentSkill::choice(cx) == Some(Choice::Installed);
        let (id, label, choice) = if installed {
            (
                "settings-remove-browser-skill",
                "Remove browser skill",
                Choice::Declined,
            )
        } else {
            (
                "settings-install-browser-skill",
                "Install browser skill",
                Choice::Installed,
            )
        };
        self.control_card("Browser skill")
            .child(self.control_row("Status", if installed { "Installed, kept up to date" } else { "Not installed" }))
            .child(self.control_note("Teaches local agents to use browser tabs and page notes. Installs in existing ~/.claude and ~/.agents directories; removal deletes only app-managed copies."))
            .child(self.control_choice(id, label, false, !self.busy())
                .debug_selector(move || id.into())
                .when(!self.busy(), |button| button.on_click(cx.listener(move |this, _, _, cx| this.save_skill(choice, cx)))))
    }

    fn render_general_controls(&self, cx: &mut Context<Self>) -> Div {
        let ready = !self.busy();
        let general = self
            .control_card("Interface")
            .child(
                self.control_switch(
                    "settings-usage",
                    "Show usage",
                    self.config.usage.show,
                    ready,
                )
                .when(ready, |button| {
                    button.on_click(cx.listener(|this, _, _, cx| {
                        let show = !this.config.usage.show;
                        this.save_native(move || Config::save_usage_visibility(show), cx);
                    }))
                }),
            )
            .child(self.preference_switch(
                "settings-system-load",
                "Show CPU and memory",
                self.config.show_system_load,
                crate::config::preferences::Preference::ShowSystemLoad(
                    !self.config.show_system_load,
                ),
                cx,
            ))
            .child(self.preference_switch(
                "settings-confirm-close",
                "Confirm tab close",
                self.config.confirm_close_tab,
                crate::config::preferences::Preference::ConfirmClose(
                    !self.config.confirm_close_tab,
                ),
                cx,
            ));
        div().flex().flex_col().gap(px(24.)).child(general)
            .child(self.render_tab_bar_controls(cx))
            .child(self.render_skill_controls(cx))
            .child(self.clipboard_controls(cx))
            .child(self.control_card("Configuration")
                .child(self.control_note("GUI local overrides"))
                .child(div().min_w_0().child(self.controls.local_path.clone()))
                .child(self.control_note("Shared Herdr configuration"))
                .child(div().min_w_0().child(self.shared.as_ref().map(|shared| shared.path.display().to_string()).unwrap_or_else(|| "Unavailable".into())))
                .child(self.control_choice("settings-reload", "Reload configuration", false, ready)
                    .when(ready, |button| button.on_click(cx.listener(|this, _, _, cx| {
                        this.reload(cx);
                    }))))
                .child(self.control_note("Saved file edits reload automatically. Reloading GUI settings does not reload the daemon.")))
    }

    /// Herdr's shared tab row and selection settings, which this GUI and
    /// the TUI both follow.
    fn render_tab_bar_controls(&self, cx: &mut Context<Self>) -> Div {
        let ready = self.controls_shared_ready();
        let shared = self.shared.as_ref();
        let position = shared.map(|shared| shared.tab_bar_position);
        let hide = shared.is_some_and(|shared| shared.hide_tab_bar_when_single_tab);
        let copy = shared.is_none_or(|shared| shared.copy_on_select);
        let mut positions = div().flex().flex_wrap().gap(px(8.));
        for (id, label, choice) in [
            ("settings-tab-bar-top", "Top", TabBarPosition::Top),
            ("settings-tab-bar-bottom", "Bottom", TabBarPosition::Bottom),
        ] {
            positions = positions.child(
                self.control_choice(id, label, position == Some(choice), ready)
                    .debug_selector(move || id.into())
                    .when(ready && position != Some(choice), |button| {
                        button.on_click(cx.listener(move |this, _, _, cx| {
                            this.save_shared(Edit::TabBarPosition(choice), cx);
                        }))
                    }),
            );
        }
        self.control_card("Tabs and selection")
            .child(self.control_note("Tab bar position"))
            .child(positions)
            .child(
                self.control_switch(
                    "settings-hide-single-tab-bar",
                    "Hide tab bar with one tab",
                    hide,
                    ready,
                )
                .when(ready, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.save_shared(Edit::HideSingleTabBar(!hide), cx);
                    }))
                }),
            )
            .child(
                self.control_switch("settings-copy-on-select", "Copy on select", copy, ready)
                    .when(ready, |button| {
                        button.on_click(cx.listener(move |this, _, _, cx| {
                            this.save_shared(Edit::CopyOnSelect(!copy), cx);
                        }))
                    }),
            )
            .child(self.control_note(if cfg!(unix) {
                "Shared with Herdr. With copy on select off, Cmd-C or Ctrl-C copies the highlighted selection."
            } else {
                "Shared with Herdr and read-only on this platform. With copy on select off, Ctrl-C copies the highlighted selection."
            }))
    }

    pub(super) fn render_sidebar_layout_controls(&self, cx: &mut Context<Self>) -> Div {
        let ready = !self.quitting && self.closing.is_none();
        let mode = self.config.layout.mode;
        let mut layouts = div().flex().flex_col().gap(px(4.));
        for mode in LayoutMode::ALL {
            layouts = layouts.child(
                self.control_choice(
                    format!("settings-layout-{}", mode.name()),
                    mode.label(),
                    self.config.layout.mode == mode,
                    ready,
                )
                .debug_selector(move || format!("settings-layout-{}", mode.name()))
                .py(px(5.))
                .when(ready, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.accept_layout_choice(mode, cx);
                    }))
                }),
            );
        }
        let widths = div().flex().gap_1().children(
            crate::sidebar::preview::Preview::WIDTHS
                .into_iter()
                .map(|width| {
                    self.control_choice(
                        format!("preview-width-{width}"),
                        width.to_string(),
                        self.controls.sidebar_preview.width() == width,
                        true,
                    )
                    .debug_selector(move || format!("preview-width-{width}"))
                    .px_2()
                    .py_1()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.controls.sidebar_preview.set_width(width);
                        cx.notify();
                    }))
                }),
        );
        let mut font = self.config.sidebar.clone();
        font.size = self.controls.size(FontFace::Sidebar, &self.config);
        let light = matches!(
            cx.window_appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let preview = self.controls.sidebar_preview.render(
            mode,
            &font,
            &self.theme,
            crate::sidebar::Indicators::new(self.shared.as_ref(), light, &self.theme),
            cx.listener(|this, _, _, cx| {
                this.controls.sidebar_preview.toggle_fold();
                cx.notify();
            }),
            |target| {
                Box::new(cx.listener(move |this, _, _, cx| {
                    this.controls.sidebar_preview.select(target);
                    cx.notify();
                }))
            },
        );
        let chooser = div()
            .flex()
            .flex_wrap()
            .gap_4()
            .child(
                div()
                    .w(px(174.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(self.control_note("Choose a layout"))
                    .child(layouts)
                    .child(self.control_note("Preview width (px)"))
                    .child(widths),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(mode.label())
                    .child(preview),
            );
        self.control_card("Sidebar layout")
            .mt(px(24.))
            .relative()
            .debug_selector(|| "settings-sidebar-layout".into())
            .map(|card| {
                #[cfg(all(feature = "integration-test", target_os = "macos"))]
                let card = card.child(super::native::probe(super::native::SIDEBAR_LAYOUT));
                card
            })
            .child(chooser)
            .child(
                self.control_switch(
                    "settings-show-agents",
                    "Show agents",
                    self.config.show_agents,
                    !self.busy(),
                )
                .debug_selector(|| "settings-show-agents".into())
                .when(!self.busy(), |button| {
                    button.on_click(cx.listener(|this, _, _, cx| {
                        let show = !this.config.show_agents;
                        this.save_native(move || Config::save_show_agents(show), cx);
                    }))
                }),
            )
            .child(self.sidebar_gap_control(cx))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    pub(super) fn skill_fixture(
        window: &mut Window,
        cx: &mut Context<SettingsWindow>,
    ) -> SettingsWindow {
        let source = cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx));
        let mut view = SettingsWindow::new(source.downgrade(), cx);
        view.section = Section::General;
        view
    }

    pub(super) fn skill_load() -> crate::Result<super::super::Loaded> {
        Ok(super::super::Loaded {
            config: Config::default(),
            theme: Default::default(),
            shared: None,
            error: None,
        })
    }

    #[gpui::test]
    fn notification_delivery_columns_align_without_overflow(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(skill_fixture);
        for font_size in [12., 24., 48.] {
            for width in [680., 960.] {
                cx.simulate_resize(size(px(width), px(2200.)));
                view.update(cx, |view, cx| {
                    view.section = Section::Notifications;
                    view.config.ui.size = font_size;
                    cx.notify();
                });
                cx.update(|window, cx| {
                    crate::sidebar::layout_tests::full_draw(window, cx).clear(cx)
                });
                let body = cx.debug_bounds("settings-body").unwrap();
                let mut columns = None;
                let mut previous_bottom = body.top();
                for selectors in [
                    [
                        "settings-delivery-Off",
                        "delivery-label-Off",
                        "delivery-radio-Off",
                        "delivery-note-Off",
                    ],
                    [
                        "settings-delivery-Herdr",
                        "delivery-label-Herdr",
                        "delivery-radio-Herdr",
                        "delivery-note-Herdr",
                    ],
                    [
                        "settings-delivery-Terminal",
                        "delivery-label-Terminal",
                        "delivery-radio-Terminal",
                        "delivery-note-Terminal",
                    ],
                    [
                        "settings-delivery-System",
                        "delivery-label-System",
                        "delivery-radio-System",
                        "delivery-note-System",
                    ],
                ] {
                    let [row, name, radio, note] =
                        selectors.map(|selector| cx.debug_bounds(selector).unwrap());
                    let current = (radio.left(), note.left());
                    assert_eq!(*columns.get_or_insert(current), current);
                    assert_eq!(name.size.width, px(font_size * 5.));
                    assert_eq!(radio.size, size(px(14.), px(14.)));
                    assert_eq!(radio.left() - name.right(), px(12.));
                    assert_eq!(note.left() - radio.right(), px(12.));
                    assert!(note.size.width > px(0.));
                    assert!(row.left() >= body.left() && row.right() <= body.right());
                    assert!(row.top() >= previous_bottom);
                    for child in [name, radio, note] {
                        assert!(child.left() >= row.left() && child.right() <= row.right());
                        assert!(child.top() >= row.top() && child.bottom() <= row.bottom());
                    }
                    previous_bottom = row.bottom();
                }
                if width == 680. {
                    let note = cx.debug_bounds("delivery-note-System").unwrap();
                    assert!(note.size.height > px(18.), "narrow descriptions must wrap");
                }
            }
        }
    }

    #[gpui::test]
    fn general_shows_shared_tab_bar_and_copy_controls_and_holds_them_while_busy(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(skill_fixture);
        cx.simulate_resize(size(px(960.), px(2200.)));
        view.update(cx, |view, cx| {
            view.shared = Some(
                crate::herdr_settings::Settings::parse_text(
                    "[ui]\ncopy_on_select = false\ntab_bar_position = 'bottom'",
                )
                .unwrap(),
            );
            view.saving = true;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        for id in [
            "settings-tab-bar-top",
            "settings-tab-bar-bottom",
            "settings-hide-single-tab-bar",
            "settings-copy-on-select",
        ] {
            let control = cx.debug_bounds(id).unwrap();
            // A save in flight owns the shared snapshot: clicks wait for it.
            cx.simulate_click(control.center(), Default::default());
            view.read_with(cx, |view, _| {
                assert!(view.save_completion.is_none(), "{id}");
                let shared = view.shared.as_ref().unwrap();
                assert!(!shared.copy_on_select);
                assert_eq!(shared.tab_bar_position, TabBarPosition::Bottom);
                assert!(!shared.hide_tab_bar_when_single_tab);
            });
        }
    }

    #[gpui::test]
    fn show_agents_is_in_appearance_and_disabled_during_saves(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(skill_fixture);
        cx.simulate_resize(size(px(960.), px(2200.)));
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        assert!(cx.debug_bounds("settings-show-agents").is_none());
        for show in [false, true] {
            view.update(cx, |view, cx| {
                view.section = Section::Appearance;
                view.config.show_agents = show;
                view.saving = true;
                cx.notify();
            });
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
            let button = cx.debug_bounds("settings-show-agents").unwrap();
            cx.simulate_click(button.center(), Default::default());
            view.update(cx, |view, cx| {
                assert_eq!(view.config.show_agents, show);
                assert!(view.save_completion.is_none());
                view.saving = false;
                view.accept_layout_choice(LayoutMode::Orca, cx);
                // Exercise reconciliation without touching personal configuration.
                view.save_with(
                    || Ok(()),
                    move || {
                        let mut loaded = skill_load()?;
                        loaded.config.show_agents = !show;
                        Ok(loaded)
                    },
                    false,
                    cx,
                );
            });
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(view.config.show_agents, !show);
                assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
                assert_eq!(view.config.layout.mode, LayoutMode::Orca);
                assert!(!view.busy());
            });
        }
    }

    #[gpui::test]
    fn sidebar_chooser_wraps_and_sample_controls_never_save(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(skill_fixture);
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        assert!(cx.debug_bounds("sidebar-preview").is_none());
        view.update(cx, |view, cx| {
            view.section = Section::Appearance;
            cx.notify();
        });
        let original = view.read_with(cx, |view, _| view.config.layout.mode);
        for width in [680., 960.] {
            cx.simulate_resize(size(px(width), px(2200.)));
            view.update(cx, |view, cx| {
                view.controls.sidebar_preview = Default::default();
                cx.notify();
            });
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
            let body = cx.debug_bounds("settings-body").unwrap();
            let panel = cx.debug_bounds("sidebar-preview").unwrap();
            assert!(panel.right() <= body.right());
            let mut previous = None;
            for selector in [
                "settings-layout-normal",
                "settings-layout-compact",
                "settings-layout-comfortable",
                "settings-layout-normal-rounded",
                "settings-layout-compact-rounded",
                "settings-layout-comfortable-rounded",
                "settings-layout-superset",
                "settings-layout-orca",
                "settings-layout-minimal",
            ] {
                let bounds = cx.debug_bounds(selector).unwrap();
                if let Some(bottom) = previous {
                    assert!(bounds.top() >= bottom);
                }
                previous = Some(bounds.bottom());
            }
            for selector in [
                "preview-width-320",
                "row-Settings window",
                "row-preview-agent-0",
                "collapse-0",
            ] {
                let bounds = cx.debug_bounds(selector).unwrap();
                cx.simulate_click(bounds.center(), Default::default());
                cx.update(|window, cx| {
                    crate::sidebar::layout_tests::full_draw(window, cx).clear(cx)
                });
                view.read_with(cx, |view, _| {
                    assert!(!view.busy());
                    assert!(view.save_completion.is_none());
                    assert_eq!(view.config.layout.mode, original);
                });
            }
            assert_eq!(
                cx.debug_bounds("sidebar-preview").unwrap().size.width,
                px(320.)
            );
        }
        view.update(cx, |view, cx| {
            view.saving = true;
            view.error = Some("Existing error".into());
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let choice = cx.debug_bounds("settings-layout-orca").unwrap();
        cx.simulate_click(choice.center(), Default::default());
        view.update(cx, |view, _| {
            assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
            assert!(view.save_completion.is_none());
            assert_eq!(view.error.as_deref(), Some("Existing error"));
            view.saving = false;
        });
    }

    #[gpui::test]
    fn appearance_scroll_reaches_sidebar_preview(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(skill_fixture);
        for width in [680., 960.] {
            cx.simulate_resize(size(px(width), px(560.)));
            view.update(cx, |view, cx| {
                view.section = Section::Appearance;
                view.body_scroll.set_offset(Point::default());
                cx.notify();
            });
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
            let panel = cx.debug_bounds("sidebar-preview").unwrap();
            let body = cx.debug_bounds("settings-body").unwrap();
            assert!(panel.top() > body.bottom());
            view.update(cx, |view, cx| {
                view.body_scroll
                    .set_offset(point(px(0.), body.top() - panel.top()));
                cx.notify();
            });
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
            let row = cx.debug_bounds("row-Settings window").unwrap();
            assert!(row.top() >= body.top() && row.bottom() <= body.bottom());
            cx.simulate_click(row.center(), Default::default());
            view.read_with(cx, |view, _| {
                assert!(!view.busy());
                assert!(view.save_completion.is_none());
                assert!(view.layout_intent.is_none());
            });
        }
    }

    #[gpui::test]
    fn sidebar_layout_draft_survives_other_save_failure(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(skill_fixture);
        view.update(cx, |view, cx| {
            view.accept_layout_choice(LayoutMode::Orca, cx);
            view.save_with(|| Err(crate::Error::MissingHome), skill_load, false, cx);
            assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
            assert!(view.busy());
            view.sync_controls(cx);
            assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.busy());
            assert!(view.error.is_some());
            assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
            assert_eq!(view.config.layout.mode, LayoutMode::Orca);
        });
    }

    #[gpui::test]
    fn browser_skill_general_card_tracks_choice_without_a_source(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_global(AgentSkill::unasked()));
        let (view, cx) = cx.add_window_view(skill_fixture);
        cx.simulate_resize(size(px(960.), px(1200.)));
        for choice in [None, Some(Choice::Installed), Some(Choice::Declined)] {
            cx.update(|window, cx| {
                if let Some(choice) = choice {
                    AgentSkill::choose(choice, cx);
                }
                view.update(cx, |view, cx| {
                    assert!(view.source.upgrade().is_none());
                    cx.notify();
                });
                window.draw(cx).clear(cx);
            });
            assert_eq!(
                cx.debug_bounds("settings-install-browser-skill").is_some(),
                choice != Some(Choice::Installed)
            );
            assert_eq!(
                cx.debug_bounds("settings-remove-browser-skill").is_some(),
                choice == Some(Choice::Installed)
            );
        }
        // Busy buttons must not invoke the real installer even when clicked.
        view.update(cx, |view, cx| {
            view.saving = true;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let button = cx.debug_bounds("settings-install-browser-skill").unwrap();
        cx.simulate_click(button.center(), Modifiers::default());
        view.update(cx, |view, cx| {
            assert_eq!(AgentSkill::choice(cx), Some(Choice::Declined));
            assert!(view.save_completion.is_none());
            view.saving = false;
        });
    }

    #[gpui::test]
    fn browser_skill_choice_changes_only_after_success_and_serializes(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_global(AgentSkill::unasked()));
        let (view, cx) = cx.add_window_view(skill_fixture);
        for (choice, succeeds, expected) in [
            (Choice::Installed, false, None),
            (Choice::Installed, true, Some(Choice::Installed)),
            (Choice::Declined, false, Some(Choice::Installed)),
            (Choice::Declined, true, Some(Choice::Declined)),
        ] {
            view.update(cx, |view, cx| {
                let before = AgentSkill::choice(cx);
                view.save_skill_with(
                    choice,
                    move || {
                        if succeeds {
                            Ok(())
                        } else {
                            Err(crate::Error::MissingHome)
                        }
                    },
                    skill_load,
                    cx,
                );
                assert!(view.busy());
                assert_eq!(AgentSkill::choice(cx), before);
                view.save_skill_with(
                    choice,
                    || panic!("overlapping skill operation"),
                    skill_load,
                    cx,
                );
            });
            cx.run_until_parked();
            view.read_with(cx, |view, cx| {
                assert!(!view.busy());
                assert_eq!(AgentSkill::choice(cx), expected);
                assert_eq!(view.error.is_some(), !succeeds);
                assert_eq!(
                    view.status.as_deref(),
                    Some(if succeeds {
                        "Saved"
                    } else {
                        "Save failed; reloaded current preferences"
                    })
                );
            });
        }
        // A successful file operation remains successful if config reload fails.
        view.update(cx, |view, cx| {
            view.save_skill_with(
                Choice::Installed,
                || Ok(()),
                || Err(crate::Error::MissingHome),
                cx,
            );
        });
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert_eq!(AgentSkill::choice(cx), Some(Choice::Installed));
            assert!(view.error.is_some());
        });
    }

    #[test]
    fn font_filter_keeps_all_four_hundred_families_and_matches_case_insensitively() {
        let names: Vec<_> = (0..400).map(|i| format!("Family {i:03}")).collect();
        assert_eq!(filter_fonts(&names, "").len(), 400);
        assert_eq!(
            filter_fonts(&names, " FAMILY 39 "),
            (390..400).collect::<Vec<_>>()
        );
        assert_eq!(filter_fonts(&names, "Family 399"), vec![399]);
        assert!(filter_fonts(&names, "missing").is_empty());
    }

    #[test]
    fn size_steps_are_integral_bounded_and_reject_non_finite_values() {
        assert_eq!(stepped_size(8., -1.), Some(8.));
        assert_eq!(stepped_size(48., 1.), Some(48.));
        assert_eq!(stepped_size(14.2, 1.), Some(15.));
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(stepped_size(invalid, 1.), None);
            assert_eq!(stepped_size(14., invalid), None);
        }
    }

    #[test]
    fn rapid_size_intents_coalesce_without_erasing_other_faces() {
        let mut pending = Vec::new();
        queue_size(&mut pending, FontFace::Sidebar, 15.);
        queue_size(&mut pending, FontFace::Terminal, 18.);
        queue_size(&mut pending, FontFace::Sidebar, 16.);
        assert_eq!(
            pending,
            vec![(FontFace::Sidebar, 16.), (FontFace::Terminal, 18.)]
        );
    }

    #[gpui::test]
    fn busy_size_clicks_survive_config_refresh_without_starting_a_write(cx: &mut TestAppContext) {
        let (source, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let settings = cx.new(|cx| SettingsWindow::new(source.downgrade(), cx));
        settings.update(cx, |settings, cx| {
            settings.saving = true;
            settings.controls.saving_sizes = vec![(FontFace::Sidebar, 20.)];
            for _ in 0..20 {
                settings.step_control_size(FontFace::Sidebar, 1., cx);
            }
            settings.step_control_size(FontFace::Terminal, 1., cx);
            settings.config.sidebar.size = 12.;
            settings.sync_controls(cx);
            assert_eq!(
                settings.controls.size(FontFace::Sidebar, &settings.config),
                40.
            );
            assert_eq!(settings.controls.pending_sizes.len(), 2);
            assert_eq!(
                settings.controls.saving_sizes,
                vec![(FontFace::Sidebar, 20.)]
            );
            // Keep the test entirely in memory: completion and persistence are
            // exercised by the root save tests, not the user's configuration.
            settings.controls.pending_sizes.clear();
            settings.saving = false;
            settings.sync_controls(cx);
            assert_eq!(
                settings.controls.size(FontFace::Sidebar, &settings.config),
                12.
            );
        });
    }

    #[test]
    fn numeric_size_accepts_only_bounded_ascii_integers() {
        for (text, expected) in [
            ("8", Some(8.)),
            (" 48 ", Some(48.)),
            ("24", Some(24.)),
            ("7", None),
            ("49", None),
            ("12.5", None),
            ("-12", None),
            ("", None),
            ("２４", None),
            ("NaN", None),
            ("999999", None),
        ] {
            assert_eq!(parse_size(text), expected, "{text:?}");
        }
    }

    fn numeric_fixture(window: &mut Window, cx: &mut Context<SettingsWindow>) -> SettingsWindow {
        let source = cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx));
        let mut view = SettingsWindow::new(source.downgrade(), cx);
        view.section = Section::Fonts;
        // Accepted intents remain in memory while assertions inspect the queue.
        view.saving = true;
        view.controls.discovering = false;
        window.focus(&view.focus, cx);
        view
    }

    #[gpui::test]
    fn numeric_enter_blur_and_escape_use_the_same_size_queue(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(numeric_fixture);
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.begin_control_size_edit(FontFace::Terminal, window, cx);
            })
        });
        cx.simulate_input("23");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert!(view.controls.size_editor.is_none());
            assert_eq!(view.controls.pending_sizes, vec![(FontFace::Terminal, 23.)]);
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.begin_control_size_edit(FontFace::Terminal, window, cx);
            })
        });
        cx.simulate_input("41");
        cx.simulate_keystrokes("escape");
        view.read_with(cx, |view, _| {
            assert_eq!(view.controls.pending_sizes, vec![(FontFace::Terminal, 23.)])
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.begin_control_size_edit(FontFace::Sidebar, window, cx);
            })
        });
        cx.simulate_input("18");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.focus(&view.read(cx).focus.clone(), cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        view.update(cx, |view, _| {
            assert!(view.controls.size_editor.is_none());
            assert_eq!(
                view.take_pending_control_sizes(),
                vec![(FontFace::Terminal, 23.), (FontFace::Sidebar, 18.)]
            );
        });
    }

    #[gpui::test]
    fn numeric_invalid_and_composition_do_not_enqueue_writes(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(numeric_fixture);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.begin_control_size_edit(FontFace::Ui, window, cx);
            })
        });
        cx.simulate_input("99");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert!(view.controls.size_editor.as_ref().unwrap().invalid);
            assert!(view.controls.pending_sizes.is_empty());
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.begin_control_size_edit(FontFace::Ui, window, cx);
                let input = view.controls.size_editor.as_ref().unwrap().input.clone();
                input.update(cx, |input, cx| {
                    input.replace_and_mark_text_in_range(None, "二十四", Some(3..3), window, cx)
                });
                for key in ["enter", "escape"] {
                    view.control_size_key(
                        &KeyDownEvent {
                            keystroke: Keystroke::parse(key).unwrap(),
                            is_held: false,
                            prefer_character_input: false,
                        },
                        window,
                        cx,
                    );
                    assert!(view.controls.size_editor.is_some());
                    assert!(view.controls.pending_sizes.is_empty());
                }
                input.update(cx, |input, cx| input.unmark_text(window, cx));
                assert!(!view.finish_control_size_edit(true, cx));
                assert!(view.finish_control_size_edit(false, cx));
                assert!(view.controls.pending_sizes.is_empty());
            })
        });
    }
}
