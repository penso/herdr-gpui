use crate::{
    config::{Config, Theme},
    diagnostics::{self, Record},
    fonts::StyledFont,
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};
use std::{sync::Arc, time::Duration};
use tracing::Level;

const LEVELS: [Level; 5] = [
    Level::TRACE,
    Level::DEBUG,
    Level::INFO,
    Level::WARN,
    Level::ERROR,
];

actions!(log_window, [Close, FocusSearch, FocusLevel]);

/// The console's own keys, scoped to its window. They are bound with the app
/// keymap so a config reload, which replaces every binding, keeps them.
pub(crate) fn key_bindings() -> [KeyBinding; 5] {
    [
        KeyBinding::new("cmd-w", Close, Some("LogWindow")),
        KeyBinding::new("cmd-f", FocusSearch, Some("LogWindow")),
        KeyBinding::new("cmd-l", FocusLevel, Some("LogWindow")),
        KeyBinding::new("tab", FocusLevel, Some("LogWindow")),
        KeyBinding::new("shift-tab", FocusSearch, Some("LogWindow")),
    ]
}

#[derive(Default)]
struct LogWindowHandle(Option<WindowHandle<LogWindow>>);
impl Global for LogWindowHandle {}

#[derive(Clone, Default)]
struct Appearance {
    config: Config,
    theme: Theme,
}
impl Global for Appearance {}

// Follow the rendered appearance, including previews; the console never reloads files itself.
pub(super) fn set_appearance(config: &Config, theme: &Theme, cx: &mut App) {
    cx.set_global(Appearance {
        config: config.clone(),
        theme: theme.clone(),
    });
}

fn palette_color(theme: &Theme, index: usize) -> Rgba {
    // Terminal ANSI colors can have very low contrast against UI backgrounds.
    rgb(theme.foreground).blend(rgba((theme.palette[index] << 8) | 0x70))
}

fn severity_color(theme: &Theme, level: Level) -> Rgba {
    palette_color(
        theme,
        match level {
            Level::ERROR => 1,
            Level::WARN => 3,
            Level::INFO => 2,
            Level::DEBUG => 4,
            Level::TRACE => 5,
        },
    )
}

pub(super) fn open(cx: &mut App) {
    // Global menu actions can run inside the existing window's update.
    cx.defer(open_deferred);
}

fn open_deferred(cx: &mut App) {
    if let Some(handle) = cx.default_global::<LogWindowHandle>().0
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(1100.), px(650.)), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(620.), px(360.))),
            titlebar: Some(crate::titlebar::options("Logs")),
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        |window, cx| cx.new(|cx| LogWindow::new(window, cx)),
    ) {
        Ok(handle) => cx.set_global(LogWindowHandle(Some(handle))),
        Err(_) => tracing::error!("Unable to open log window"),
    }
}

struct LogWindow {
    appearance: Appearance,
    _appearance: Subscription,
    focus: FocusHandle,
    search: Entity<SearchInput>,
    minimum: Level,
    level_focus: FocusHandle,
    menu_focus: FocusHandle,
    level_menu: Option<usize>,
    rows: Vec<Arc<Record>>,
    retained: Vec<Arc<Record>>,
    generation: Option<u64>,
    dropped: u64,
    following: bool,
    scroll: ListState,
    selected: Option<Arc<Record>>,
    status: String,
    exporting: bool,
    _search: Subscription,
    _poll: Task<()>,
}

fn filtered(records: Vec<Arc<Record>>, query: &str, minimum: Level) -> Vec<Arc<Record>> {
    let query = query.to_lowercase();
    records
        .into_iter()
        .filter(|record| {
            // tracing orders ERROR < WARN < INFO < DEBUG < TRACE.
            if record.level > minimum {
                return false;
            }
            let mut text = None;
            query.split_whitespace().all(|term| {
                if let Some(namespace) = term.strip_prefix("namespace:") {
                    record.namespace.eq_ignore_ascii_case(namespace)
                } else if let Some(target) = term.strip_prefix("target:") {
                    record.target.to_lowercase().contains(target)
                } else {
                    text.get_or_insert_with(|| record.line().to_lowercase())
                        .contains(term)
                }
            })
        })
        .collect()
}

fn export_text(rows: &[Arc<Record>], dropped: u64) -> serde_json::Result<String> {
    let mut text = serde_json::to_string(&serde_json::json!({
        "type": "metadata", "schema_version": 1,
        "app_version": crate::APP_VERSION,
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "dropped": dropped, "timestamp_format": "local [YYYY-MM-DD HH:MM:SS]"
    }))?;
    text.push('\n');
    for row in rows {
        text.push_str(&serde_json::to_string(row.as_ref())?);
        text.push('\n');
    }
    Ok(text)
}

impl LogWindow {
    fn open_levels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.level_menu = LEVELS.iter().position(|level| *level == self.minimum);
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    fn close_levels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.level_menu = None;
        window.focus(&self.level_focus, cx);
        cx.notify();
    }

    fn level_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.level_menu else { return };
        cx.stop_propagation();
        window.prevent_default();
        match event.keystroke.key.as_str() {
            "escape" | "tab" => self.close_levels(window, cx),
            "up" => self.level_menu = Some((index + LEVELS.len() - 1) % LEVELS.len()),
            "down" => self.level_menu = Some((index + 1) % LEVELS.len()),
            "home" => self.level_menu = Some(0),
            "end" => self.level_menu = Some(LEVELS.len() - 1),
            "enter" | "space" => {
                self.minimum = LEVELS[index];
                self.generation = None;
                self.close_levels(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(SearchInput::new);
        let appearance = cx.default_global::<Appearance>().clone();
        search.update(cx, |input, cx| {
            input.set_appearance(appearance.config.ui.clone(), appearance.theme.clone(), cx);
            input.set_placeholder("Search, namespace:herdr_gpui target:terminal_painter", cx);
            window.focus(&input.focus, cx);
        });
        let appearance_subscription = cx.observe_global::<Appearance>(|this, cx| {
            let font = &cx.global::<Appearance>().config.terminal;
            if font.family != this.appearance.config.terminal.family
                || font.size != this.appearance.config.terminal.size
            {
                // Width changes are handled by GPUI; font changes need explicit invalidation.
                let offset = this.scroll.logical_scroll_top();
                this.scroll.reset(this.rows.len());
                this.scroll.scroll_to(offset);
            }
            this.appearance = cx.global::<Appearance>().clone();
            this.search.update(cx, |input, cx| {
                input.set_appearance(
                    this.appearance.config.ui.clone(),
                    this.appearance.theme.clone(),
                    cx,
                );
            });
            cx.notify();
        });
        let subscription = cx.subscribe(&search, |this, _, _: &Changed, cx| {
            this.generation = None;
            cx.notify();
        });
        let poll = cx.spawn(async move |this, cx| {
            // The reader lives in this task; a background read borrows it by value.
            let mut tail = diagnostics::path().map(diagnostics::Tail::new);
            loop {
                let request = this.update(cx, |this, cx| {
                    (this.generation.is_none()
                        || (this.following && this.generation != Some(diagnostics::generation())))
                    .then(|| {
                        (
                            this.search.read(cx).text().to_owned(),
                            this.minimum,
                            this.following,
                            (!this.following).then(|| (this.retained.clone(), this.dropped)),
                        )
                    })
                });
                let Ok(request) = request else { break };
                if let Some((query, minimum, following, frozen)) = request {
                    let filter_query = query.clone();
                    // Read the hint first so a write racing the read triggers another one.
                    let generation = diagnostics::generation();
                    let (returned, snapshot) = cx
                        .background_executor()
                        .spawn(async move {
                            let snapshot = match frozen {
                                Some((retained, dropped)) => Ok((retained, dropped)),
                                None => tail.as_mut().map_or(Ok(()), diagnostics::Tail::read).map(
                                    |()| {
                                        (
                                            tail.as_ref()
                                                .map_or_else(Vec::new, diagnostics::Tail::records),
                                            diagnostics::dropped(),
                                        )
                                    },
                                ),
                            }
                            .map(|(retained, dropped)| {
                                (
                                    filtered(retained.clone(), &filter_query, minimum),
                                    retained,
                                    dropped,
                                )
                            });
                            (tail, snapshot)
                        })
                        .await;
                    tail = returned;
                    if this
                        .update(cx, |this, cx| {
                            if this.search.read(cx).text() != query
                                || this.minimum != minimum
                                || this.following != following
                            {
                                return;
                            }
                            this.generation = Some(generation);
                            match snapshot {
                                Ok((rows, retained, dropped)) => {
                                    if rows.len() != this.rows.len()
                                        || !rows
                                            .iter()
                                            .zip(&this.rows)
                                            .all(|(a, b)| Arc::ptr_eq(a, b))
                                    {
                                        this.scroll.reset(rows.len());
                                    }
                                    this.rows = rows;
                                    this.retained = retained;
                                    this.dropped = dropped;
                                }
                                // Not logged: a failing read would log on every change.
                                Err(error) => {
                                    this.status = format!("Unable to read logs: {}", error.kind())
                                }
                            }
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
            }
        });
        Self {
            appearance,
            _appearance: appearance_subscription,
            focus: cx.focus_handle(),
            search,
            minimum: Level::TRACE,
            level_focus: cx.focus_handle(),
            menu_focus: cx.focus_handle(),
            level_menu: None,
            rows: Vec::new(),
            retained: Vec::new(),
            generation: None,
            dropped: 0,
            following: true,
            scroll: ListState::new(0, ListAlignment::Top, px(100.)),
            selected: None,
            status: match diagnostics::path() {
                Some(path) => format!("Saved to {}. Review before sharing.", path.display()),
                None => "Not saved: no state directory. Review before sharing.".into(),
            },
            exporting: false,
            _search: subscription,
            _poll: poll,
        }
    }

    fn share(&mut self, save: bool, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        self.exporting = true;
        self.status = if save {
            "Choose an export destination..."
        } else {
            "Preparing clipboard..."
        }
        .into();
        let records = self.retained.clone();
        let query = self.search.read(cx).text().to_owned();
        let minimum = self.minimum;
        let dropped = self.dropped;
        let picker = save
            .then(|| cx.prompt_for_new_path(std::path::Path::new("."), Some("herdr-gpui.jsonl")));
        cx.spawn(async move |this, cx| {
            let path = match picker {
                Some(picker) => match picker.await {
                    Ok(Ok(Some(path))) => Some(path),
                    result => {
                        let cancelled = matches!(result, Ok(Ok(None)));
                        let _ = this.update(cx, |this, cx| {
                            this.exporting = false;
                            this.status = if cancelled {
                                "Export cancelled."
                            } else {
                                "Unable to open save dialog."
                            }
                            .into();
                            cx.notify();
                        });
                        return;
                    }
                },
                None => None,
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    let text = export_text(&filtered(records, &query, minimum), dropped)
                        .map_err(std::io::Error::other)?;
                    if let Some(path) = path {
                        std::fs::write(path, text).map(|()| None)
                    } else {
                        Ok(Some(text))
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.exporting = false;
                match result {
                    Ok(Some(text)) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        this.status = "Filtered logs copied. Review before sharing.".into();
                    }
                    Ok(None) => {
                        this.status = "Filtered logs exported. Review before sharing.".into()
                    }
                    Err(error) => {
                        tracing::warn!(kind = ?error.kind(), "Log export failed");
                        this.status = format!("Export failed: {}", error.kind());
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn button(id: &'static str, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    let active = theme.active;
    div()
        .id(id)
        .debug_selector(move || id.into())
        .px_2()
        .py_1()
        .rounded(px(crate::config::corners::CONTROL))
        .bg(rgb(theme.surface))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(active)))
        .child(label.into())
}

fn styled_body(record: &Record, theme: &Theme) -> StyledText {
    use std::fmt::Write;
    let mut text = record.target.clone();
    let target_end = text.len();
    for span in &record.spans {
        let _ = write!(text, " [{span}]");
    }
    let spans_end = text.len();
    if !record.message.is_empty() {
        let _ = write!(text, " {}", record.message);
    }
    let fields_start = text.len();
    for (key, value) in &record.fields {
        let _ = write!(text, " {key}={value}");
    }
    if record.truncated {
        text.push_str(" [truncated]");
    }
    let end = text.len();
    StyledText::new(text).with_highlights(
        [
            (
                0..target_end,
                HighlightStyle {
                    color: Some(palette_color(theme, 6).into()),
                    ..Default::default()
                },
            ),
            (
                target_end..spans_end,
                HighlightStyle {
                    color: Some(rgb(theme.muted).into()),
                    ..Default::default()
                },
            ),
            (
                fields_start..end,
                HighlightStyle {
                    color: Some(palette_color(theme, 4).into()),
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .filter(|(range, _)| !range.is_empty()),
    )
}

impl Render for LogWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.following {
            self.scroll.scroll_to(ListOffset {
                item_ix: self.rows.len(),
                offset_in_item: px(0.),
            });
        }
        let Appearance { config, theme } = &self.appearance;
        div()
            .key_context("LogWindow")
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &Close, window, _| window.remove_window()))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.level_menu = None;
                window.focus(&this.search.read(cx).focus.clone(), cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusLevel, window, cx| {
                this.level_menu = None;
                window.focus(&this.level_focus, cx);
                cx.notify();
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_font(&config.ui)
            .text_size(px(config.ui.size))
            .line_height(px(config.ui.line_height()))
            .map(|root| {
                #[cfg(target_os = "macos")]
                let root = root.child(crate::titlebar::render(theme.surface, 100));
                root
            })
            .child(
                div()
                    .flex_none()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(config.ui.size + 4.))
                            .child("Logs"),
                    )
                    .child(self.search.clone())
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                button("minimum-level", format!("Minimum: {} v", self.minimum), theme)
                                    .track_focus(&self.level_focus)
                                    .border_1()
                                    .border_color(if self.level_focus.is_focused(window) { palette_color(theme, 4) } else { rgb(theme.active) })
                                    .on_click(cx.listener(|this, _, window, cx| this.open_levels(window, cx)))
                                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space" | "down" | "up") {
                                            cx.stop_propagation();
                                            window.prevent_default();
                                            this.open_levels(window, cx);
                                        }
                                    }))
                                    .when_some(self.level_menu, |el, selected| el.child(
                                        deferred(
                                            anchored().child(
                                                div().id("level-menu").debug_selector(|| "level-menu".into())
                                                    .absolute().top_full().left_0().w(px(180.))
                                                    .p_1().bg(rgb(theme.surface)).border_1().border_color(rgb(theme.active))
                                                    .occlude().track_focus(&self.menu_focus)
                                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                    .on_click(|_, _, cx| cx.stop_propagation())
                                                    .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_levels(window, cx)))
                                                    .on_key_down(cx.listener(Self::level_key))
                                                    .children(LEVELS.iter().enumerate().map(|(index, level)| {
                                                        let level = *level;
                                                        div().id(("level-option", index)).debug_selector(move || format!("level-option-{index}"))
                                                            .px_2().py_1().cursor_pointer()
                                                            .when(index == selected, |el| el.bg(rgb(theme.active)))
                                                            .child(format!("{} {}", if self.minimum == level { "*" } else { " " }, level))
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                cx.stop_propagation();
                                                                this.minimum = level;
                                                                this.generation = None;
                                                                this.close_levels(window, cx);
                                                            }))
                                                    }))
                                            )
                                        ).with_priority(1)
                                    )),
                            )
                            .child(div().flex_1())
                            .child(
                                button(
                                    "follow",
                                    if self.following {
                                        "Pause"
                                    } else {
                                        "Resume tail"
                                    },
                                    theme,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.following = !this.following;
                                        this.generation = None;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                button("copy", "Copy", theme)
                                    .on_click(cx.listener(|this, _, _, cx| this.share(false, cx))),
                            )
                            .child(
                                button(
                                    "export",
                                    if self.exporting {
                                        "Working..."
                                    } else {
                                        "Export..."
                                    },
                                    theme,
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.share(true, cx))),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                        this.following = false;
                        cx.notify();
                    }))
                    .when(self.rows.is_empty(), |el| {
                        el.child(div().p_3().child("No matching logs."))
                    })
                    .when(!self.rows.is_empty(), |el| {
                        el.child(
                            list(
                                self.scroll.clone(),
                                cx.processor(|this, index: usize, _, cx| {
                                    let record = this.rows[index].clone();
                                    let theme = &this.appearance.theme;
                                    let font = &this.appearance.config.terminal;
                                    let active = theme.active;
                                    div()
                                        .id(index)
                                        .debug_selector(move || format!("log-row-{index}"))
                                        .flex()
                                        .w_full()
                                        .py(px(1.))
                                        .px_3()
                                        .text_color(rgb(theme.foreground))
                                        .text_font(font)
                                        .text_size(px(font.size))
                                        .line_height(px(font.line_height()))
                                        .child(
                                            div()
                                                .flex_none()
                                                .whitespace_nowrap()
                                                .text_color(rgb(theme.muted))
                                                .child(format!("{} ", record.timestamp)),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .whitespace_nowrap()
                                                .text_color(severity_color(
                                                    theme,
                                                    record.level,
                                                ))
                                                .child(format!("{:<5} ", record.level.as_str())),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .whitespace_normal()
                                                .debug_selector(move || format!("log-body-{index}"))
                                                .child(styled_body(&record, theme)),
                                        )
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(rgb(active)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.selected = Some(record.clone());
                                            cx.notify();
                                        }))
                                        .into_any_element()
                                }),
                            )
                            .size_full(),
                        )
                    }),
            )
            .when_some(self.selected.clone(), |el, record| {
                el.child(
                    div()
                        .id("log-detail")
                        .debug_selector(|| "log-detail".into())
                        .flex_none()
                        .h((window.viewport_size().height * 0.2).min(px(100.)))
                        .overflow_y_scroll()
                        .p_3()
                        .bg(rgb(theme.surface))
                        .text_color(severity_color(theme, record.level))
                        .text_font(&config.terminal)
                        .text_size(px(config.terminal.size))
                        .line_height(px(config.terminal.line_height()))
                        .child(record.line()),
                )
            })
            .child(
                div()
                    .flex_none()
                    .p_3()
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .truncate()
                    .child(format!(
                        "{} shown | {} dropped | {} | {}",
                        self.rows.len(),
                        self.dropped,
                        if self.following { "LIVE" } else { "PAUSED" },
                        self.status
                    )),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn records() -> Vec<Arc<Record>> {
        [
            (Level::WARN, "slow paint elapsed_ms=32"),
            (Level::INFO, "slow transport elapsed_ms=8"),
            (Level::WARN, "unrelated message"),
        ]
        .into_iter()
        .map(|(level, line)| Arc::new(Record::fixture(level, line)))
        .collect()
    }

    #[test]
    fn concrete_default_fonts_and_shared_native_decoration() {
        let appearance = Appearance::default();
        assert_eq!(
            appearance.config.terminal.family,
            if cfg!(target_os = "linux") {
                "DejaVu Sans Mono"
            } else {
                "Menlo"
            }
        );
        let options = crate::titlebar::options("Logs");
        assert_eq!(options.title.unwrap().as_ref(), "Logs");
        assert_eq!(options.appears_transparent, cfg!(target_os = "macos"));
        assert_eq!(
            options.traffic_light_position,
            cfg!(target_os = "macos").then(|| point(px(9.), px(9.)))
        );
    }

    #[gpui::test]
    fn picker_preview_and_cancel_keep_console_appearance_in_sync(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        let original = view.read_with(cx, |view, _| view.theme.clone());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_theme_picker(window, cx));
        });
        cx.simulate_input("Nord");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                cx.global::<Appearance>().theme,
                Theme::builtin("Nord").unwrap()
            );
            assert_eq!(view.read(cx).theme, cx.global::<Appearance>().theme);
            assert_eq!(view.read(cx).config.theme, "Default");
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.dismiss_menu(window, cx));
            assert_eq!(view.read(cx).theme, original);
            assert_eq!(cx.global::<Appearance>().theme, original);
        });
    }

    #[gpui::test]
    fn appearance_updates_open_paused_console_and_geometry(cx: &mut TestAppContext) {
        let mut config = Config {
            theme: "Nord".into(),
            ..Config::default()
        };
        cx.update(|cx| set_appearance(&config, &config.theme().unwrap(), cx));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = LogWindow::new(window, cx);
            view.following = false;
            view.generation = Some(diagnostics::generation());
            view.rows = records();
            view.scroll.reset(view.rows.len());
            view.selected = Some(view.rows[0].clone());
            view
        });
        for name in ["Nord", "Catppuccin Latte", "Dracula"] {
            config.theme = name.into();
            config.terminal.family = "DejaVu Sans Mono".into();
            config.terminal.size = 20.;
            config.ui.size = 16.;
            let theme = config.theme().unwrap();
            cx.update(|_, cx| set_appearance(&config, &theme, cx));
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(view.appearance.theme, theme);
                assert_eq!(view.appearance.config.ui.family, config.ui.family);
                assert_eq!(view.appearance.config.ui.size, 16.);
                assert_eq!(view.appearance.config.terminal.family, "DejaVu Sans Mono");
                assert!(!view.following);
                assert_eq!(view.rows.len(), 3);
                assert!(view.selected.is_some());
                assert_eq!(
                    severity_color(&theme, Level::ERROR),
                    palette_color(&theme, 1)
                );
                assert_eq!(
                    severity_color(&theme, Level::WARN),
                    palette_color(&theme, 3)
                );
                assert_eq!(
                    severity_color(&theme, Level::INFO),
                    palette_color(&theme, 2)
                );
                assert_eq!(
                    severity_color(&theme, Level::DEBUG),
                    palette_color(&theme, 4)
                );
                assert_eq!(
                    severity_color(&theme, Level::TRACE),
                    palette_color(&theme, 5)
                );
            });
            for (width, height) in [(1100., 650.), (620., 360.)] {
                cx.simulate_resize(size(px(width), px(height)));
                cx.run_until_parked();
                cx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
                let search = cx.debug_bounds("theme-search").unwrap();
                #[cfg(target_os = "macos")]
                {
                    let header = cx.debug_bounds("titlebar").unwrap();
                    assert_eq!(
                        header,
                        Bounds::new(point(px(0.), px(0.)), size(px(width), px(34.)))
                    );
                    assert!(search.top() >= header.bottom());
                }
                let first = cx.debug_bounds("log-row-0").unwrap();
                assert!(first.size.height >= px(config.terminal.line_height() + 1.));
                assert!(first.top() >= search.bottom());
                let detail = cx.debug_bounds("log-detail").unwrap();
                assert_eq!(detail.size.height, px((height * 0.2).min(100.)));
                view.read_with(cx, |view, _| {
                    assert!(detail.top() >= view.scroll.viewport_bounds().bottom());
                });
                assert!(detail.bottom() <= px(height));
                for selector in ["minimum-level", "follow", "copy", "export"] {
                    let bounds = cx.debug_bounds(selector).unwrap();
                    assert!(bounds.left() >= px(0.) && bounds.right() <= px(width));
                    assert!(
                        bounds.bottom() <= first.top(),
                        "{name} {width}x{height} {selector}: {bounds:?}, row: {first:?}"
                    );
                }
            }
        }
    }

    #[gpui::test]
    fn narrow_layout_renders_search_and_virtualized_rows(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = LogWindow::new(window, cx);
            view.following = false;
            view.generation = Some(diagnostics::generation());
            view.retained = (0..5000)
                .map(|index| Arc::new(Record::fixture(Level::INFO, format!("fixture row {index}"))))
                .collect();
            view.rows = view.retained.clone();
            view.scroll.reset(view.rows.len());
            view
        });
        cx.simulate_resize(size(px(620.), px(650.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let search = cx.debug_bounds("theme-search").unwrap();
        assert!(search.size.width > px(500.));
        assert!(search.size.height > px(0.));
        assert!(search.left() >= px(0.) && search.right() <= px(620.));
        let first = cx.debug_bounds("log-row-0").unwrap();
        let tenth = cx.debug_bounds("log-row-10").unwrap();
        assert_eq!(first.size.height, px(22.));
        assert!(first.top() >= search.bottom());
        assert_eq!(tenth.top() - first.top(), px(220.));
        assert!(tenth.bottom() < px(650.));
        assert!(cx.debug_bounds("log-row-4999").is_none());
        for selector in ["minimum-level", "follow", "copy", "export"] {
            let bounds = cx.debug_bounds(selector).unwrap();
            assert!(
                bounds.left() >= px(0.) && bounds.right() <= px(620.),
                "{selector}: {bounds:?}"
            );
        }
        view.update(cx, |view, cx| {
            view.scroll.scroll_to(ListOffset {
                item_ix: 5000,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let last = cx.debug_bounds("log-row-4999").unwrap();
        assert!(last.top() > search.bottom() && last.bottom() < px(650.));
        cx.simulate_input("fixture");
        view.read_with(cx, |view, cx| {
            assert_eq!(view.search.read(cx).text(), "fixture")
        });
    }

    #[gpui::test]
    fn wrapped_rows_reflow_keep_columns_and_tail_without_changing_exports(cx: &mut TestAppContext) {
        let body = "x".repeat(160);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = LogWindow::new(window, cx);
            view.following = false;
            view.generation = Some(diagnostics::generation());
            view.retained = (0..5000)
                .map(|index| {
                    Arc::new(Record::fixture(
                        LEVELS[index % LEVELS.len()],
                        if index == 1 || index == 4999 {
                            body.clone()
                        } else {
                            "short".into()
                        },
                    ))
                })
                .collect();
            view.rows = view.retained.clone();
            view.scroll.reset(view.rows.len());
            view
        });
        let mut heights = Vec::new();
        for (width, font_size) in [(1100., 14.), (620., 14.), (620., 20.), (1100., 14.)] {
            let mut config = Config::default();
            config.terminal.size = font_size;
            cx.update(|_, cx| set_appearance(&config, &Theme::default(), cx));
            cx.simulate_resize(size(px(width), px(850.)));
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let short = cx.debug_bounds("log-row-0").unwrap();
            let long = cx.debug_bounds("log-row-1").unwrap();
            let short_body = cx.debug_bounds("log-body-0").unwrap();
            let long_body = cx.debug_bounds("log-body-1").unwrap();
            let info_body = cx.debug_bounds("log-body-2").unwrap();
            assert!(long.size.height > short.size.height);
            assert_eq!(long_body.left(), short_body.left());
            assert_eq!(info_body.left(), short_body.left());
            assert!(long_body.right() <= px(width));
            assert_eq!(short.bottom(), long.top());
            assert!(cx.debug_bounds("log-row-4999").is_none());
            heights.push(long.size.height);
        }
        assert!(heights[1] > heights[0]);
        assert!(heights[2] > heights[1]);
        assert_eq!(heights[3], heights[0]);

        // Tail must reach the bottom of the final wrapped row, not its first line.
        view.update(cx, |view, cx| {
            view.following = true;
            cx.notify();
        });
        for width in [620., 1100.] {
            cx.simulate_resize(size(px(width), px(850.)));
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let last = cx.debug_bounds("log-row-4999").unwrap();
            view.read_with(cx, |view, _| {
                assert!((last.bottom() - view.scroll.viewport_bounds().bottom()).abs() < px(1.));
                assert!(view.scroll.bounds_for_item(0).is_none());
            });
        }
        let offset = view.read_with(cx, |view, _| view.scroll.logical_scroll_top());
        let follow = cx.debug_bounds("follow").unwrap();
        cx.simulate_click(follow.center(), Modifiers::default());
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.following);
            let paused = view.scroll.logical_scroll_top();
            assert_eq!(paused.item_ix, offset.item_ix);
            assert_eq!(paused.offset_in_item, offset.offset_in_item);
        });
        cx.simulate_click(follow.center(), Modifiers::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.following));
        let position = view.read_with(cx, |view, _| view.scroll.viewport_bounds().center());
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(120.))),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.following);
            assert_eq!(view.retained.len(), 5000);
        });
        view.update(cx, |view, cx| {
            view.search
                .update(cx, |input, cx| input.set_text_selected("xxx", cx));
        });
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            assert_eq!(view.rows.len(), 2);
            assert_eq!(view.scroll.item_count(), 2);
            assert!(Arc::ptr_eq(&view.rows[1], &view.retained[4999]));
            view.share(false, cx);
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let text = cx.read_from_clipboard().unwrap().text().unwrap();
            let rows = exported_records(&text);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].level, Level::DEBUG);
            assert_eq!(rows[1].level, Level::ERROR);
            assert!(rows.iter().all(|row| row.message == body));
        });
    }

    #[gpui::test]
    fn paused_filter_changes_preserve_retained_snapshot(cx: &mut TestAppContext) {
        let retained = records();
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = LogWindow::new(window, cx);
            view.following = false;
            view.generation = Some(diagnostics::generation());
            view.retained = retained.clone();
            view.rows = retained.clone();
            view.scroll.reset(view.rows.len());
            view.dropped = 17;
            view
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_input("SLOW");
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.following);
            assert_eq!(view.rows.len(), 2);
            assert!(Arc::ptr_eq(&view.rows[0], &retained[0]));
            assert!(Arc::ptr_eq(&view.rows[1], &retained[1]));
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let select = cx.debug_bounds("minimum-level").unwrap();
        cx.simulate_click(select.center(), Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let warn = cx.debug_bounds("level-option-3").unwrap();
        cx.simulate_click(warn.center(), Modifiers::default());
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.following);
            assert_eq!(view.minimum, Level::WARN);
            assert_eq!(view.rows.len(), 1);
            assert!(Arc::ptr_eq(&view.rows[0], &retained[0]));
            assert_eq!(view.dropped, 17);
            assert_eq!(view.retained.len(), retained.len());
            assert!(
                view.retained
                    .iter()
                    .zip(&retained)
                    .all(|(a, b)| Arc::ptr_eq(a, b))
            );
        });
    }

    #[gpui::test]
    fn copy_uses_current_query_and_levels_before_rows_refresh(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = LogWindow::new(window, cx);
            view.following = false;
            view.generation = Some(diagnostics::generation());
            view.retained = records();
            // The visible rows deliberately omit the record the current filter wants.
            view.rows = vec![view.retained[2].clone()];
            view.scroll.reset(view.rows.len());
            view.dropped = 23;
            view
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            view.search
                .update(cx, |input, cx| input.set_text_selected("SLOW", cx));
            view.minimum = Level::WARN;
            assert_eq!(view.rows[0].message, "unrelated message");
            view.share(false, cx);
            assert!(view.exporting);
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let text = cx.read_from_clipboard().unwrap().text().unwrap();
            let metadata: serde_json::Value =
                serde_json::from_str(text.lines().next().unwrap()).unwrap();
            assert_eq!(metadata["dropped"], 23);
            let rows = exported_records(&text);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].message, "slow paint elapsed_ms=32");
            assert_eq!(rows[0].level, Level::WARN);
            assert!(!view.read(cx).exporting);
            assert_eq!(
                view.read(cx).status,
                "Filtered logs copied. Review before sharing."
            );
        });
    }

    #[gpui::test]
    fn shortcuts_focus_search_and_close_only_log_window(cx: &mut TestAppContext) {
        let other = cx.add_window(|_, cx| SearchInput::new(cx));
        let (view, cx) = cx.add_window_view(|window, cx| {
            crate::bind_keys(cx);
            LogWindow::new(window, cx)
        });
        cx.update(|window, cx| {
            window.focus(&view.read(cx).focus.clone(), cx);
            window.draw(cx).clear(cx);
            assert!(!view.read(cx).search.read(cx).focus.is_focused(window));
        });
        cx.simulate_keystrokes("cmd-f");
        cx.update(|window, cx| assert!(view.read(cx).search.read(cx).focus.is_focused(window)));
        cx.simulate_keystrokes("cmd-w");
        assert!(cx.windows() == vec![other.into()]);
    }

    #[gpui::test]
    fn log_window_is_singleton(cx: &mut TestAppContext) {
        cx.update(|cx| {
            open(cx);
            open(cx);
        });
        cx.update(|cx| {
            assert_eq!(cx.windows().len(), 1);
            let handle = cx.default_global::<LogWindowHandle>().0;
            if let Some(handle) = handle {
                assert!(handle.update(cx, |_, _, cx| open(cx)).is_ok());
            }
        });
        cx.update(|cx| assert_eq!(cx.windows().len(), 1));
    }
    #[test]
    fn search_levels_and_export_preserve_full_lines() {
        let records = vec![
            Arc::new(Record::fixture(Level::WARN, "slow paint elapsed_ms=32")),
            Arc::new(Record::fixture(Level::TRACE, "connected")),
        ];
        let rows = filtered(records.clone(), "SLOW", Level::TRACE);
        assert_eq!(rows.len(), 1);
        let text = export_text(&rows, 7).unwrap();
        assert!(text.contains("\"dropped\":7"));
        assert_eq!(exported_records(&text)[0], *rows[0]);
        assert!(filtered(records, "", Level::ERROR).is_empty());
    }

    fn exported_records(text: &str) -> Vec<Record> {
        text.lines()
            .skip(1)
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn structured_filters_and_minimum_severity_apply_to_json_exports() {
        let records: Vec<_> = LEVELS
            .iter()
            .map(|level| {
                let mut record = Record::fixture(*level, "paint finished");
                record.namespace = "herdr_gpui".into();
                record.target = "herdr_gpui::terminal_painter".into();
                record.fields.insert("elapsed_ms".into(), 32.into());
                Arc::new(record)
            })
            .collect();
        for (index, minimum) in LEVELS.iter().enumerate() {
            let rows = filtered(
                records.clone(),
                "namespace:HERDR_GPUI target:terminal_painter elapsed_ms=32 finished",
                *minimum,
            );
            assert_eq!(rows.len(), 5 - index);
            assert_eq!(rows[0].level, *minimum);
            let exported = exported_records(&export_text(&rows, 0).unwrap());
            assert_eq!(exported.len(), 5 - index);
            assert_eq!(exported[0].fields["elapsed_ms"], 32);
        }
        // Matching strings in message/fields must not satisfy structured filters.
        let mut impostor = Record::fixture(Level::ERROR, "herdr_gpui::terminal_painter");
        impostor.namespace = "herdr_client".into();
        impostor.target = "herdr_client::worker".into();
        let records = vec![Arc::new(impostor)];
        assert!(filtered(records.clone(), "namespace:herdr_gpui", Level::TRACE).is_empty());
        assert!(filtered(records.clone(), "target:terminal_painter", Level::TRACE).is_empty());
        assert_eq!(filtered(records, "terminal_painter", Level::TRACE).len(), 1);
        assert!(filtered(Vec::new(), "", Level::TRACE).is_empty());
        let empty = export_text(&[], 0).unwrap();
        assert_eq!(empty.lines().count(), 1);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(empty.trim()).unwrap()["type"],
            "metadata"
        );
    }

    #[gpui::test]
    fn level_dropdown_keyboard_dismissal_focus_and_input_isolation(cx: &mut TestAppContext) {
        cx.update(|cx| cx.bind_keys(key_bindings()));
        let (view, cx) = cx.add_window_view(LogWindow::new);
        cx.simulate_resize(size(px(620.), px(360.)));
        cx.run_until_parked();
        cx.simulate_keystrokes("cmd-l enter");
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            assert!(view.read(cx).menu_focus.is_focused(window));
        });
        let menu = cx.debug_bounds("level-menu").unwrap();
        assert!(menu.left() >= px(0.) && menu.right() <= px(620.));
        assert!(menu.top() >= px(0.) && menu.bottom() <= px(360.));
        cx.simulate_keystrokes("down down enter");
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.minimum, Level::INFO);
            assert!(view.level_menu.is_none());
            assert!(view.level_focus.is_focused(window));
        });
        cx.simulate_keystrokes("enter down x escape");
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.minimum, Level::INFO);
            assert!(view.level_menu.is_none());
            assert_eq!(view.search.read(cx).text(), "");
            assert!(view.level_focus.is_focused(window));
        });
        cx.simulate_keystrokes("enter end home down enter");
        view.read_with(cx, |view, _| assert_eq!(view.minimum, Level::DEBUG));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_click(point(px(600.), px(340.)), Modifiers::default());
        view.read_with(cx, |view, _| assert!(view.level_menu.is_none()));
        cx.simulate_keystrokes("shift-tab");
        cx.update(|window, cx| assert!(view.read(cx).search.read(cx).focus.is_focused(window)));
    }
}
