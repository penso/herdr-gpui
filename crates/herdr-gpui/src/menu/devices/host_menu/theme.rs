//! Picking a saved device's terminal theme from its menu. Hovering a row
//! previews it on the terminal while that device is the one shown; Enter or a
//! click saves `[devices.<id>] theme`, and "Same as app" removes it.
use super::HostMenu;
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    config::{Theme, ThemeName},
    menu::Page,
    search_input::SearchInput,
};
use gpui::{prelude::*, *};

/// The row that removes the device's own theme.
const SAME_AS_APP: &str = "host-menu-theme-same-as-app";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Row {
    /// Follow the app theme: no `theme` key for the device.
    App,
    Named(String),
}

pub(crate) struct DeviceThemePicker {
    search: Entity<SearchInput>,
    names: Vec<String>,
    pub(super) filtered: Vec<Row>,
    pub(super) selected: usize,
    scroll: UniformListScrollHandle,
    pub(super) error: Option<String>,
    /// The previewed theme and its colors, once read.
    preview: Option<(String, Theme)>,
    /// The read of a theme file to preview; replacing it drops the old one.
    preview_load: Option<Task<()>>,
    /// The read of a chosen theme that saves it once it loads.
    save_load: Option<Task<()>>,
    /// Listing the theme files, cancelled with the picker so it never
    /// fills a picker opened later for another device.
    pub(super) discovery: Option<Task<()>>,
    _subscription: Subscription,
}

impl DeviceThemePicker {
    fn filter(&mut self, query: &str) {
        let query = query.trim().to_lowercase();
        // Its name in the shown language and in English, as the palette
        // matches both.
        let app = [
            crate::i18n::message(SAME_AS_APP, &[]),
            crate::i18n::english(SAME_AS_APP, &[]),
        ]
        .iter()
        .any(|name| name.to_lowercase().contains(&query));
        self.filtered = app
            .then_some(Row::App)
            .into_iter()
            .chain(
                self.names
                    .iter()
                    .filter(|name| name.to_lowercase().contains(&query))
                    .map(|name| Row::Named(name.clone())),
            )
            .collect();
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    pub(super) fn selected_row(&self) -> Option<&Row> {
        self.filtered.get(self.selected)
    }
}

/// The row the device's saved value shows for `light`.
fn current_row(value: Option<&str>, light: bool) -> Row {
    match value {
        Some(value) => Row::Named(ThemeName::side(value, light).to_owned()),
        None => Row::App,
    }
}

impl HerdrWindow {
    pub(super) fn open_device_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &self.menu.host else {
            return;
        };
        let light = crate::app::light_appearance(cx);
        let current = current_row(self.device_theme_value(&host.id), light);
        let search = cx.new(SearchInput::new);
        search.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("host-menu-theme-search"), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        let subscription = cx.subscribe(
            &search,
            |this, search, _: &crate::search_input::Changed, cx| {
                let query = search.read(cx).text().to_owned();
                if let Some(picker) = this.device_theme_picker_mut() {
                    picker.filter(&query);
                }
                this.preview_device_theme(cx);
            },
        );
        let mut names: Vec<String> = Theme::BUILTIN_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        if let Row::Named(name) = &current
            && !names.contains(name)
        {
            names.push(name.clone());
        }
        names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
        let mut picker = DeviceThemePicker {
            search,
            names,
            filtered: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            error: None,
            preview: None,
            preview_load: None,
            save_load: None,
            discovery: None,
            _subscription: subscription,
        };
        picker.filter("");
        picker.selected = picker
            .filtered
            .iter()
            .position(|row| *row == current)
            .unwrap_or(0);
        picker
            .scroll
            .scroll_to_item(picker.selected, ScrollStrategy::Center);
        picker.discovery = Some(self.discover_device_themes(current, cx));
        if let Some(host) = &mut self.menu.host {
            host.theme = Some(picker);
        }
        self.menu.page = Some(Page::DeviceTheme);
        self.preview_device_theme(cx);
        cx.notify();
    }

    fn device_theme_picker_mut(&mut self) -> Option<&mut DeviceThemePicker> {
        self.menu.host.as_mut()?.theme.as_mut()
    }

    /// Lists the theme files as the app picker does, off the UI thread.
    fn discover_device_themes(&self, current: Row, cx: &mut Context<Self>) -> Task<()> {
        let config = self.config.clone();
        let discovery = cx
            .background_executor()
            .spawn(async move { config.available_themes() });
        cx.spawn(async move |this, cx| {
            let result = discovery.await;
            let _ = this.update(cx, |this, cx| {
                let Some(picker) = this.device_theme_picker_mut() else {
                    return;
                };
                picker.discovery = None;
                match result {
                    Ok(mut names) => {
                        if let Row::Named(name) = &current
                            && !names.contains(name)
                        {
                            names.push(name.clone());
                            names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
                        }
                        let selected = picker.selected_row().cloned();
                        let query = picker.search.read(cx).text().to_owned();
                        picker.names = names;
                        picker.filter(&query);
                        if let Some(index) = picker
                            .filtered
                            .iter()
                            .position(|row| Some(row) == selected.as_ref())
                        {
                            picker.selected = index;
                            picker.scroll.scroll_to_item(index, ScrollStrategy::Center);
                        }
                    }
                    Err(error) => picker.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
    }

    /// Previews the selected row: a built-in at once, a file once read.
    pub(super) fn preview_device_theme(&mut self, cx: &mut Context<Self>) {
        let config = self.config.clone();
        let light = crate::app::light_appearance(cx);
        let Some(picker) = self.device_theme_picker_mut() else {
            return;
        };
        let Some(Row::Named(name)) = picker.selected_row().cloned() else {
            picker.preview = None;
            picker.preview_load = None;
            cx.notify();
            return;
        };
        if picker
            .preview
            .as_ref()
            .is_some_and(|(previewed, _)| *previewed == name)
        {
            return;
        }
        if let Some(theme) = Theme::builtin(&name) {
            picker.preview = Some((name, theme.styled_for(&config)));
            picker.preview_load = None;
            cx.notify();
            return;
        }
        let read = cx.background_executor().spawn({
            let name = name.clone();
            async move { config.theme_named(&name, light) }
        });
        picker.preview_load = Some(cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                let Some(picker) = this.device_theme_picker_mut() else {
                    return;
                };
                if picker.selected_row() != Some(&Row::Named(name.clone())) {
                    return;
                }
                picker.preview_load = None;
                match result {
                    Ok(theme) => {
                        picker.preview = Some((name, theme));
                        picker.error = None;
                    }
                    Err(error) => picker.error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    /// The terminal colors the picker shows for `endpoint`, while it is open
    /// on that device: the app theme on "Same as app", else the theme read
    /// for the selected row.
    pub(crate) fn device_theme_preview(&self, endpoint: &str) -> Option<&Theme> {
        if self.menu.page != Some(Page::DeviceTheme) {
            return None;
        }
        let host = self.menu.host.as_ref().filter(|host| host.id == endpoint)?;
        let picker = host.theme.as_ref()?;
        match picker.selected_row()? {
            Row::App => Some(&self.theme),
            Row::Named(name) => picker
                .preview
                .as_ref()
                .filter(|(previewed, _)| previewed == name)
                .map(|(_, theme)| theme),
        }
    }

    /// Saves the row and closes the menu. A pair keeps its other side: the
    /// row replaces the side the current appearance shows, as in the app
    /// picker. A theme file is read first; one that cannot load is not
    /// saved, and the picker stays open showing why.
    fn choose_device_theme(&mut self, row: Row, window: &mut Window, cx: &mut Context<Self>) {
        let light = crate::app::light_appearance(cx);
        let Some(host) = &self.menu.host else {
            return;
        };
        let theme = match &row {
            Row::App => None,
            Row::Named(name) => {
                let loaded = host
                    .theme
                    .as_ref()
                    .and_then(|picker| picker.preview.as_ref())
                    .filter(|(previewed, _)| previewed == name)
                    .map(|(_, theme)| theme.clone())
                    .or_else(|| Theme::builtin(name).map(|theme| theme.styled_for(&self.config)));
                let Some(theme) = loaded else {
                    self.read_then_choose(name.clone(), window, cx);
                    return;
                };
                Some(theme)
            }
        };
        let value = match &row {
            Row::App => None,
            Row::Named(name) => Some(match self.device_theme_value(&host.id) {
                Some(existing) => ThemeName::with_side(existing, light, name),
                None => name.clone(),
            }),
        };
        let Some(profile) = host.profile() else {
            return;
        };
        if value.as_deref() != self.device_theme_value(&host.id) {
            let (endpoint, profile) = (host.id.clone(), profile.to_owned());
            self.save_device_theme(endpoint, profile, value, theme, cx);
        }
        self.dismiss_menu(window, cx);
    }

    /// Reads the chosen theme file, then saves it.
    fn read_then_choose(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let config = self.config.clone();
        let light = crate::app::light_appearance(cx);
        let read = cx.background_executor().spawn({
            let name = name.clone();
            async move { config.theme_named(&name, light) }
        });
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(picker) = this.device_theme_picker_mut() else {
                    return;
                };
                picker.save_load = None;
                match result {
                    Ok(theme) => {
                        picker.preview = Some((name.clone(), theme));
                        picker.error = None;
                        this.choose_device_theme(Row::Named(name), window, cx);
                    }
                    Err(error) => {
                        picker.error = Some(error.to_string());
                        cx.notify();
                    }
                }
            });
        });
        if let Some(picker) = self.device_theme_picker_mut() {
            picker.save_load = Some(task);
        }
    }

    pub(in crate::menu) fn device_theme_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.device_theme_picker_mut() else {
            return;
        };
        // Unhandled text must reach the native input system, including IME.
        if picker.search.read(cx).is_composing() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                window.prevent_default();
                self.dismiss_menu(window, cx);
            }
            key @ ("up" | "down") if !picker.filtered.is_empty() => {
                cx.stop_propagation();
                window.prevent_default();
                let count = picker.filtered.len();
                picker.selected =
                    (picker.selected + if key == "up" { count - 1 } else { 1 }) % count;
                picker
                    .scroll
                    .scroll_to_item(picker.selected, ScrollStrategy::Center);
                self.preview_device_theme(cx);
            }
            "enter" => {
                cx.stop_propagation();
                window.prevent_default();
                if let Some(row) = picker.selected_row().cloned() {
                    self.choose_device_theme(row, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn render_device_theme(&self, host: &HostMenu, cx: &mut Context<Self>) -> Div {
        let Some(picker) = &host.theme else {
            return div();
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        let shown = self
            .endpoints
            .get(self.selected_endpoint)
            .is_some_and(|endpoint| endpoint.id == host.id);
        let header = div()
            .flex_none()
            .p(px(16.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(font.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(fl!("host-menu-theme-title", device = host.label.as_str())),
                    )
                    .child(
                        div()
                            .id("device-theme-close")
                            .debug_selector(|| "device-theme-close".into())
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded(px(crate::config::corners::CONTROL))
                            .hover(|s| s.bg(rgb(theme.active)))
                            .child(fl!("host-menu-theme-close"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx)),
                            ),
                    ),
            )
            .child(div().pt(px(12.)).child(picker.search.clone()));
        let current = current_row(
            self.device_theme_value(&host.id),
            crate::app::light_appearance(cx),
        );
        let list = uniform_list(
            "device-theme-results",
            picker.filtered.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                let Some(picker) = this.menu.host.as_ref().and_then(|host| host.theme.as_ref())
                else {
                    return Vec::new();
                };
                range
                    .map(|index| {
                        let row = picker.filtered[index].clone();
                        let label = match &row {
                            Row::App => crate::i18n::message(SAME_AS_APP, &[]),
                            Row::Named(name) => name.clone(),
                        };
                        div()
                            .id(index)
                            .debug_selector(move || format!("device-theme-row-{index}"))
                            .w_full()
                            .h(px(this.config.ui.line_height() + 20.))
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .when(index == picker.selected, |row| {
                                row.bg(rgb(this.theme.active))
                            })
                            .hover(|s| s.bg(rgb(this.theme.active)))
                            .child(div().flex_1().min_w_0().truncate().child(label))
                            .when(row == current, |row| {
                                row.child(
                                    div()
                                        .text_color(rgb(this.theme.muted))
                                        .child(fl!("host-menu-theme-current")),
                                )
                            })
                            .on_hover(cx.listener(move |this, hovered, _, cx| {
                                if *hovered && let Some(picker) = this.device_theme_picker_mut() {
                                    picker.selected = index;
                                    this.preview_device_theme(cx);
                                }
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose_device_theme(row.clone(), window, cx)
                            }))
                    })
                    .collect()
            }),
        )
        .track_scroll(&picker.scroll)
        .flex_1()
        .min_h_0();
        let status = if let Some(error) = &picker.error {
            error.clone()
        } else if shown {
            fl!("host-menu-theme-hover-hint")
        } else {
            fl!("host-menu-theme-select-hint")
        };
        div()
            .debug_selector(|| "device-theme".into())
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(header)
            .child(list)
            .child(
                div()
                    .debug_selector(|| "device-theme-status".into())
                    .flex_none()
                    .px(px(16.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child(status),
            )
    }
}
