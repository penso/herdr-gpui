//! Every Claude account that claude-swap (`cswap`) manages, read from its own
//! `cswap list --json`. cswap holds each account's sign-in, refreshes it under
//! Claude Code's credential locks, and caches usage, so asking it neither
//! risks a stored refresh token nor adds calls to the usage endpoint.
//! Without cswap on the probed host there are no further accounts.

use crate::{
    Result,
    usage::{
        model::{Kind, SESSION, WEEK, Window},
        probe::Probe,
        service::{Timestamp, json},
        ui::Ui,
    },
};
use gpui::{AnyElement, Div, div, prelude::*, px};
use serde::Deserialize;
use std::time::Duration;

/// cswap may fetch a few accounts' usage before it answers.
const TIMEOUT: Duration = Duration::from_secs(15);
/// The payload shape this reads; another one is a breaking change upstream.
const SCHEMA: u32 = 1;

/// The accounts cswap lists, split into the one Claude Code is signed in
/// with and the rest.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Accounts {
    pub active: Option<Other>,
    pub others: Vec<Other>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Other {
    pub email: String,
    pub alias: Option<String>,
    /// Held out of cswap's rotation; still usable when chosen by name.
    pub disabled: bool,
    pub status: Status,
    /// The windows are cswap's last good reading, not a current one.
    pub stale: bool,
    pub windows: Vec<Window>,
}

impl Other {
    pub fn name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.email)
    }

    /// Why the windows are missing or old, in cswap's own terms.
    pub fn note(&self) -> Option<&'static str> {
        match self.status {
            Status::Ok if self.stale => Some("last known"),
            Status::Ok => None,
            Status::TokenExpired => Some("token expired"),
            Status::ApiKey => Some("API key"),
            Status::KeychainUnavailable => Some("keychain unavailable"),
            Status::ReloginRequired => Some("re-login needed"),
            Status::ForeignCredential => Some("foreign credential"),
            Status::NoCredentials => Some("no credentials"),
            Status::Unavailable | Status::Unknown => Some("unavailable"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Ok,
    TokenExpired,
    ApiKey,
    KeychainUnavailable,
    ReloginRequired,
    ForeignCredential,
    NoCredentials,
    Unavailable,
    #[serde(other)]
    Unknown,
}

/// What cswap lists on the probed host; nothing when it is not installed,
/// fails, or answers in a shape this does not know.
pub(super) fn accounts(probe: &mut Probe) -> Accounts {
    probe
        .command("cswap", &["list", "--json"], TIMEOUT)
        .ok()
        .filter(|output| output.success)
        .and_then(|output| parse(&output.stdout).ok())
        .unwrap_or_default()
}

pub(crate) fn parse(body: &str) -> Result<Accounts> {
    let list: List = json(body)?;
    if list.schema_version != SCHEMA {
        return Ok(Accounts::default());
    }
    let mut accounts = Accounts::default();
    for row in list.accounts {
        let (usage, stale) = match (row.usage, row.last_good_usage) {
            (Some(usage), _) => (Some(usage), false),
            (None, last_good) => (last_good, true),
        };
        let other = Other {
            email: row.email,
            alias: row.alias.filter(|alias| !alias.trim().is_empty()),
            disabled: row.disabled,
            status: row.usage_status,
            stale: stale && usage.is_some(),
            windows: usage.map(Usage::windows).unwrap_or_default(),
        };
        if row.active && accounts.active.is_none() {
            accounts.active = Some(other);
        } else {
            accounts.others.push(other);
        }
    }
    Ok(accounts)
}

/// Each other account under a heading: its name and note, then a bar of
/// what is left per window.
pub(super) fn render(others: &[Other], ui: &Ui) -> AnyElement {
    ui.block()
        .child(ui.heading("Other accounts"))
        .children(others.iter().map(|other| {
            let note = other
                .note()
                .into_iter()
                .chain(other.disabled.then_some("disabled"))
                .collect::<Vec<_>>()
                .join(", ");
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .pt(px(4.))
                .child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(px(8.))
                        .child(div().truncate().child(other.name().to_owned()))
                        .when(!note.is_empty(), |row| {
                            row.child(
                                div()
                                    .flex_none()
                                    .text_size(ui.small())
                                    .text_color(ui.muted())
                                    .child(note),
                            )
                        }),
                )
                .children(other.windows.iter().map(|limit| window(limit, ui)))
        }))
        .into_any_element()
}

fn window(limit: &Window, ui: &Ui) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(ui.small())
        .child(
            div()
                .w(px(96.))
                .flex_none()
                .truncate()
                .text_color(ui.muted())
                .child(limit.kind.title().to_owned()),
        )
        .child(
            div()
                .flex_1()
                .child(ui.bar(100. - limit.used, limit.used, limit.pace(ui.now))),
        )
        .child(
            div()
                .w(px(64.))
                .flex_none()
                .flex()
                .justify_end()
                .child(format!("{}% left", limit.left())),
        )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct List {
    schema_version: u32,
    /// Absent from cswap's error payload.
    #[serde(default)]
    accounts: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    email: String,
    alias: Option<String>,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    disabled: bool,
    usage_status: Status,
    usage: Option<Usage>,
    last_good_usage: Option<Usage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    five_hour: Option<Reading>,
    seven_day: Option<Reading>,
    #[serde(default)]
    scoped: Vec<Scoped>,
}

impl Usage {
    /// Session, weekly, then each model's weekly window, as Claude's own are.
    fn windows(self) -> Vec<Window> {
        let mut windows: Vec<Window> = [
            (Kind::Session, self.five_hour, SESSION),
            (Kind::Weekly, self.seven_day, WEEK),
        ]
        .into_iter()
        .filter_map(|(kind, reading, length)| reading.map(|reading| reading.window(kind, length)))
        .chain(
            self.scoped
                .into_iter()
                .filter(|scoped| !scoped.name.trim().is_empty())
                .map(|scoped| scoped.reading.window(Kind::Named(scoped.name), WEEK)),
        )
        .collect();
        windows.sort_by(|a, b| a.kind.cmp(&b.kind));
        windows
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reading {
    pct: f64,
    resets_at: Option<Timestamp>,
}

impl Reading {
    fn window(self, kind: Kind, length: Duration) -> Window {
        Window::new(
            kind,
            self.pct,
            self.resets_at.as_ref().and_then(Timestamp::time),
            Some(length),
        )
    }
}

#[derive(Deserialize)]
struct Scoped {
    name: String,
    #[serde(flatten)]
    reading: Reading,
}

#[cfg(test)]
mod tests;
