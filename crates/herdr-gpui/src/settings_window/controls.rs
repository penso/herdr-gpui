//! Prepared controls for the standalone window; persistence belongs to its serial save path.
pub(super) mod backdrop;
pub(super) mod chat_env;
pub(super) mod code;
mod fonts;
mod forms;
pub(super) mod indicators;
pub(super) mod plugins;
#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(super) use fonts::native_probe;
pub(super) use fonts::{face_font, family_label};
pub(super) mod preferences;
pub(super) mod px0;

use super::{Section, SettingsWindow};
use crate::{
    config::{Config, FONT_SIZE_RANGE, FontFace, corners},
    font_picker::FontTarget,
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};

pub(in crate::settings_window) const FACES: [(FontFace, &str); 4] = [
    (FontFace::Terminal, "Terminal"),
    (FontFace::Sidebar, "Sidebar"),
    (FontFace::Tabs, "Tabs"),
    (FontFace::Ui, "Interface"),
];

pub(super) struct Controls {
    #[cfg(test)]
    pub(in crate::settings_window) preference_io: Option<preferences::PreferenceIo>,
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
    pub(in crate::settings_window) local_path: String,
    pub(in crate::settings_window) local_file: Option<std::path::PathBuf>,
    pub(in crate::settings_window) ruler: backdrop::Ruler,
    _search_changed: Subscription,
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
            local_path: Config::local_path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| format!("Unavailable ({error})")),
            local_file: Config::local_path().ok(),
            ruler: backdrop::Ruler::default(),
            _search_changed: subscription,
        }
    }

    pub(super) fn set_active_face(&mut self, face: FontFace) {
        self.active_face = face;
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
        let installed = self.installed_fonts.clone();
        let discovery = cx.background_executor().spawn(async move {
            let mut names: Vec<_> = installed
                .get_or_init(|| text_system.all_font_names())
                .iter()
                .filter(|name| !name.trim().is_empty())
                .cloned()
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
            self.controls.ruler.settle();
            self.flush_control_sizes(cx);
            self.flush_code_url(cx);
        }
    }

    pub(super) fn refresh_control_appearance(&mut self, cx: &mut Context<Self>) {
        self.controls.integration_search.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        self.controls.search.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        self.nav_search.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
        });
        self.plugins
            .refresh_appearance(&self.config, &self.theme, cx);
        self.refresh_code_appearance(cx);
        self.sync_px0_field(cx);
        self.sync_chat_env_field(cx);
        self.sync_remote_fields(cx);
        if let Some(edit) = &self.number_edit {
            edit.input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            });
        }
        if let Some(edit) = &self.hex_edit {
            edit.input.update(cx, |input, cx| {
                input.set_appearance(self.config.terminal.clone(), self.theme.clone(), cx);
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

    pub(in crate::settings_window) fn control_note(&self, text: impl Into<SharedString>) -> Div {
        div()
            .min_w_0()
            .text_color(rgb(self.theme.muted))
            .child(text.into())
    }

    pub(in crate::settings_window) fn control_choice(
        &self,
        id: impl Into<SharedString>,
        label: impl IntoElement,
        selected: bool,
        enabled: bool,
    ) -> Stateful<Div> {
        // A choice is a button or one of several options, and its label may
        // be any element, so its role and name are the caller's to set.
        crate::accessibility::disabled(div().id(id.into()), !enabled)
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

    /// Whether Herdr's shared settings can be edited at all. Busy is not part
    /// of it: saves and loads refuse overlapping work themselves, and a control
    /// that changes look for their few milliseconds flickers.
    pub(in crate::settings_window) fn controls_shared_ready(&self) -> bool {
        cfg!(unix) && self.shared.is_some() && self.error.is_none()
    }

    pub(super) fn render_controls(&self, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let content = match self.section {
            Section::Fonts => self.render_fonts(cx),
            Section::StatusBar => self.render_status_bar(cx),
            Section::Sidebar => self.render_sidebar(cx),
            Section::Indicators => self.render_indicators(cx),
            Section::General => self.render_general(cx),
            Section::Tabs => self.render_tabs(cx),
            Section::Notifications => self.render_notifications(cx),
            Section::Configuration => self.render_configuration(cx),
            Section::Theme | Section::Integrations | Section::Plugins | Section::Code => div(),
            #[cfg(feature = "cloud")]
            Section::CloudDevices => div(),
            Section::RemoteControl => div(),
        };
        div().flex().flex_col().min_w_0().child(content)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
