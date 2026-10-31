//! The panel body of a provider's report: the shared fields, then whatever
//! detail the provider attached, found by its type. A provider attaches
//! detail in `herdr-usage`; drawing it lives here, beside the rest of the
//! panel, so that crate needs no GPUI.

use super::{
    model::{Report, Window},
    text,
    ui::Ui,
};
use crate::i18n::fl;
use gpui::{AnyElement, Div, div, prelude::*, px};
use herdr_usage::details::{ClaudeAccount, ClaudeAccountStatus, OpenaiDaily};

pub(super) fn render(report: &Report, ui: &Ui) -> AnyElement {
    let standard = ui.standard(report);
    if let Some(daily) = report
        .detail::<OpenaiDaily>()
        .filter(|daily| !daily.days.is_empty())
    {
        return div()
            .flex()
            .flex_col()
            .child(standard)
            .child(ui.rule())
            .child(daily_spend(daily, ui))
            .into_any_element();
    }
    if let Some(others) = report
        .detail::<Vec<ClaudeAccount>>()
        .filter(|others| !others.is_empty())
    {
        return div()
            .flex()
            .flex_col()
            .child(standard)
            .child(ui.rule())
            .child(other_accounts(others, ui))
            .into_any_element();
    }
    standard
}

/// OpenAI's spend per day, as bars scaled to the costliest day.
fn daily_spend(daily: &OpenaiDaily, ui: &Ui) -> Div {
    let max = daily.days.iter().map(|(_, cost)| *cost).fold(0., f64::max);
    ui.block()
        .child(ui.heading(fl!("usage-daily-spend")))
        .children(daily.days.iter().map(|(label, cost)| {
            let fill = if max > 0. {
                (cost / max * 100.) as f32
            } else {
                0.
            };
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(ui.small())
                .child(
                    div()
                        .w(px(52.))
                        .flex_none()
                        .text_color(ui.muted())
                        .child(label.clone()),
                )
                .child(div().flex_1().child(ui.bar(fill, 0., None)))
                .child(
                    div()
                        .w(px(64.))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .child(format!("${cost:.2}")),
                )
        }))
}

/// Each other Claude account claude-swap manages, under a heading: its name
/// and note, then a bar of what is left per window.
fn other_accounts(others: &[ClaudeAccount], ui: &Ui) -> Div {
    ui.block()
        .child(ui.heading(fl!("usage-other-accounts")))
        .children(others.iter().map(|other| {
            let note = note(other);
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
}

/// Why an account's windows are missing or old, and whether claude-swap
/// holds it out of rotation, as `ClaudeAccount::note` says in English.
pub(super) fn note(other: &ClaudeAccount) -> String {
    let status = match other.status {
        ClaudeAccountStatus::Ok if other.stale => "stale",
        ClaudeAccountStatus::Ok => "ok",
        ClaudeAccountStatus::TokenExpired => "token-expired",
        ClaudeAccountStatus::ApiKey => "api-key",
        ClaudeAccountStatus::KeychainUnavailable => "keychain",
        ClaudeAccountStatus::ReloginRequired => "relogin",
        ClaudeAccountStatus::ForeignCredential => "foreign",
        ClaudeAccountStatus::NoCredentials => "no-credentials",
        ClaudeAccountStatus::Unavailable | ClaudeAccountStatus::Unknown => "unavailable",
    };
    match (status, other.disabled) {
        ("ok", false) => String::new(),
        ("ok", true) => fl!("usage-account-disabled"),
        (status, disabled) => fl!(
            "usage-account-note",
            status = status,
            disabled = if disabled { "yes" } else { "no" }
        ),
    }
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
                .child(text::title(&limit.kind)),
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
                .child(text::left(limit)),
        )
}
