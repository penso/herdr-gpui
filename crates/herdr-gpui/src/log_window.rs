use crate::diagnostics::{self, Record};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, IconName, IndexPath, Root, Sizable, TitleBar,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        resizable::{resizable_panel, v_resizable},
        scroll::Scrollbar,
        searchable_list::SearchableListItem,
        select::{Select, SelectEvent, SelectState},
        status_bar::StatusBar,
        tag::Tag,
        theme::Theme as KitTheme,
        v_flex,
    },
    prelude::*,
    *,
};
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
struct LogWindowHandle(Option<WindowHandle<Root>>);
impl Global for LogWindowHandle {}

/// One entry of the minimum-severity select.
#[derive(Clone)]
struct LevelItem(Level);

impl SearchableListItem for LevelItem {
    type Value = Level;

    fn title(&self) -> SharedString {
        self.0.as_str().into()
    }

    fn value(&self) -> &Level {
        &self.0
    }
}

type LevelSelect = SelectState<Vec<LevelItem>>;

/// Terminal ANSI colors can have very low contrast against UI backgrounds, so
/// accents are mixed into the foreground rather than used directly.
fn tint(theme: &KitTheme, color: Hsla) -> Hsla {
    theme.foreground.blend(color.opacity(0.44))
}

fn severity_color(theme: &KitTheme, level: Level) -> Hsla {
    tint(
        theme,
        match level {
            Level::ERROR => theme.red,
            Level::WARN => theme.yellow,
            Level::INFO => theme.green,
            Level::DEBUG => theme.blue,
            Level::TRACE => theme.magenta,
        },
    )
}

/// Row height scales with the mono face, as the terminal's line height does.
const LINE_HEIGHT: f32 = 20. / 14.;

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
            titlebar: Some(TitlebarOptions {
                title: Some("Logs".into()),
                ..TitleBar::title_bar_options()
            }),
            ..TitleBar::window_options()
        },
        |window, cx| {
            let view = cx.new(|cx| LogWindow::new(window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    ) {
        Ok(handle) => cx.set_global(LogWindowHandle(Some(handle))),
        Err(_) => tracing::error!("Unable to open log window"),
    }
}

struct LogWindow {
    focus: FocusHandle,
    search: Entity<InputState>,
    level: Entity<LevelSelect>,
    detail: Entity<TextareaState>,
    minimum: Level,
    /// The mono face rows were measured with; a change remeasures them.
    font: (SharedString, Pixels),
    rows: Vec<Arc<Record>>,
    retained: Vec<Arc<Record>>,
    generation: Option<u64>,
    dropped: u64,
    following: bool,
    scroll: ListState,
    selected: Option<Arc<Record>>,
    status: String,
    exporting: bool,
    _subscriptions: [Subscription; 3],
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

fn mono_font(cx: &App) -> (SharedString, Pixels) {
    let theme = cx.theme();
    (theme.mono_font_family.clone(), theme.mono_font_size)
}

impl LogWindow {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search, namespace:herdr_gpui target:terminal_painter")
        });
        search.update(cx, |input, cx| input.focus(window, cx));
        let level = cx.new(|cx| {
            SelectState::new(
                LEVELS.map(LevelItem).to_vec(),
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let detail = cx.new(|cx| TextareaState::new(window, cx));
        let subscriptions = [
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.generation = None;
                    cx.notify();
                }
            }),
            cx.subscribe(
                &level,
                |this, _, event: &SelectEvent<Vec<LevelItem>>, cx| {
                    if let SelectEvent::Confirm(Some(level)) = event {
                        this.set_minimum(*level, cx);
                    }
                },
            ),
            cx.observe_global::<KitTheme>(|this, cx| {
                let font = mono_font(cx);
                if font != this.font {
                    // Width changes are handled by GPUI; font changes need explicit invalidation.
                    let offset = this.scroll.logical_scroll_top();
                    this.scroll.reset(this.rows.len());
                    this.scroll.scroll_to(offset);
                    this.font = font;
                }
                cx.notify();
            }),
        ];
        let poll = cx.spawn(async move |this, cx| {
            // The reader lives in this task; a background read borrows it by value.
            let mut tail = diagnostics::path().map(diagnostics::Tail::new);
            loop {
                let request = this.update(cx, |this, cx| {
                    (this.generation.is_none()
                        || (this.following && this.generation != Some(diagnostics::generation())))
                    .then(|| {
                        (
                            this.search.read(cx).value(),
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
                            if this.search.read(cx).value() != query
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
            focus: cx.focus_handle(),
            search,
            level,
            detail,
            minimum: Level::TRACE,
            font: mono_font(cx),
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
            _subscriptions: subscriptions,
            _poll: poll,
        }
    }

    fn set_minimum(&mut self, level: Level, cx: &mut Context<Self>) {
        if self.minimum == level {
            return;
        }
        self.minimum = level;
        self.generation = None;
        cx.notify();
    }

    fn set_following(&mut self, following: bool, cx: &mut Context<Self>) {
        if self.following == following {
            return;
        }
        self.following = following;
        self.generation = None;
        cx.notify();
    }

    fn select(&mut self, record: Arc<Record>, window: &mut Window, cx: &mut Context<Self>) {
        let line = record.line();
        self.detail
            .update(cx, |detail, cx| detail.set_value(line, window, cx));
        self.selected = Some(record);
        cx.notify();
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
        let query = self.search.read(cx).value();
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

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_wrap()
            .gap_2()
            .child(
                Select::new(&self.level)
                    .id("minimum-level")
                    .title_prefix("Minimum: ")
                    .small()
                    .w(px(170.)),
            )
            .child(div().flex_1())
            .child(
                Button::new("follow")
                    .debug_selector(|| "follow".into())
                    .small()
                    .outline()
                    .icon(if self.following {
                        IconName::Pause
                    } else {
                        IconName::Play
                    })
                    .label(if self.following {
                        "Pause"
                    } else {
                        "Resume tail"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        let following = !this.following;
                        this.set_following(following, cx);
                    })),
            )
            .child(
                Button::new("copy")
                    .small()
                    .outline()
                    .icon(IconName::Copy)
                    .label("Copy")
                    .disabled(self.exporting)
                    .on_click(cx.listener(|this, _, _, cx| this.share(false, cx))),
            )
            .child(
                Button::new("export")
                    .small()
                    .primary()
                    .icon(IconName::FileText)
                    .label("Export...")
                    .loading(self.exporting)
                    .on_click(cx.listener(|this, _, _, cx| this.share(true, cx))),
            )
    }

    fn render_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.rows.is_empty() {
            return div()
                .p_3()
                .text_color(cx.theme().muted_foreground)
                .child("No matching logs.")
                .into_any_element();
        }
        let rows = list(
            self.scroll.clone(),
            cx.processor(|this, index: usize, _, cx| {
                let record = this.rows[index].clone();
                let theme = cx.theme();
                div()
                    .id(index)
                    .debug_selector(move || format!("log-row-{index}"))
                    .flex()
                    .w_full()
                    .py(px(1.))
                    .px_3()
                    .when(
                        this.selected
                            .as_ref()
                            .is_some_and(|selected| Arc::ptr_eq(selected, &record)),
                        |row| row.bg(theme.list_active),
                    )
                    .hover(|style| style.bg(theme.list_hover))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .text_color(theme.muted_foreground)
                            .child(format!("{} ", record.timestamp)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .text_color(severity_color(theme, record.level))
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select(record.clone(), window, cx);
                    }))
                    .into_any_element()
            }),
        )
        .size_full();
        div()
            .size_full()
            .relative()
            .child(rows)
            .child(
                // Dragging the thumb moves away from the tail like the wheel does.
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .w(px(16.))
                    .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                        this.set_following(false, cx);
                    }))
                    .child(Scrollbar::vertical(&self.scroll)),
            )
            .into_any_element()
    }
}

fn styled_body(record: &Record, theme: &KitTheme) -> StyledText {
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
    let color = |color: Hsla| HighlightStyle {
        color: Some(color),
        ..Default::default()
    };
    StyledText::new(text).with_highlights(
        [
            (0..target_end, color(tint(theme, theme.cyan))),
            (target_end..spans_end, color(theme.muted_foreground)),
            (fields_start..end, color(tint(theme, theme.blue))),
        ]
        .into_iter()
        .filter(|(range, _)| !range.is_empty()),
    )
}

impl Render for LogWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.following {
            self.scroll.scroll_to(ListOffset {
                item_ix: self.rows.len(),
                offset_in_item: px(0.),
            });
        }
        let (mono_family, mono_size) = self.font.clone();
        let rows = div()
            .size_full()
            .font_family(mono_family.clone())
            .text_size(mono_size)
            .line_height(mono_size * LINE_HEIGHT)
            .on_scroll_wheel(cx.listener(|this, _, _, cx| this.set_following(false, cx)))
            .child(self.render_rows(cx));
        let body = if self.selected.is_some() {
            v_resizable("log-split")
                .child(resizable_panel().child(rows))
                .child(
                    resizable_panel()
                        .size(px(120.))
                        .size_range(px(48.)..px(600.))
                        .flex_none()
                        .child(
                            div()
                                .id("log-detail")
                                .debug_selector(|| "log-detail".into())
                                .size_full()
                                .p_2()
                                .child(
                                    Textarea::new(&self.detail)
                                        .readonly(true)
                                        .h_full()
                                        .font_family(mono_family)
                                        .text_size(mono_size),
                                ),
                        ),
                )
                .into_any_element()
        } else {
            rows.into_any_element()
        };
        let theme = cx.theme();
        v_flex()
            .key_context("LogWindow")
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &Close, window, _| window.remove_window()))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.update(cx, |input, cx| input.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &FocusLevel, window, cx| {
                this.level.update(cx, |level, cx| level.focus(window, cx));
            }))
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(TitleBar::new().child(div().text_sm().child("Logs")))
            .child(
                v_flex()
                    .flex_none()
                    .p_3()
                    .gap_2()
                    .child(
                        Input::new(&self.search)
                            .prefix(IconName::Search)
                            .cleanable(true),
                    )
                    .child(self.render_toolbar(cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(body),
            )
            .child(
                StatusBar::new()
                    .left(format!(
                        "{} shown | {} dropped",
                        self.rows.len(),
                        self.dropped
                    ))
                    .left(if self.following {
                        Tag::success().small().child("LIVE")
                    } else {
                        Tag::warning().small().child("PAUSED")
                    })
                    .right(div().min_w_0().truncate().child(self.status.clone())),
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

    fn paused(
        retained: Vec<Arc<Record>>,
        window: &mut Window,
        cx: &mut Context<LogWindow>,
    ) -> LogWindow {
        let mut view = LogWindow::new(window, cx);
        view.following = false;
        view.generation = Some(diagnostics::generation());
        view.rows = retained.clone();
        view.retained = retained;
        view.scroll.reset(view.rows.len());
        view
    }

    #[gpui_kit::test]
    fn paused_filter_changes_preserve_retained_snapshot(cx: &mut TestAppContext) {
        let retained = records();
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = paused(retained.clone(), window, cx);
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
        view.update(cx, |view, cx| view.set_minimum(Level::WARN, cx));
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

    #[gpui_kit::test]
    fn copy_uses_current_query_and_levels_before_rows_refresh(cx: &mut TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = paused(records(), window, cx);
            // The visible rows deliberately omit the record the current filter wants.
            view.rows = vec![view.retained[2].clone()];
            view.scroll.reset(view.rows.len());
            view.dropped = 23;
            view
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.search
                    .update(cx, |input, cx| input.set_value("SLOW", window, cx));
                view.minimum = Level::WARN;
                assert_eq!(view.rows[0].message, "unrelated message");
                view.share(false, cx);
                assert!(view.exporting);
            });
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

    #[gpui_kit::test]
    fn level_select_and_follow_button_drive_the_filter(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, |window, cx| paused(records(), window, cx));
        cx.update(|window, cx| {
            let level = view.read(cx).level.clone();
            level.update(cx, |_, cx| {
                cx.emit(SelectEvent::Confirm(Some(Level::WARN)));
            });
            window.draw(cx).clear(cx);
        });
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.minimum, Level::WARN);
            assert_eq!(view.rows.len(), 2);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let follow = cx.debug_bounds("follow").unwrap();
        cx.simulate_click(follow.center(), Modifiers::default());
        view.read_with(cx, |view, _| assert!(view.following));
        cx.simulate_click(follow.center(), Modifiers::default());
        view.read_with(cx, |view, _| assert!(!view.following));
    }

    #[gpui_kit::test]
    fn selecting_a_row_shows_its_full_line_in_the_detail_pane(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, |window, cx| paused(records(), window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let row = cx.debug_bounds("log-row-1").unwrap();
        cx.simulate_click(row.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("log-detail").is_some());
        view.read_with(cx, |view, cx| {
            let selected = view.selected.clone().unwrap();
            assert!(Arc::ptr_eq(&selected, &view.rows[1]));
            assert_eq!(view.detail.read(cx).value(), selected.line());
        });
    }

    #[gpui_kit::test]
    fn shortcuts_focus_search_and_close_only_log_window(cx: &mut TestAppContext) {
        let other = cx.add_window(|_, _| Empty);
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            crate::bind_keys(cx);
            LogWindow::new(window, cx)
        });
        let search_focused = |view: &Entity<LogWindow>, window: &Window, cx: &App| {
            view.read(cx)
                .search
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        };
        cx.update(|window, cx| {
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
            window.draw(cx).clear(cx);
            assert!(!search_focused(&view, window, cx));
        });
        cx.simulate_keystrokes("cmd-f");
        cx.update(|window, cx| assert!(search_focused(&view, window, cx)));
        cx.simulate_keystrokes("cmd-l");
        cx.update(|window, cx| {
            assert!(
                view.read(cx)
                    .level
                    .read(cx)
                    .focus_handle(cx)
                    .contains_focused(window, cx)
            );
        });
        cx.simulate_keystrokes("shift-tab");
        cx.update(|window, cx| assert!(search_focused(&view, window, cx)));
        cx.simulate_keystrokes("cmd-w");
        assert!(cx.windows() == vec![other.into()]);
    }

    #[gpui_kit::test]
    fn log_window_is_singleton(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::test_support::init_kit(cx);
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
}
