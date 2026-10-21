//! Drawing an open item's page: a header like an issue's (list toggle,
//! breadcrumbs, stepping, GitHub, close), its tags, title, and tabs, then
//! the tab's body, which `alert` and `report` draw. With the list beside it
//! the facts are label rows; with the list folded away they are a column.

mod alert;
mod report;

use super::{
    Severity,
    page::{Found, Loaded, Opened, Page},
    section::{ago, severity_color},
};
use crate::{
    HerdrWindow,
    config::{corners, mix},
    orchestrator::Look,
};
use chrono::{DateTime, Utc};
use gpui::{prelude::*, *};
use std::time::SystemTime;

/// Where a page's GitHub text is: the listing's own facts show either way.
#[derive(Debug)]
pub(super) enum Read<'a, T> {
    Reading,
    Failed,
    Done(&'a T),
}

impl<T> Clone for Read<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Read<'_, T> {}

impl<'a, T> Read<'a, T> {
    fn new(page: Option<&'a T>, failed: bool) -> Self {
        match page {
            Some(page) => Self::Done(page),
            None if failed => Self::Failed,
            None => Self::Reading,
        }
    }

    pub(super) fn get(self) -> Option<&'a T> {
        match self {
            Self::Done(page) => Some(page),
            Self::Reading | Self::Failed => None,
        }
    }

    /// What stands in for the text not read.
    pub(super) fn waiting(self, look: &Look) -> Div {
        look.muted(match self {
            Self::Failed => "Not read from GitHub.",
            Self::Reading | Self::Done(_) => "Reading from GitHub…",
        })
    }
}

/// The facts column's width beside a wide page.
const FACTS_WIDTH: f32 = 220.;

impl HerdrWindow {
    /// What the page draws with: the section's own fonts.
    pub(super) fn security_look(&self) -> Look {
        Look {
            theme: self.theme.clone(),
            ui: self.config.ui.clone(),
            mono: self.config.terminal.clone(),
        }
    }

    pub(super) fn render_security_page(
        &self,
        opened: &Opened,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = self.security_look();
        let Some(found) = self.security.find(&opened.item) else {
            return div().into_any_element();
        };
        let (page, failed) = match self.security.page(&opened.item) {
            Some(Loaded::Read(page)) => (Some(page.as_ref()), None),
            Some(Loaded::Failed(error)) => (None, Some(error.to_string())),
            Some(Loaded::Reading { .. }) | None => (None, None),
        };
        let wide = self.security.list_hidden();
        let body = match &found {
            Found::Alert(repo, alert) => {
                let page = page.and_then(|page| match page {
                    Page::Alert(page) => Some(page),
                    Page::Report(_) => None,
                });
                let page = Read::new(page, failed.is_some());
                self.security_alert_body(&look, opened.tab, repo, alert, page, wide, cx)
            }
            Found::Report(repo, report) => {
                let page = page.and_then(|page| match page {
                    Page::Report(page) => Some(page),
                    Page::Alert(_) => None,
                });
                let page = Read::new(page, failed.is_some());
                self.security_report_body(&look, opened.tab, repo, report, page, wide, cx)
            }
        };
        let body = div()
            .flex()
            .flex_col()
            .gap_4()
            .children(failed.map(|error| self.security_failed(&look, error, cx)))
            .child(body);
        div()
            .id("security-page")
            .debug_selector(|| "security-page".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.security_page_header(&look, opened, &found, cx))
            .child(
                div()
                    .id("security-page-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.security.page_scroll)
                    .child(body.p_5()),
            )
            .into_any_element()
    }

    fn security_page_header(
        &self,
        look: &Look,
        opened: &Opened,
        found: &Found<'_>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &look.theme;
        let items = self.shown_items();
        let at = items.iter().position(|item| *item == opened.item);
        let (lead, list, last, url) = match found {
            Found::Alert(repo, alert) => (
                "icons/shield.svg",
                "Dependabot",
                format!("#{}", alert.number),
                Some(repo.alert_url(alert)),
            ),
            Found::Report(repo, report) => (
                "icons/lock.svg",
                "Private reports",
                report.advisory.clone(),
                repo.report_url(report),
            ),
        };
        let repo = opened.item.repo().to_owned();
        let crumbs = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .text_size(look.small())
            .text_color(rgb(theme.muted))
            .child(
                look.icon_button(
                    "security-page-list",
                    "icons/panel-left.svg",
                    if self.security.list_hidden() {
                        "Show the list"
                    } else {
                        "Hide the list"
                    },
                )
                .debug_selector(|| "security-page-list".into())
                .on_click(cx.listener(|this, _, _, cx| this.toggle_security_list(cx))),
            )
            .child(look.icon(lead, 11., theme.muted))
            .child(div().flex_none().child(repo))
            .child("›")
            .child(div().flex_none().child(list))
            .child("›")
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.foreground))
                    .child(last),
            );
        let stepper = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .text_size(look.small())
            .text_color(rgb(theme.muted))
            .children(at.map(|at| div().pr_1().child(format!("{} of {}", at + 1, items.len()))))
            .child(
                look.icon_button("security-page-previous", "icons/chevron-up.svg", "Previous")
                    .on_click(cx.listener(|this, _, _, cx| this.step_security_item(-1, cx))),
            )
            .child(
                look.icon_button("security-page-next", "icons/chevron-down.svg", "Next")
                    .on_click(cx.listener(|this, _, _, cx| this.step_security_item(1, cx))),
            )
            .when_some(url, |el, url| {
                el.child(
                    look.icon_button("security-page-web", "icons/external.svg", "Open on GitHub")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_security_page(&url, window, cx);
                        })),
                )
            })
            .child(
                look.icon_button("security-page-close", "icons/close.svg", "Close")
                    .debug_selector(|| "security-page-close".into())
                    .on_click(cx.listener(|this, _, _, cx| this.close_security_item(cx))),
            );
        let (tags, title) = match found {
            Found::Alert(_, alert) => (
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(tag(
                        look,
                        alert.severity.label(),
                        severity_color(theme, alert.severity),
                    ))
                    .child(tag(look, "Open", look.hue(2)))
                    .child(
                        look.mono(alert.package.clone())
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(look.muted(alert.ecosystem.clone())),
                alert.summary.clone(),
            ),
            Found::Report(_, report) => (
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(match report.severity {
                        Some(severity) => tag(
                            look,
                            format!("{} · reporter's rating", severity.label()),
                            severity_color(theme, severity),
                        ),
                        None => tag(look, "Unrated", theme.muted),
                    })
                    .child(tag(look, "Triage", look.hue(3)))
                    .child(look.icon("icons/user.svg", 12., theme.muted))
                    .child(
                        look.muted(
                            [report.reporter.clone(), report.created_at.map(since)]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" · "),
                        ),
                    ),
                report.summary.clone(),
            ),
        };
        let current = opened.tab;
        let tabs = div()
            .flex()
            .gap_4()
            .children(opened.item.tabs().iter().map(|&tab| {
                let on = tab == current;
                div()
                    .id(tab.label())
                    .debug_selector(move || format!("security-tab-{}", tab.label()))
                    .pb_2()
                    .border_b_2()
                    .border_color(rgb(if on {
                        theme.primary()
                    } else {
                        theme.background
                    }))
                    .text_color(rgb(if on { theme.foreground } else { theme.muted }))
                    .cursor_pointer()
                    .child(tab.label())
                    .on_click(cx.listener(move |this, _, _, cx| this.show_security_tab(tab, cx)))
            }));
        div()
            .flex_none()
            .px_4()
            .pt_2()
            .flex()
            .flex_col()
            .gap_2()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(crumbs.flex_1())
                    .child(stepper),
            )
            .child(
                div().px_1().flex().flex_col().gap_2().child(tags).child(
                    div()
                        .debug_selector(|| "security-page-title".into())
                        .text_size(px(look.ui.size + 4.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                ),
            )
            .child(div().px_1().pt_1().child(tabs))
    }

    fn security_failed(&self, look: &Look, error: String, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(|| "security-page-failed".into())
            .flex()
            .flex_col()
            .items_start()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(look.hue(3)))
                    .child(look.icon("icons/error.svg", 13., look.hue(3)))
                    .child(format!("GitHub would not give this page: {error}")),
            )
            .child(
                look.button("security-page-retry", "Try again", false)
                    .on_click(cx.listener(|this, _, _, cx| this.retry_security_page(cx))),
            )
    }
}

/// A colored word, such as a severity or state.
pub(super) fn tag(look: &Look, text: impl Into<SharedString>, color: u32) -> Div {
    look.tag(text, color)
}

pub(super) fn heading(text: impl Into<SharedString>) -> Div {
    div().font_weight(FontWeight::SEMIBOLD).child(text.into())
}

/// A tinted box that leads a page: what to do about the item.
pub(super) fn callout(
    look: &Look,
    hue: u32,
    mark: &'static str,
    title: Div,
    body: Div,
    actions: Div,
) -> Div {
    div()
        .debug_selector(|| "security-callout".into())
        .p_4()
        .flex()
        .flex_col()
        .gap_3()
        .rounded(px(corners::PANEL))
        .border_1()
        .border_color(rgb(mix(look.theme.background, hue, 45)))
        .bg(rgb(mix(look.theme.background, hue, 8)))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(look.icon(mark, 13., hue))
                .child(title),
        )
        .child(body)
        .child(actions.flex().flex_wrap().gap_2())
}

/// Label and value rows in one surface, for a page beside the list.
pub(super) fn facts_list(look: &Look, rows: Vec<(&'static str, AnyElement)>) -> Div {
    div()
        .debug_selector(|| "security-facts".into())
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .gap(px(6.))
        .rounded(px(corners::PANEL))
        .bg(rgb(look.theme.surface))
        .children(rows.into_iter().map(|(label, value)| {
            div()
                .flex()
                .items_start()
                .gap_3()
                .child(
                    div()
                        .flex_none()
                        .w(px(120.))
                        .text_size(look.small())
                        .text_color(rgb(look.theme.muted))
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(value))
        }))
}

/// The same facts as a column beside the page's text, once it is wide.
pub(super) fn facts_column(look: &Look, rows: Vec<(&'static str, AnyElement)>) -> Div {
    div()
        .debug_selector(|| "security-facts-column".into())
        .flex_none()
        .w(px(FACTS_WIDTH))
        .pl_4()
        .border_l_1()
        .border_color(rgb(look.theme.active))
        .flex()
        .flex_col()
        .gap_3()
        .children(rows.into_iter().map(|(label, value)| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(look.small())
                        .text_color(rgb(look.theme.muted))
                        .child(label),
                )
                .child(value)
        }))
}

/// A page's layout: the lead (callout and text) with its facts beside it
/// when wide, or the facts between the callout and the text when narrow.
pub(super) fn arrange(
    look: &Look,
    wide: bool,
    lead: Option<Div>,
    facts: Vec<(&'static str, AnyElement)>,
    text: Div,
) -> Div {
    if wide {
        div()
            .flex()
            .gap_4()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .children(lead)
                    .child(text),
            )
            .child(facts_column(look, facts))
    } else {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .children(lead)
            .child(facts_list(look, facts))
            .child(text)
    }
}

/// A Markdown body, or a quiet line while it is read or when it is empty.
pub(super) fn body(
    look: &Look,
    id: &'static str,
    lines: Option<&[crate::release_notes::Line]>,
    waiting: Div,
) -> Div {
    match lines {
        None => waiting,
        Some([]) => look.muted("No description."),
        Some(lines) => crate::release_notes::render(id, lines, &look.theme, &look.mono),
    }
}

/// "Oct 14, 2026 · 5 h ago".
pub(super) fn date(at: DateTime<Utc>) -> String {
    format!("{} · {}", at.format("%b %-d, %Y"), since(at))
}

pub(super) fn since(at: DateTime<Utc>) -> String {
    let elapsed = SystemTime::now()
        .duration_since(at.into())
        .unwrap_or_default();
    if elapsed.as_secs() >= 48 * 3600 {
        format!("{} days ago", elapsed.as_secs() / 86_400)
    } else {
        ago(elapsed)
    }
}

pub(super) fn severity_text(look: &Look, severity: Severity, cvss: Option<f64>) -> AnyElement {
    let text = match cvss {
        Some(score) => format!("{} · CVSS {score:.1}", severity.label()),
        None => severity.label().to_owned(),
    };
    div()
        .text_color(rgb(severity_color(&look.theme, severity)))
        .child(text)
        .into_any_element()
}

/// Untrusted text in the terminal font.
pub(super) fn mono(look: &Look, text: impl Into<SharedString>) -> AnyElement {
    look.mono(text).into_any_element()
}

pub(super) fn plain(text: impl Into<SharedString>) -> AnyElement {
    div().child(text.into()).into_any_element()
}

/// A link GitHub gave, drawn as text; a click opens it in a browser tab.
pub(super) fn link(
    look: &Look,
    id: impl Into<ElementId>,
    url: &str,
    cx: &mut Context<HerdrWindow>,
) -> Stateful<Div> {
    let target = url.to_owned();
    let hover = look.theme.active;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .rounded(px(corners::SMALL))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .text_color(rgb(look.theme.primary()))
        .child(look.icon("icons/external.svg", 11., look.theme.muted))
        .child(div().min_w_0().truncate().child(url.to_owned()))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_security_page(&target, window, cx);
        }))
}
