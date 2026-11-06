//! The panel of keys that can follow the prefix. A chord typed without a
//! pause never shows it; after [`DELAY`] it lists every chord that would run,
//! by topic, and it leaves with the next key. It lists what the keymap
//! resolved, so the daemon's `[keys]` and custom commands appear less anything
//! the app's `[keybindings]` took from them, and it draws no controls: every
//! key goes to the chord, and the pointer to whatever lies beneath. While it
//! shows, each poll keeps its rows true to what the next key would run.
// i18n: migrated

use super::SheprdWindow;
use crate::{
    Command,
    controls::CommandInfo,
    i18n::fl,
    keymap::{Keymap, Section},
    menu::{command_name, keycap, keycaps},
    motion::{self, Toggle},
};
use gpui::{Context, Keystroke, Modifiers, Role, Task, Window, div, prelude::*, px, rgb};
use sheprd_client::protocol::ClientShellCommand;
use std::time::{Duration, Instant};

/// Long enough that a chord typed from memory finishes first.
const DELAY: Duration = Duration::from_millis(400);

/// Most windows fit every default chord in this many columns.
const MAX_COLUMNS: usize = 4;

/// The space between columns.
const COLUMN_GAP: f32 = 16.;

/// Space kept between the panel and the window's sides and top.
const MARGIN: f32 = 12.;

/// How far the panel rises into place.
const RISE: f32 = 4.;

/// Below this the window has no room for a useful list.
const MIN_VIEWPORT: f32 = 180.;

/// A run of commands told apart by a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Numbered {
    Agent,
    Tab,
    Workspace,
}

impl Numbered {
    fn of(command: Command) -> Option<(Self, u8)> {
        match command {
            Command::AgentNumber(number) => Some((Self::Agent, number)),
            Command::TabNumber(number) => Some((Self::Tab, number)),
            Command::WorkspaceNumber(number) => Some((Self::Workspace, number)),
            _ => None,
        }
    }

    fn description(self) -> String {
        match self {
            Self::Agent => fl!("next-keys-focus-agent"),
            Self::Tab => fl!("next-keys-focus-tab"),
            Self::Workspace => fl!("next-keys-focus-workspace"),
        }
    }
}

/// One key the panel lists.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    /// As `keycaps` reads it.
    key: String,
    description: String,
}

/// A topic's keys, in catalog order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Group {
    title: String,
    rows: Vec<Row>,
}

/// One line of a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line<'a> {
    Heading(&'a str),
    Key(&'a Row),
}

/// Where the panel stands. Its rows outlive the prefix so it can fade out
/// with what it showed.
#[derive(Debug, Default)]
pub(crate) struct NextKeys {
    groups: Vec<Group>,
    /// The prefix's first key is still waiting out [`DELAY`]. Dropping it
    /// cancels the wait.
    pending: Option<Task<()>>,
    /// The prefix as shown in the hint.
    prefix: String,
    toggle: Toggle,
}

impl NextKeys {
    /// The prefix ended: a pending wait is off, and a shown panel leaves.
    pub(super) fn dismiss(&mut self) {
        self.pending = None;
        self.toggle.set(false, motion::now(), false);
    }

    pub(super) fn is_open(&self) -> bool {
        self.toggle.is_open()
    }

    /// Whether any of it shows at `now`, its fade out included.
    pub(super) fn showing(&self, now: Instant) -> bool {
        self.toggle.is_open() || self.toggle.shown(now) > 0.
    }
}

/// How many columns `width` wide fit in `across`, gaps between them included.
fn columns_within(across: f32, width: f32) -> usize {
    (((across + COLUMN_GAP) / (width + COLUMN_GAP)) as usize).clamp(1, MAX_COLUMNS)
}

/// The keys `keymap` runs after the prefix, by topic. A run of numbered
/// commands on the matching digits reads as one row. Empty topics are left
/// out.
fn groups(keymap: &Keymap, commands: &[ClientShellCommand]) -> Vec<Group> {
    let mut rows: [Vec<Row>; Section::ALL.len()] = Default::default();
    let mut run = None;
    for (info, key) in keymap.chords() {
        let section = &mut rows[Section::of(info.command) as usize];
        run = extend_run(section, run, info, key);
    }
    rows[Section::Commands as usize].extend(keymap.custom_chords(commands).into_iter().map(
        |(command, key)| Row {
            key,
            description: command_name(command).to_owned(),
        },
    ));
    Section::ALL
        .into_iter()
        .zip(rows)
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(section, rows)| Group {
            title: section.title(),
            rows,
        })
        .collect()
}

/// Files one chord under `rows`, folding it into the numbered run just before
/// it when it continues that run, and returns the run it leaves open: its
/// kind, first number, and last number. A digit bound to some other number
/// keeps a row of its own, as a run would misstate it.
fn extend_run(
    rows: &mut Vec<Row>,
    run: Option<(Numbered, u8, u8)>,
    info: &CommandInfo,
    key: &Keystroke,
) -> Option<(Numbered, u8, u8)> {
    let numbered = Numbered::of(info.command)
        .filter(|(_, number)| key.modifiers == Modifiers::none() && key.key == number.to_string());
    if let (Some((kind, number)), Some((open, first, last)), Some(row)) =
        (numbered, run, rows.last_mut())
        && kind == open
        && number == last + 1
    {
        row.key = format!("{first}…{number}"); // i18n: data
        row.description = kind.description();
        return Some((kind, first, number));
    }
    rows.push(Row {
        key: key.unparse(),
        description: crate::i18n::command(info),
    });
    numbered.map(|(kind, number)| (kind, number, number))
}

/// Lays `groups` out in up to `columns` columns of at most `capacity` lines,
/// as even as they allow, and counts the keys that did not fit. A heading
/// never ends a column, away from its first key.
fn flow(groups: &[Group], columns: usize, capacity: usize) -> (Vec<Vec<Line<'_>>>, usize) {
    let columns = columns.max(1);
    let total: usize = groups.iter().map(|group| group.rows.len() + 1).sum();
    // A heading moved to the next column can leave one line unused in each.
    let height = (total.div_ceil(columns) + 1).min(capacity);
    let mut laid: Vec<Vec<Line<'_>>> = vec![Vec::new()];
    let mut hidden = 0;
    for group in groups {
        let mut rows = group.rows.iter();
        let room = |laid: &[Vec<Line<'_>>]| height.saturating_sub(laid.last().map_or(0, Vec::len));
        if room(&laid) < 2 && laid.last().is_some_and(|column| !column.is_empty()) {
            laid.push(Vec::new());
        }
        if laid.len() > columns || height < 2 {
            laid.truncate(columns);
            hidden += rows.len();
            continue;
        }
        if let Some(column) = laid.last_mut() {
            column.push(Line::Heading(&group.title));
        }
        for row in rows.by_ref() {
            if room(&laid) == 0 {
                laid.push(Vec::new());
            }
            if laid.len() > columns {
                laid.truncate(columns);
                hidden += 1;
                continue;
            }
            if let Some(column) = laid.last_mut() {
                column.push(Line::Key(row));
            }
        }
    }
    (laid, hidden)
}

impl SheprdWindow {
    /// Starts the wait before the panel shows.
    pub(super) fn schedule_next_keys(&mut self, cx: &mut Context<Self>) {
        self.next_keys.pending = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DELAY).await;
            let _ = this.update(cx, |this, cx| this.show_next_keys(cx));
        }));
    }

    /// What the panel would list now, and the prefix it follows. Custom
    /// commands appear only while they would run; see `custom_command`.
    fn next_keys_rows(&self) -> (Vec<Group>, String) {
        let commands = self
            .live
            .snapshot
            .as_ref()
            .filter(|_| self.live.status.is_connected())
            .map_or(&[][..], |snapshot| &snapshot.commands);
        let keymap = self.keymap();
        (
            groups(keymap, commands),
            keymap.prefix_label().unwrap_or_default(),
        )
    }

    /// Resolves the rows here, not while drawing, which only lays them out.
    fn show_next_keys(&mut self, cx: &mut Context<Self>) {
        self.next_keys.pending = None;
        if !self.prefix_armed {
            return;
        }
        let (groups, prefix) = self.next_keys_rows();
        if groups.is_empty() {
            return;
        }
        self.next_keys.groups = groups;
        self.next_keys.prefix = prefix;
        self.next_keys.toggle.set(true, motion::now(), false);
        cx.notify();
    }

    /// Keeps an open panel true to the keys, which a config reload, a new
    /// snapshot, or a lost connection can change under it. A chord armed under
    /// another prefix must not complete under this one, so that ends the
    /// chord, as does nothing being left to list.
    pub(crate) fn sync_next_keys(&mut self, cx: &mut Context<Self>) {
        if !self.next_keys.is_open() {
            return;
        }
        let (groups, prefix) = self.next_keys_rows();
        if groups.is_empty() || prefix != self.next_keys.prefix {
            self.end_prefix();
            cx.notify();
        } else if groups != self.next_keys.groups {
            self.next_keys.groups = groups;
            cx.notify();
        }
    }

    pub(super) fn render_next_keys(&self, window: &mut Window) -> Option<impl IntoElement> {
        let now = motion::now();
        let panel = &self.next_keys;
        let shown = panel.toggle.shown(now);
        if panel.toggle.moving(now) {
            window.request_animation_frame();
        }
        let viewport = window.viewport_size();
        if !panel.showing(now)
            || f32::from(viewport.width) < MIN_VIEWPORT
            || f32::from(viewport.height) < MIN_VIEWPORT
        {
            return None;
        }
        let theme = &self.theme;
        let size = self.config.ui.size;
        let line = (size * 1.5 + 6.).ceil();
        let column_width = size * 19.;
        let foot = self.status_bar_height() + 8.;
        let padding = 12.;
        // Room for the hint beneath the columns.
        let room = f32::from(viewport.height) - foot - MARGIN - padding * 2. - line;
        let capacity = (room / line).max(0.) as usize;
        let across = f32::from(viewport.width) - MARGIN * 2. - padding * 2.;
        let columns = columns_within(across, column_width);
        let (laid, hidden) = flow(&panel.groups, columns, capacity);
        let width = column_width.min(across);
        let column = |lines: Vec<Line<'_>>| {
            div()
                .flex_none()
                .w(px(width))
                .flex()
                .flex_col()
                .children(lines.into_iter().map(|entry| {
                    let row = div().h(px(line)).flex().items_center().gap(px(4.));
                    match entry {
                        Line::Heading(title) => row
                            .text_size(px(size * 0.85))
                            .text_color(rgb(theme.muted))
                            .child(title.to_owned()),
                        Line::Key(key) => row
                            // Sized for two caps, so descriptions line up; a
                            // longer chord pushes only its own.
                            .child(
                                div()
                                    .flex_none()
                                    .min_w(px(size * 6.))
                                    .flex()
                                    .gap(px(4.))
                                    .children(
                                        keycaps(&key.key).map(|cap| keycap(cap, theme, size)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .ml(px(4.))
                                    .truncate()
                                    .child(key.description.clone()),
                            ),
                    }
                }))
        };
        let hint = fl!("next-keys-hint", prefix = panel.prefix.clone());
        let hint = match hidden {
            0 => hint,
            count => format!("{} · {hint}", fl!("next-keys-more", count = count)), // i18n: data
        };
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(foot - (1. - shown) * RISE))
                .flex()
                .justify_center()
                .opacity(shown)
                .child(
                    div()
                        .id("next-keys")
                        .debug_selector(|| "next-keys".into())
                        .role(Role::Status)
                        .aria_label(fl!("next-keys-label"))
                        .relative()
                        .max_w(px(across + padding * 2.))
                        .flex()
                        .flex_col()
                        .p(px(padding))
                        .rounded(px(crate::config::corners::PANEL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(theme.foreground))
                        .text_size(px(size))
                        .shadow_lg()
                        .child(
                            div()
                                .flex()
                                .gap(px(COLUMN_GAP))
                                .children(laid.into_iter().map(column)),
                        )
                        .child(
                            div()
                                .debug_selector(|| "next-keys-hint".into())
                                .h(px(line))
                                .flex()
                                .items_center()
                                .text_size(px(size * 0.85))
                                .text_color(rgb(theme.muted))
                                .truncate()
                                .child(hint),
                        )
                        .children(self.overlay_probe()),
                ),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
