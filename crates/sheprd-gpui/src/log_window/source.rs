//! Where the Logs window reads from: this app's own log (the default), or
//! the daemon log of `herdr-automate` on a host a window found it on
//! (`crate::automations::LogSources`). A host's log is read with
//! `herdr-automate log --daemon --json` on the background executor every
//! [`INTERVAL`] while it is shown; its lines are untrusted, so each is read
//! as a record when it has this app's log shape or a tracing JSON shape,
//! and otherwise shown as plain text stripped of control characters.

use super::{LogWindow, button, filtered};
use crate::{automations::LogSource, diagnostics::Record, i18n::fl};
use gpui::{prelude::*, *};
use serde_json::{Map, Value};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tracing::Level;

/// How often a host's daemon log is read again while shown.
const INTERVAL: Duration = Duration::from_secs(5);
/// How often the window checks for a filter change or a due read.
const TICK: Duration = Duration::from_millis(250);
/// The longest text kept of one untrusted field.
const FIELD: usize = 4096;

/// What the window reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum Source {
    #[default]
    App,
    Automations(LogSource),
}

impl Source {
    pub(super) fn label(&self) -> String {
        match self {
            Self::App => fl!("log-window-source-app"),
            Self::Automations(source) => {
                fl!(
                    "log-window-source-automations",
                    host = source.label.as_str()
                )
            }
        }
    }
}

/// A host's log being followed; dropping it stops the read in flight.
pub(super) struct Remote {
    cancelled: Arc<AtomicBool>,
    _task: Task<()>,
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// The records in a daemon log's text, oldest first.
pub(super) fn parse(text: &str) -> Vec<Arc<Record>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Arc::new(record(line)))
        .collect()
}

fn clean(text: &str) -> String {
    crate::notifications::safe_text(text, FIELD)
}

/// One line as a record: this app's own shape, a `tracing` JSON event, or
/// plain text at INFO.
pub(super) fn record(line: &str) -> Record {
    if let Ok(record) = serde_json::from_str::<Record>(line) {
        return sanitized(record);
    }
    if let Ok(Value::Object(object)) = serde_json::from_str::<Value>(line)
        && let Some(record) = tracing_json(&object)
    {
        return record;
    }
    plain(line)
}

fn sanitized(record: Record) -> Record {
    Record {
        timestamp: clean(&record.timestamp),
        target: clean(&record.target),
        namespace: clean(&record.namespace),
        message: clean(&record.message),
        fields: record
            .fields
            .into_iter()
            .map(|(key, value)| (clean(&key), value))
            .collect(),
        spans: record.spans.iter().map(|span| clean(span)).collect(),
        ..record
    }
}

/// `{"timestamp", "level", "target", "fields": {"message", ...}}`, as
/// `tracing-subscriber`'s JSON format writes events.
fn tracing_json(object: &Map<String, Value>) -> Option<Record> {
    let level: Level = object.get("level")?.as_str()?.trim().parse().ok()?;
    let text = |key: &str| object.get(key).and_then(Value::as_str).map(clean);
    let mut fields = match object.get("fields") {
        Some(Value::Object(fields)) => fields.clone(),
        _ => Map::new(),
    };
    let message = fields
        .remove("message")
        .and_then(|value| value.as_str().map(clean))
        .or_else(|| text("message"))
        .unwrap_or_default();
    let fields = fields
        .into_iter()
        .map(|(key, value)| (clean(&key), value))
        .collect();
    let target = text("target").unwrap_or_default();
    Some(Record {
        level,
        timestamp: text("timestamp").map_or_else(String::new, |at| format!("[{at}]")),
        namespace: target.split("::").next().unwrap_or_default().to_owned(),
        target,
        message,
        fields,
        spans: Vec::new(),
        truncated: false,
    })
}

fn plain(line: &str) -> Record {
    Record {
        level: Level::INFO,
        timestamp: String::new(),
        target: String::new(),
        namespace: String::new(),
        message: clean(line),
        fields: Map::new(),
        spans: Vec::new(),
        truncated: line.len() > FIELD,
    }
}

impl LogWindow {
    /// Shows `source`, dropping the last one's rows and any read in flight.
    pub(super) fn pick_source(&mut self, source: Source, cx: &mut Context<Self>) {
        if source == self.source {
            return;
        }
        self.remote = None;
        self.rows.clear();
        self.retained.clear();
        self.selected = None;
        self.dropped = 0;
        self.generation = None;
        self.scroll.reset(0);
        self.status = match &source {
            Source::App => self.app_status.clone(),
            Source::Automations(_) => fl!("log-window-reading"),
        };
        if let Source::Automations(log) = &source {
            self.remote = Some(self.follow(log.clone(), cx));
        }
        self.source = source;
        cx.notify();
    }

    /// Reads `log` now and every [`INTERVAL`] while it is followed, and
    /// filters again whenever the search or level changes.
    fn follow(&mut self, log: LogSource, cx: &mut Context<Self>) -> Remote {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let task = cx.spawn(async move |this, cx| {
            let mut read_at: Option<Instant> = None;
            loop {
                let due = read_at.is_none_or(|at| at.elapsed() >= INTERVAL);
                let wanted = this.update(cx, |this, cx| {
                    let refilter = this.generation.is_none();
                    // Turned off in the settings: show what was read, read
                    // no more.
                    let off = crate::automations::LogSources::off(cx);
                    if off && this.following {
                        let status = fl!(
                            "log-window-automations-off",
                            source = Source::Automations(log.clone()).label()
                        );
                        if this.status != status {
                            this.status = status;
                            cx.notify();
                        }
                    }
                    let fetch = this.following && due && !off;
                    (fetch || refilter).then(|| {
                        (
                            fetch,
                            this.search.read(cx).text().to_owned(),
                            this.minimum,
                            this.retained.clone(),
                        )
                    })
                });
                let Ok(wanted) = wanted else { break };
                if let Some((fetch, query, minimum, retained)) = wanted {
                    if fetch {
                        read_at = Some(Instant::now());
                    }
                    let source = log.clone();
                    let cancelled = flag.clone();
                    let filter_query = query.clone();
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            let read = if fetch {
                                source
                                    .read(&cancelled)
                                    .map(|text| (parse(&text.text), text.truncated))
                                    .map(Some)
                            } else {
                                Ok(None)
                            };
                            read.map(|read| {
                                let (records, truncated) = read
                                    .map_or((retained, None), |(records, truncated)| {
                                        (records, Some(truncated))
                                    });
                                (
                                    filtered(records.clone(), &filter_query, minimum),
                                    records,
                                    truncated,
                                )
                            })
                        })
                        .await;
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let applied = this.update(cx, |this, cx| {
                        if this.search.read(cx).text() != query || this.minimum != minimum {
                            return;
                        }
                        this.generation = Some(0);
                        match result {
                            Ok((rows, retained, truncated)) => {
                                if rows.len() != this.rows.len() {
                                    this.scroll.reset(rows.len());
                                }
                                this.rows = rows;
                                this.retained = retained;
                                if let Some(truncated) = truncated {
                                    let source = Source::Automations(log.clone()).label();
                                    this.status = if truncated {
                                        fl!("log-window-log-tail", source = source)
                                    } else {
                                        fl!("log-window-log-whole", source = source)
                                    };
                                }
                            }
                            Err(error) => {
                                this.status = fl!(
                                    "log-window-host-unreadable",
                                    host = log.label.as_str(),
                                    error = error.to_string()
                                );
                            }
                        }
                        cx.notify();
                    });
                    if applied.is_err() {
                        break;
                    }
                }
                cx.background_executor().timer(TICK).await;
            }
        });
        Remote {
            cancelled,
            _task: task,
        }
    }

    /// The source buttons, when any host has a daemon log to offer.
    pub(super) fn render_sources(&self, cx: &mut Context<Self>) -> Option<Div> {
        let sources = crate::automations::LogSources::current(cx);
        if sources.is_empty() && self.source == Source::App {
            return None;
        }
        let theme = &self.appearance.theme;
        let mut choices = vec![Source::App];
        choices.extend(sources.into_iter().map(Source::Automations));
        if !choices.contains(&self.source) {
            // A host that went away stays listed while it is shown.
            choices.push(self.source.clone());
        }
        Some(
            div()
                .debug_selector(|| "log-sources".into())
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child(fl!("log-window-source")),
                )
                .children(choices.into_iter().enumerate().map(|(index, choice)| {
                    let on = choice == self.source;
                    button(format!("log-source-{index}"), choice.label(), theme)
                        .border_1()
                        .border_color(rgb(if on { theme.primary() } else { theme.active }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pick_source(choice.clone(), cx);
                        }))
                })),
        )
    }
}
