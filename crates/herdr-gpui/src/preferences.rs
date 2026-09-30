use crate::{
    HerdrWindow,
    config::{Config, FONT_SIZE_RANGE, Features, FontFace},
    contrast::Contrast,
    font_picker::{FontTarget, shared_family},
    fonts::StyledFont,
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};

/// Debug selector, label, and state of each feature flag, in display order.
/// Flags are turned on in the config file, so Preferences only reports them.
pub(crate) fn feature_rows(features: &Features) -> [(&'static str, &'static str, bool); 1] {
    [(
        "preferences-feature-sidebar-hover-menu",
        "Sidebar hover menu",
        features.sidebar_hover_menu,
    )]
}

pub(crate) struct FontSizeEditor {
    face: FontFace,
    pub(crate) input: Entity<SearchInput>,
    _blur: Subscription,
}

/// The inline editor for `projects_root`. It closes on blur or Enter, exactly
/// like the font size field, but has no live preview: the sidebar only reads
/// the setting, so applying it is the save.
pub(crate) struct ProjectsRootEditor {
    pub(crate) input: Entity<SearchInput>,
    _blur: Subscription,
}

/// The queued writes behind the Preferences fields, kept apart from the font
/// size writer so a slow font save never drops one of these. At most one edit
/// per setting is queued; the latest wins.
#[derive(Default)]
pub(crate) struct SettingsSaves {
    root: Option<RootEdit>,
    flag: Option<(bool, bool)>,
    task: Option<Task<()>>,
    error: Option<String>,
}

struct RootEdit {
    value: String,
    previous: Option<PathBuf>,
}

/// One queued settings write, with the value to restore if it fails.
#[derive(Clone)]
enum Edit {
    Root {
        value: String,
        previous: Option<PathBuf>,
    },
    Flag {
        value: bool,
        previous: bool,
    },
}

impl SettingsSaves {
    fn is_busy(&self) -> bool {
        self.task.is_some() || self.root.is_some() || self.flag.is_some()
    }

    pub(crate) fn status(&self) -> Option<&str> {
        self.error
            .as_deref()
            .or_else(|| self.is_busy().then_some("Saving settings…"))
    }

    fn queue_root(&mut self, value: String, previous: Option<PathBuf>) {
        self.root = Some(RootEdit { value, previous });
        self.error = None;
    }

    fn queue_flag(&mut self, value: bool, previous: bool) {
        self.flag = Some((value, previous));
        self.error = None;
    }
}

fn save_setting(edit: &Edit) -> crate::Result<()> {
    match edit {
        Edit::Root { value, .. } => Config::save_projects_root(value),
        Edit::Flag { value, .. } => Config::save_use_herdr_projects(*value),
    }
}

fn parse_font_size(text: &str) -> Option<f32> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value = text.parse::<u8>().ok()?;
    FONT_SIZE_RANGE
        .contains(&f32::from(value))
        .then_some(f32::from(value))
}

impl HerdrWindow {
    fn begin_font_size_edit(
        &mut self,
        face: FontFace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_projects_root_edit(true, cx);
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_text_selected(&format!("{}", face.size(&self.config)), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        let focus = input.read(cx).focus.clone();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.finish_font_size_edit(true, cx);
        });
        self.menu.font_size_editor = Some(FontSizeEditor {
            face,
            input: input.clone(),
            _blur: blur,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn finish_font_size_edit(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.menu.font_size_editor.take() else {
            return;
        };
        if save
            && !editor.input.read(cx).is_composing()
            && let Some(size) = parse_font_size(editor.input.read(cx).text())
        {
            self.set_font_size(editor.face, size, cx);
        }
        cx.notify();
    }

    /// Write one setting off the UI thread, then reload so the window shows
    /// what the file now says rather than what was clicked.
    fn save_preference(
        &mut self,
        save: impl FnOnce() -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let text_system = cx.text_system().clone();
        self.load_gui_config_with(
            move || {
                save()?;
                let mut config = Config::load()?;
                config.resolve_font_fallbacks(|| text_system.all_font_names());
                let theme = config.theme()?;
                Ok((config, theme))
            },
            cx,
        );
    }

    fn begin_projects_root_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_font_size_edit(true, cx);
        let input = cx.new(SearchInput::new);
        let current = self
            .config
            .projects_root
            .as_ref()
            .map(|root| root.display().to_string())
            .unwrap_or_default();
        input.update(cx, |input, cx| {
            input.set_text_selected(&current, cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        let focus = input.read(cx).focus.clone();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.finish_projects_root_edit(true, cx);
        });
        self.menu.projects_root_editor = Some(ProjectsRootEditor { input, _blur: blur });
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn finish_projects_root_edit(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.menu.projects_root_editor.take() else {
            return;
        };
        if save && !editor.input.read(cx).is_composing() {
            let value = editor.input.read(cx).text().to_owned();
            self.set_projects_root(value, cx);
        }
        cx.notify();
    }

    /// Applies a committed `projects_root`: the same validation as the config
    /// file, an immediate in-memory update, then one background write. An
    /// invalid value changes nothing and is reported in the footer.
    fn set_projects_root(&mut self, value: String, cx: &mut Context<Self>) {
        let resolved = match Config::resolve_projects_root(&value) {
            Ok(root) => root,
            Err(error) => {
                self.settings_saves.error =
                    Some(format!("Could not save projects folder: {error}"));
                cx.notify();
                return;
            }
        };
        let previous = self.config.projects_root.clone();
        self.config.projects_root = resolved;
        self.settings_saves.queue_root(value, previous);
        cx.notify();
        self.flush_settings_saves(cx);
    }

    /// Turns the `herdr-projects` integration on or off. Turning it on fills and
    /// saves the plugin's default projects folder when none was chosen, and
    /// re-checks that the plugin is installed.
    pub(super) fn toggle_use_herdr_projects(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.config.use_herdr_projects;
        let previous_root = self.config.projects_root.clone();
        self.config.use_herdr_projects = enabled;
        self.herdr_projects.error = None;
        if enabled {
            if self.config.projects_root.is_none() {
                match crate::herdr_projects::default_projects_root() {
                    Ok(root) => {
                        self.settings_saves
                            .queue_root(root.display().to_string(), previous_root);
                        self.config.projects_root = Some(root);
                    }
                    Err(error) => {
                        self.herdr_projects.error =
                            Some(format!("Could not default the projects folder: {error}"));
                    }
                }
            }
            self.refresh_herdr_projects(cx);
        }
        self.settings_saves.queue_flag(enabled, !enabled);
        cx.notify();
        self.flush_settings_saves(cx);
    }

    fn flush_settings_saves(&mut self, cx: &mut Context<Self>) {
        self.flush_settings_saves_with(save_setting, cx);
    }

    fn flush_settings_saves_with(
        &mut self,
        save: impl Fn(&Edit) -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.config_load.is_some() || self.settings_saves.task.is_some() {
            return;
        }
        let mut edits = Vec::new();
        if let Some(root) = self.settings_saves.root.take() {
            edits.push(Edit::Root {
                value: root.value,
                previous: root.previous,
            });
        }
        if let Some((value, previous)) = self.settings_saves.flag.take() {
            edits.push(Edit::Flag { value, previous });
        }
        if edits.is_empty() {
            return;
        }
        let background_edits = edits.clone();
        let saved = cx.background_executor().spawn(async move {
            let mut result = Ok(());
            for edit in &background_edits {
                if let Err(error) = save(edit) {
                    result = Err(error);
                    break;
                }
            }
            (result, save)
        });
        self.settings_saves.task = Some(cx.spawn(async move |this, cx| {
            let (result, save) = saved.await;
            let _ = this.update(cx, |this, cx| {
                this.settings_saves.task = None;
                if let Err(error) = result {
                    tracing::warn!(%error, "Could not save settings");
                    // A newer edit already owns its setting; never roll it back.
                    for edit in &edits {
                        match edit {
                            Edit::Root { previous, .. } if this.settings_saves.root.is_none() => {
                                this.config.projects_root = previous.clone();
                            }
                            Edit::Flag { previous, .. } if this.settings_saves.flag.is_none() => {
                                this.config.use_herdr_projects = *previous;
                            }
                            _ => {}
                        }
                    }
                    this.settings_saves.error = Some(format!("Could not save settings: {error}"));
                }
                this.flush_settings_saves_with(save, cx);
                cx.notify();
            });
        }));
    }

    pub(super) fn render_preferences(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        let accent = crate::menu::accent(theme);
        let section = |title: &'static str| {
            div()
                .pt(px(12.))
                .pb(px(6.))
                .text_size(px(font.size * 0.85))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(title)
        };
        let row = |id: &'static str, label: &'static str, value: String| {
            div()
                .debug_selector(move || id.into())
                .flex()
                .min_w_0()
                .gap(px(12.))
                .py(px(7.))
                .border_b_1()
                .border_color(rgb(theme.active))
                .child(
                    div()
                        .w(relative(0.3))
                        .flex_none()
                        .min_w_0()
                        .text_color(rgb(theme.muted))
                        .child(label),
                )
                .child(div().flex_1().min_w_0().text_right().child(value))
        };
        // A row that flips a setting when clicked, with a switch showing its state.
        let toggle = |id: &'static str, label: &'static str, on: bool| {
            row(id, label, if on { "On" } else { "Off" }.into())
                .id(id)
                .items_center()
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.active)))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .w(px(30.))
                        .h(px(18.))
                        .p(px(2.))
                        .rounded_full()
                        .bg(rgb(if on { theme.foreground } else { theme.muted }))
                        .when(on, |track| track.justify_end())
                        .child(div().size(px(14.)).rounded_full().bg(rgb(theme.background))),
                )
        };
        let note = |text: &'static str| {
            div()
                .min_w_0()
                .py(px(10.))
                .text_color(rgb(theme.muted))
                .child(text)
        };
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .min_w_0()
                .px(px(8.))
                .py(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .border_color(rgb(theme.active))
                .bg(rgb(theme.background))
                .text_color(accent)
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.active)))
                .child(label)
        };
        let projects_root_value = match &self.config.projects_root {
            Some(root) => root.display().to_string(),
            None => "Not set".into(),
        };
        let projects_root_row = div()
            .debug_selector(|| "preferences-projects-root".into())
            .flex()
            .min_w_0()
            .gap(px(12.))
            .py(px(7.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .w(relative(0.3))
                    .flex_none()
                    .min_w_0()
                    .text_color(rgb(theme.muted))
                    .child("Projects folder"),
            )
            .child(match &self.menu.projects_root_editor {
                Some(editor) => div()
                    .flex_1()
                    .min_w_0()
                    .child(editor.input.clone())
                    .into_any_element(),
                None => div()
                    .id("preferences-projects-root-edit")
                    .debug_selector(|| "preferences-projects-root-edit".into())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_right()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.active)))
                    .child(projects_root_value)
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.begin_projects_root_edit(window, cx);
                    }))
                    .into_any_element(),
            });
        let use_toggle_row = div()
            .debug_selector(|| "preferences-use-herdr-projects".into())
            .flex()
            .min_w_0()
            .gap(px(12.))
            .py(px(7.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .w(relative(0.3))
                    .flex_none()
                    .min_w_0()
                    .text_color(rgb(theme.muted))
                    .child("Use herdr-projects"),
            )
            .child(
                div()
                    .id("preferences-use-herdr-projects-toggle")
                    .debug_selector(|| "preferences-use-herdr-projects-toggle".into())
                    .flex_1()
                    .min_w_0()
                    .text_right()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.active)))
                    .child(if self.config.use_herdr_projects {
                        "On"
                    } else {
                        "Off"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_use_herdr_projects(cx);
                    })),
            );
        let plugin_status = self
            .herdr_projects
            .error
            .clone()
            .unwrap_or_else(|| self.herdr_projects.plugin_status());
        let plugin_row = div()
            .debug_selector(|| "preferences-herdr-projects-plugin".into())
            .flex()
            .min_w_0()
            .gap(px(12.))
            .py(px(7.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .w(relative(0.3))
                    .flex_none()
                    .min_w_0()
                    .text_color(rgb(theme.muted))
                    .child("Plugin"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_right()
                    .child(plugin_status),
            );
        let show_install = self.herdr_projects.checked
            && self.herdr_projects.installed.is_none()
            && !self.herdr_projects.installing();
        let install_row = div().py(px(8.)).child(
            button(
                "preferences-install-herdr-projects",
                "Install herdr-projects",
            )
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.install_herdr_projects(cx);
            })),
        );
        let mut body = div()
            .id("preferences-body")
            .debug_selector(|| "preferences-body".into())
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.menu.preferences_scroll)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.menu.font_size_editor.is_some() {
                        this.finish_font_size_edit(true, cx);
                        window.focus(&this.menu.focus, cx);
                    }
                    if this.menu.projects_root_editor.is_some() {
                        this.finish_projects_root_edit(true, cx);
                        window.focus(&this.menu.focus, cx);
                    }
                }),
            )
            .px(px(16.))
            .py(px(8.))
            .child(section("APPEARANCE"))
            .child(row(
                "preferences-show-agents",
                "Show agents",
                self.config.show_agents.to_string(),
            ))
            .child(
                toggle(
                    "preferences-show-usage",
                    "Show usage",
                    self.config.usage.show,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    let show = !this.config.usage.show;
                    this.save_preference(move || Config::save_usage_visibility(show), cx);
                })),
            )
            .child(
                toggle(
                    "preferences-high-contrast",
                    "High contrast",
                    self.config.contrast == Contrast::High,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    let contrast = match this.config.contrast {
                        Contrast::Standard => Contrast::High,
                        Contrast::High => Contrast::Standard,
                    };
                    this.save_preference(move || Config::save_contrast(contrast), cx);
                })),
            )
            .child(row(
                "preferences-confirm-close-tab",
                "Confirm tab close",
                self.config.confirm_close_tab.to_string(),
            ))
            .child(row(
                "preferences-layout",
                "Layout",
                self.config.layout.mode.to_string(),
            ))
            .child(row(
                "preferences-sidebar-gap",
                "Sidebar gap",
                format!("{} px", self.config.layout.sidebar_gap),
            ))
            .child(row("preferences-theme", "Theme", self.config.theme.clone()))
            .child(div().py(px(10.)).child(
                button("preferences-choose-theme", "Choose theme").on_click(cx.listener(
                    |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_theme_picker(window, cx);
                    },
                )),
            ))
            .child(section("HERDR PROJECTS"))
            .child(use_toggle_row)
            .child(plugin_row)
            .when(show_install, |body| body.child(install_row))
            .child(projects_root_row)
            .child(note(
                "Optional integration with the herdr-projects plugin. With the switch on and a projects folder set, Spaces get a + for creating a project from that folder, and spaces inside the folder move to a Projects section. Install the plugin first if it is missing.",
            ))
            .child(section("FONTS"));
        body = body.child(
            div()
                .debug_selector(|| "preferences-font-all".into())
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(12.))
                .py(px(7.))
                .border_b_1()
                .border_color(rgb(theme.active))
                .child(
                    div()
                        .w(relative(0.3))
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child("All fonts"),
                )
                .child(
                    div()
                        .id("preferences-font-all-choose")
                        .debug_selector(|| "preferences-font-all-choose".into())
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_right()
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(theme.active)))
                        .child(format!(
                            "{} ▾",
                            shared_family(&self.config).unwrap_or("Mixed")
                        ))
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_font_picker(FontTarget::All, window, cx);
                        })),
                ),
        );
        for (face, id, label, value) in [
            (
                FontFace::Sidebar,
                "preferences-font-sidebar",
                "Sidebar",
                &self.config.sidebar,
            ),
            (
                FontFace::Tabs,
                "preferences-font-tabs",
                "Tabs",
                &self.config.tabs,
            ),
            (
                FontFace::Terminal,
                "preferences-font-terminal",
                "Terminal",
                &self.config.terminal,
            ),
            (FontFace::Ui, "preferences-font-ui", "UI", &self.config.ui),
        ] {
            let control =
                |suffix: &'static str, symbol: &'static str, direction: f32, enabled: bool| {
                    div()
                        .id(format!("{id}-{suffix}"))
                        .debug_selector(move || format!("{id}-{suffix}"))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.background))
                        .when(enabled, |button| {
                            button
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(theme.active)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.change_font_size(face, direction, cx);
                                }))
                        })
                        .when(!enabled, |button| button.text_color(rgb(theme.muted)))
                        .child(symbol)
                };
            body = body.child(
                div()
                    .debug_selector(move || id.into())
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap(px(12.))
                    .py(px(7.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .w(relative(0.3))
                            .flex_none()
                            .min_w_0()
                            .text_color(rgb(theme.muted))
                            .child(label),
                    )
                    .child(
                        div()
                            .id(format!("{id}-choose"))
                            .debug_selector(move || format!("{id}-choose"))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_right()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(theme.active)))
                            .child(format!("{} ▾", value.family))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_font_picker(FontTarget::Face(face), window, cx);
                            })),
                    )
                    .child(control(
                        "decrease",
                        "−",
                        -1.,
                        value.size > *FONT_SIZE_RANGE.start(),
                    ))
                    .child(
                        if let Some(editor) = &self.menu.font_size_editor
                            && editor.face == face
                        {
                            div()
                                .w(px(55.))
                                .flex_none()
                                .child(editor.input.clone())
                                .into_any_element()
                        } else {
                            div()
                                .id(format!("{id}-size"))
                                .debug_selector(move || format!("{id}-size"))
                                .flex_none()
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(theme.active)))
                                .child(format!("{} px", value.size))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.begin_font_size_edit(face, window, cx);
                                }))
                                .into_any_element()
                        },
                    )
                    .child(control(
                        "increase",
                        "+",
                        1.,
                        value.size < *FONT_SIZE_RANGE.end(),
                    )),
            );
        }
        body = body
            .child(section("NOTIFICATIONS"))
            .child(row("preferences-notifications-enabled", "In-app toasts", self.config.notifications.enabled.to_string()))
            .child(row("preferences-notifications-delay", "Delay (seconds)", self.config.notifications.delay_seconds.to_string()))
            .child(row("preferences-notifications-position", "Corner", format!("{:?}", self.config.notifications.position)))
            .child(note("Edit [notifications] in the local GUI config file; saved changes reload automatically. In-app notifications default off; QA previews always work. No sounds or OS notifications."))
            .child(note(
                "Font families and sizes save to local GUI overrides and reload in every window. Click a size to type 8–48; Enter or leaving the field saves, Escape cancels. Sizes are logical pixels.",
            ))
            .child(section("FEATURES"));
        for (id, label, enabled) in feature_rows(&self.config.features) {
            body = body.child(row(id, label, if enabled { "On" } else { "Off" }.into()));
        }
        body = body
            .child(note(
                "Optional behaviors, off by default. Turn one on in the [features] table of the local GUI config file; saved changes reload automatically.",
            ))
            .child(section("AGENTS"));
        let installed = crate::agent_skill::AgentSkill::choice(cx)
            == Some(crate::agent_skill::Choice::Installed);
        body = body
            .child(row(
                "preferences-browser-skill",
                "Browser skill",
                if installed { "Installed, kept up to date" } else { "Not installed" }.into(),
            ))
            .child(div().py(px(10.)).child(if installed {
                button("preferences-remove-browser-skill", "Remove browser skill").on_click(
                    cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.remove_browser_skill(cx);
                    }),
                )
            } else {
                button("preferences-install-browser-skill", "Install browser skill").on_click(
                    cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.install_browser_skill(window, cx);
                    }),
                )
            }))
            .child(note(
                "Teaches Claude Code and other agents to show you pages in browser tabs and read the notes you send. Lives in ~/.claude/skills and ~/.agents/skills. Remove deletes only the copies this app wrote.",
            ))
            .child(section("CONFIGURATION"))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .py(px(7.))
                    .child("GUI local overrides"),
            )
            .child(
                div()
                    .debug_selector(|| "preferences-config-path".into())
                    .w_full()
                    .min_w_0()
                    .p(px(10.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .bg(rgb(theme.background))
                    .child(
                        Config::local_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|error| format!("Unavailable ({error})")),
                    ),
            )
            .child(note(
                "Edit this local file; saved changes reload automatically. Unset keys inherit config-gpui.toml, which is overwritten with current defaults on startup and reload. Invalid overrides leave the current appearance unchanged.",
            ))
            .child(
                button("preferences-reload-config", "Reload GUI config").on_click(cx.listener(
                    |this, _, window, cx| {
                        cx.stop_propagation();
                        this.reload_gui_config(window, cx);
                    },
                )),
            )
            .child(note(
                "Daemon configuration is separate. Reloading GUI config does not reload daemon settings.",
            ))
            .child(section("CONNECTION"))
            .child(row(
                "preferences-connection-status",
                "Status",
                self.live.status_text(self.local_error.as_deref()),
            ))
            .child(row(
                "preferences-connection-target",
                "Target",
                format!("{:?}", self.endpoints[self.selected_endpoint].connection.target),
            ));

        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(font.line_height()))
            .text_color(rgb(theme.foreground))
            .child(
                div()
                    .debug_selector(|| "preferences-header".into())
                    .flex()
                    .items_center()
                    .flex_none()
                    .gap(px(12.))
                    .p(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex_none()
                            .w(px(3.))
                            .h(px(font.size * 2.5))
                            .rounded_full()
                            .bg(accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Preferences"),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme.muted))
                                    .child("Current GUI settings"),
                            ),
                    )
                    .child(
                        div()
                            .id("preferences-close")
                            .debug_selector(|| "preferences-close".into())
                            .flex_none()
                            .px(px(8.))
                            .py(px(4.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(|style| {
                                style
                                    .bg(rgb(theme.active))
                                    .text_color(rgb(theme.foreground))
                            })
                            .child("Close")
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    ),
            )
            .child(body)
            .child(
                div()
                    .debug_selector(|| "preferences-footer".into())
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child(
                        self.settings_saves
                            .status()
                            .or_else(|| self.font_size_saves.status())
                            .unwrap_or("Esc to close  /  click outside to dismiss")
                            .to_owned(),
                    ),
            )
    }
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Asynchronous, endpoint-local preferences. Dropping lets the worker drain queued
/// saves without waiting; process exit may interrupt pending writes.
/// How the agents panel orders its rows, as in the terminal client: grouped
/// keeps the daemon's workspace order, priority floats the agents that want
/// attention. Client-local, like the sidebar width beside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentSort {
    #[default]
    Grouped,
    Priority,
}

impl std::fmt::Display for AgentSort {
    /// Also the stored spelling, which `From<&str>` reads back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Grouped => "grouped",
            Self::Priority => "priority",
        })
    }
}

/// An unreadable or unknown preference falls back to the default rather than
/// failing the load, so this is infallible and not `FromStr`.
impl From<&str> for AgentSort {
    fn from(value: &str) -> Self {
        match value {
            "priority" => Self::Priority,
            _ => Self::Grouped,
        }
    }
}

impl AgentSort {
    pub fn toggled(self) -> Self {
        match self {
            Self::Grouped => Self::Priority,
            Self::Priority => Self::Grouped,
        }
    }

    fn parse(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(serde_json::Value::as_str)
            .map_or_else(Self::default, Self::from)
    }
}

/// The window chrome this endpoint remembers between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chrome {
    pub sidebar_width: Option<f32>,
    pub sidebar_split: Option<f32>,
    pub agent_sort: AgentSort,
}

pub struct Preferences {
    saves: Option<Sender<Chrome>>,
    loaded: Option<Receiver<Chrome>>,
    worker: Option<JoinHandle<()>>,
}

impl Preferences {
    pub fn new(socket: &Path) -> Self {
        Self::start(
            state_dir()
                .map(|dir| endpoint_path(&dir, socket))
                .ok_or(crate::Error::MissingStateRoot),
        )
    }

    fn start(path: crate::Result<PathBuf>) -> Self {
        let (saves, requests) = mpsc::channel();
        let (loaded_tx, loaded) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("gpui-preferences".into())
            .spawn(move || {
                let path = match path {
                    Ok(path) => path,
                    Err(_) => {
                        tracing::warn!(
                            category = "preferences_location",
                            "Cannot locate GPUI preferences"
                        );
                        let _ = loaded_tx.send(Chrome::default());
                        return;
                    }
                };
                let chrome = match read_chrome(&path) {
                    Ok(chrome) => chrome,
                    Err(_) => {
                        tracing::warn!(
                            category = "preferences_read",
                            "Cannot read GPUI preferences"
                        );
                        Chrome::default()
                    }
                };
                let _ = loaded_tx.send(chrome);
                for chrome in requests {
                    if write_chrome(&path, chrome).is_err() {
                        tracing::warn!(
                            category = "preferences_write",
                            "Cannot save GPUI preferences"
                        );
                    }
                }
            });
        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(error) => {
                tracing::warn!(category = "preferences_worker_start", error_kind = ?error.kind(), "Cannot start GPUI preferences worker");
                None
            }
        };
        Self {
            saves: Some(saves),
            loaded: Some(loaded),
            worker,
        }
    }

    /// Takes the initial result once; `None` means pending or already taken.
    /// Defaults stand in for a failed initial load.
    pub fn loaded(&mut self) -> Option<Chrome> {
        let result = match self.loaded.as_ref()?.try_recv() {
            Ok(chrome) => Some(chrome),
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Some(Chrome::default()),
        };
        self.loaded = None;
        result
    }

    /// Queues the whole chrome, so saving one field never drops the others.
    pub fn save(&self, chrome: Chrome) {
        if let Some(saves) = &self.saves
            && saves.send(chrome).is_err()
        {
            tracing::warn!(
                category = "preferences_worker_disconnected",
                "Cannot queue GPUI preferences save"
            );
        }
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        // Disconnect and detach: the worker drains queued saves while the process
        // remains alive, without making the UI wait for disk I/O.
        self.saves.take();
        drop(self.worker.take());
    }
}

/// The GPUI client's own state directory, shared by preferences and logs.
pub(crate) fn state_dir() -> Option<PathBuf> {
    env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })
        .map(|root| root.join("herdr/gpui"))
}

fn endpoint_path(dir: &Path, socket: &Path) -> PathBuf {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in socket.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    dir.join(format!("local-{hash:016x}.json"))
}

fn read_chrome(path: &Path) -> crate::Result<Chrome> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Chrome::default()),
        Err(error) => return Err(error.into()),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let object = value
        .as_object()
        .ok_or(crate::Error::PreferencesNotObject)?;
    // A file written before the sort existed simply keeps the default.
    let agent_sort = AgentSort::parse(object.get("agent_sort"));
    let sidebar_width = match object.get("sidebar_width_px") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            let width = value.as_f64().map(|width| width as f32);
            match width {
                Some(width) if width.is_finite() && width > 0.0 => Some(width),
                _ => return Err(crate::Error::InvalidStoredWidth),
            }
        }
    };
    let sidebar_split = object
        .get("sidebar_split")
        .and_then(serde_json::Value::as_f64)
        .map(|split| split as f32)
        .filter(|split| split.is_finite() && (0.1..=0.9).contains(split));
    Ok(Chrome {
        sidebar_width,
        sidebar_split,
        agent_sort,
    })
}

fn write_chrome(path: &Path, chrome: Chrome) -> crate::Result<()> {
    let width = chrome.sidebar_width;
    if width.is_some_and(|width| !width.is_finite() || width <= 0.0) {
        return Err(crate::Error::InvalidSidebarWidth);
    }
    let parent = path.parent().ok_or(crate::Error::PreferencesPath)?;
    fs::create_dir_all(parent)?;
    let (temporary, mut file) = loop {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".preferences-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (temporary, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let result = (|| -> crate::Result<()> {
        serde_json::to_writer(
            &mut file,
            &serde_json::json!({
                "sidebar_width_px": width,
                "sidebar_split": chrome.sidebar_split.filter(|split| {
                    split.is_finite() && (0.1..=0.9).contains(split)
                }),
                "agent_sort": chrome.agent_sort.to_string(),
            }),
        )?;
        file.write_all(b"\n")?;
        Ok(file.sync_all()?)
    })();
    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, path).map_err(crate::Error::from));
    if result.is_err()
        && let Err(error) = fs::remove_file(&temporary)
    {
        tracing::warn!(category = "preferences_cleanup", error_kind = ?error.kind(), "Cannot clean up GPUI preferences");
    }
    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::time::{Duration, Instant};

    #[core::prelude::v1::test]
    fn font_size_input_accepts_only_whole_values_in_range() {
        for (input, expected) in [
            ("8", Some(8.)),
            ("48", Some(48.)),
            (" 24 ", Some(24.)),
            ("7", None),
            ("49", None),
            ("14.5", None),
            ("-8", None),
            ("+12", None),
            ("12px", None),
            ("", None),
            ("999999", None),
        ] {
            assert_eq!(parse_font_size(input), expected, "{input:?}");
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            loop {
                let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                let path = env::temp_dir().join(format!(
                    "herdr-preferences-test-{}-{sequence}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create test directory: {error}"),
                }
            }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn await_loaded(preferences: &mut Preferences) -> Chrome {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(chrome) = preferences.loaded() {
                assert_eq!(preferences.loaded(), None);
                return chrome;
            }
            assert!(Instant::now() < deadline, "preferences load timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[core::prelude::v1::test]
    fn roundtrip_and_reset_drain_after_drop() {
        let directory = TestDirectory::new();
        let path = endpoint_path(&directory.0, Path::new("/tmp/test.sock"));
        let mut preferences = Preferences::start(Ok(path.clone()));
        assert_eq!(await_loaded(&mut preferences), Chrome::default());
        for width in 1..=100 {
            preferences.save(Chrome {
                sidebar_width: Some(width as f32),
                sidebar_split: Some(0.4),
                agent_sort: AgentSort::Priority,
            });
        }
        let worker = preferences.worker.take().unwrap();
        drop(preferences);
        // Only the test waits for persistence before reading or removing files.
        worker.join().unwrap();
        let mut preferences = Preferences::start(Ok(path.clone()));
        assert_eq!(
            await_loaded(&mut preferences),
            Chrome {
                sidebar_width: Some(100.0),
                sidebar_split: Some(0.4),
                agent_sort: AgentSort::Priority,
            }
        );
        preferences.save(Chrome::default());
        let worker = preferences.worker.take().unwrap();
        drop(preferences);
        worker.join().unwrap();
        assert_eq!(read_chrome(&path).unwrap(), Chrome::default());
        let mut preferences = Preferences::start(Ok(path));
        assert_eq!(await_loaded(&mut preferences), Chrome::default());
    }

    #[core::prelude::v1::test]
    fn stored_sorts_and_older_files_both_load() {
        let directory = TestDirectory::new();
        let path = endpoint_path(&directory.0, Path::new("/tmp/sort.sock"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A file from before the sort existed, then an unknown value.
        for (json, expected) in [
            (
                r#"{"sidebar_width_px": 240.0}"#,
                Chrome {
                    sidebar_width: Some(240.0),
                    sidebar_split: None,
                    agent_sort: AgentSort::Grouped,
                },
            ),
            (
                r#"{"sidebar_width_px": null, "agent_sort": "sideways"}"#,
                Chrome::default(),
            ),
            (
                r#"{"sidebar_width_px": 200.0, "agent_sort": "priority"}"#,
                Chrome {
                    sidebar_width: Some(200.0),
                    sidebar_split: None,
                    agent_sort: AgentSort::Priority,
                },
            ),
        ] {
            fs::write(&path, json).unwrap();
            assert_eq!(read_chrome(&path).unwrap(), expected, "{json}");
        }
        // Whatever was read survives a write and read of the same value.
        let chrome = Chrome {
            sidebar_width: Some(321.0),
            sidebar_split: None,
            agent_sort: AgentSort::Priority,
        };
        write_chrome(&path, chrome).unwrap();
        assert_eq!(read_chrome(&path).unwrap(), chrome);
    }

    #[core::prelude::v1::test]
    fn drop_does_not_wait_for_blocked_worker_and_queued_saves_still_drain() {
        let (saves, requests) = mpsc::channel();
        let (ready_tx, ready) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let (drained_tx, drained) = mpsc::channel();
        let worker = thread::spawn(move || {
            ready_tx.send(()).unwrap();
            blocked.recv().unwrap();
            drained_tx
                .send(requests.into_iter().collect::<Vec<_>>())
                .unwrap();
        });
        let preferences = Preferences {
            saves: Some(saves),
            loaded: None,
            worker: Some(worker),
        };
        let queued = [
            Chrome {
                sidebar_width: Some(160.),
                sidebar_split: None,
                agent_sort: AgentSort::Grouped,
            },
            Chrome {
                sidebar_width: Some(400.),
                sidebar_split: Some(0.6),
                agent_sort: AgentSort::Priority,
            },
            Chrome::default(),
        ];
        for chrome in queued {
            preferences.save(chrome);
        }
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        let (dropped_tx, dropped) = mpsc::channel();
        let dropper = thread::spawn(move || {
            drop(preferences);
            dropped_tx.send(()).unwrap();
        });
        let result = dropped.recv_timeout(Duration::from_secs(5));
        // Release even on failure so a regressed join does not strand the threads.
        release.send(()).unwrap();
        let saved = drained.recv_timeout(Duration::from_secs(5)).unwrap();
        dropper.join().unwrap();
        assert!(result.is_ok(), "drop waited for the blocked worker");
        assert_eq!(saved, queued);
    }

    #[core::prelude::v1::test]
    fn malformed_and_invalid_widths_fall_back_to_default() {
        let directory = TestDirectory::new();
        let path = directory.0.join("preferences.json");
        for contents in [
            "not json",
            "[]",
            "null",
            r#"{"sidebar_width_px":0}"#,
            r#"{"sidebar_width_px":-1}"#,
            r#"{"sidebar_width_px":"200"}"#,
            r#"{"sidebar_width_px":true}"#,
            r#"{"sidebar_width_px":1e100}"#,
            r#"{"sidebar_width_px":1e-100}"#,
            r#"{"sidebar_width_px":NaN}"#,
        ] {
            fs::write(&path, contents).unwrap();
            assert!(read_chrome(&path).is_err(), "accepted {contents}");
            let mut preferences = Preferences::start(Ok(path.clone()));
            assert_eq!(await_loaded(&mut preferences), Chrome::default());
        }
        for contents in ["{}", r#"{"sidebar_width_px":null}"#] {
            fs::write(&path, contents).unwrap();
            assert_eq!(read_chrome(&path).unwrap(), Chrome::default());
        }
        for width in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -1.0] {
            assert!(
                write_chrome(
                    &path,
                    Chrome {
                        sidebar_width: Some(width),
                        sidebar_split: None,
                        agent_sort: AgentSort::default(),
                    }
                )
                .is_err()
            );
        }
        let chrome = Chrome {
            sidebar_width: Some(237.5),
            sidebar_split: None,
            agent_sort: AgentSort::default(),
        };
        write_chrome(&path, chrome).unwrap();
        assert_eq!(read_chrome(&path).unwrap(), chrome);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[core::prelude::v1::test]
    fn invalid_sidebar_splits_preserve_other_preferences() {
        let directory = TestDirectory::new();
        let path = directory.0.join("preferences.json");
        let expected = Chrome {
            sidebar_width: Some(240.0),
            sidebar_split: None,
            agent_sort: AgentSort::Priority,
        };
        for split in [
            "null", "0", "-1", "0.099", "0.901", "1e100", "1e-100", "\"0.5\"", "true", "[]", "{}",
        ] {
            fs::write(
                &path,
                format!(
                    r#"{{"sidebar_width_px":240,"agent_sort":"priority","sidebar_split":{split}}}"#
                ),
            )
            .unwrap();
            assert_eq!(read_chrome(&path).unwrap(), expected, "{split}");
        }
        for split in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            0.099,
            0.901,
        ] {
            write_chrome(
                &path,
                Chrome {
                    sidebar_split: Some(split),
                    ..expected
                },
            )
            .unwrap();
            assert_eq!(read_chrome(&path).unwrap(), expected, "{split}");
        }
    }

    #[core::prelude::v1::test]
    fn sidebar_split_roundtrips_including_boundaries_and_reset() {
        let directory = TestDirectory::new();
        let path = directory.0.join("preferences.json");
        for sidebar_split in [Some(0.1), Some(0.4), Some(0.9), None] {
            let chrome = Chrome {
                sidebar_width: Some(240.0),
                sidebar_split,
                agent_sort: AgentSort::Priority,
            };
            write_chrome(&path, chrome).unwrap();
            let stored: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(stored["sidebar_split"], serde_json::json!(sidebar_split));
            assert_eq!(read_chrome(&path).unwrap(), chrome);
        }
    }

    #[core::prelude::v1::test]
    fn endpoint_paths_use_stable_fnv1a() {
        let root = Path::new("/state/herdr/gpui");
        assert_eq!(
            endpoint_path(root, Path::new("hello")),
            Path::new("/state/herdr/gpui/local-a430d84680aabd0b.json")
        );
        assert_eq!(
            endpoint_path(root, Path::new("")),
            Path::new("/state/herdr/gpui/local-cbf29ce484222325.json")
        );
        assert_ne!(
            endpoint_path(root, Path::new("/a.sock")),
            endpoint_path(root, Path::new("/b.sock"))
        );
    }
}
