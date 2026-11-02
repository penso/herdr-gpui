//! A private report's tabs: Report (that it is private, what to do, the
//! facts, and the reporter's text), Advisory draft (the packages, score,
//! weaknesses, and credits the draft holds), and Conversation.

use super::{
    Read, arrange, body, callout, date, facts_list, heading, mono, plain, severity_text, since,
};
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    orchestrator::Look,
    security::{
        Item, Repo, Report, Tab,
        page::{Comments, MAX_COMMENTS, ReportPage},
    },
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn security_report_body(
        &self,
        look: &Look,
        tab: Tab,
        repo: &Repo,
        report: &Report,
        page: Read<'_, ReportPage>,
        wide: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let url = repo.report_url(report);
        match tab {
            Tab::Draft => draft(look, report, page),
            Tab::Conversation => conversation(look, page, url, cx),
            _ => {
                let item = Item::Report {
                    repo: repo.name.clone(),
                    advisory: report.advisory.clone(),
                };
                let lead = private(look, item, url, cx);
                let text = div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(heading(fl!("security-report")))
                    .child(body(
                        look,
                        "security-report-description",
                        page.get().map(|page| page.description.as_slice()),
                        page.waiting(look),
                    ));
                arrange(
                    look,
                    wide,
                    Some(lead),
                    facts(look, report, page.get()),
                    text,
                )
            }
        }
    }
}

/// The callout: the report is not public yet, and what can be done here.
fn private(look: &Look, item: Item, url: Option<String>, cx: &mut Context<HerdrWindow>) -> Div {
    let actions = div()
        .child(
            look.button("security-report-agent", fl!("security-investigate"), true)
                .debug_selector(|| "security-report-agent".into())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.security_agent(&item, window, cx);
                })),
        )
        .child(div().flex_1())
        .when_some(url, |el, url| {
            el.child(
                look.button("security-report-triage", fl!("security-triage"), false)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_security_page(&url, window, cx);
                    })),
            )
        });
    callout(
        look,
        look.hue(3),
        "icons/lock.svg",
        heading(fl!("security-private")),
        look.muted(fl!("security-private-note")),
        actions,
    )
}

fn facts(look: &Look, report: &Report, page: Option<&ReportPage>) -> Vec<(String, AnyElement)> {
    let mut rows = Vec::new();
    if let Some(severity) = report.severity {
        let cvss = page
            .and_then(|page| page.cvss.as_ref())
            .map(|cvss| cvss.score);
        rows.push((
            fl!("security-fact-rating"),
            severity_text(look, severity, cvss),
        ));
    }
    if let Some(affected) = page.and_then(|page| page.affected.first()) {
        rows.push((
            fl!("security-fact-affected"),
            mono(
                look,
                format!("{} {}", affected.package, affected.vulnerable),
            ),
        ));
    }
    if let Some(weakness) = page.and_then(|page| page.weaknesses.first()) {
        rows.push((
            fl!("security-fact-weakness"),
            plain(format!("{} {}", weakness.id, weakness.name)),
        ));
    }
    rows.push((
        fl!("security-fact-draft"),
        mono(look, report.advisory.clone()),
    ));
    if let Some(reporter) = &report.reporter {
        rows.push((fl!("security-fact-reporter"), plain(reporter.clone())));
    }
    if let Some(at) = report.created_at {
        rows.push((fl!("security-fact-filed"), plain(date(at))));
    }
    rows
}

fn draft(look: &Look, report: &Report, page: Read<'_, ReportPage>) -> Div {
    let Some(page) = page.get() else {
        return page.waiting(look);
    };
    let mut rows = Vec::new();
    for affected in &page.affected {
        rows.push((
            fl!("security-fact-package"),
            mono(
                look,
                fl!(
                    "security-draft-package",
                    package = affected.package.as_str(),
                    ecosystem = affected.ecosystem.as_str(),
                    vulnerable = affected.vulnerable.as_str(),
                    fixed = if affected.patched.is_empty() {
                        "no"
                    } else {
                        "yes"
                    },
                    patched = affected.patched.as_str()
                ),
            ),
        ));
    }
    if let Some(cvss) = &page.cvss {
        if let Some(severity) = report.severity {
            rows.push((
                fl!("security-fact-score"),
                severity_text(look, severity, Some(cvss.score)),
            ));
        }
        rows.push((
            fl!("security-fact-vector"),
            mono(look, format!("CVSS {} {}", cvss.version, cvss.vector)),
        ));
    }
    for weakness in &page.weaknesses {
        rows.push((
            fl!("security-fact-weakness"),
            plain(format!("{} {}", weakness.id, weakness.name)),
        ));
    }
    for credit in &page.credits {
        rows.push((
            fl!("security-fact-credit"),
            plain(format!("{} ({})", credit.login, credit.kind)),
        ));
    }
    if rows.is_empty() {
        return look.muted(fl!("security-draft-empty"));
    }
    facts_list(look, rows)
}

fn conversation(
    look: &Look,
    page: Read<'_, ReportPage>,
    url: Option<String>,
    cx: &mut Context<HerdrWindow>,
) -> Div {
    let Some(page) = page.get() else {
        return page.waiting(look);
    };
    let on_github = |label: String| {
        url.clone().map(|url| {
            look.button("security-conversation-web", label, false)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_security_page(&url, window, cx);
                }))
        })
    };
    match &page.comments {
        Comments::None => div()
            .flex()
            .flex_col()
            .items_start()
            .gap_3()
            .child(look.muted(fl!("security-no-comments")))
            .children(on_github(fl!("security-reply"))),
        Comments::Unavailable => div()
            .debug_selector(|| "security-conversation-unavailable".into())
            .flex()
            .flex_col()
            .items_start()
            .gap_3()
            .child(look.muted(fl!("security-comments-unavailable")))
            .children(on_github(fl!("security-read-on-github"))),
        Comments::Read { comments, more } => {
            let theme = &look.theme;
            div()
                .debug_selector(|| "security-conversation".into())
                .flex()
                .flex_col()
                .gap_3()
                .children(comments.iter().enumerate().map(|(index, comment)| {
                    let who = [comment.author.clone(), comment.created_at.map(since)]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                    div()
                        .id(("security-comment", index))
                        .p_3()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .rounded(px(crate::config::corners::PANEL))
                        .bg(rgb(theme.surface))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(look.icon("icons/user.svg", 12., theme.muted))
                                .child(look.muted(who)),
                        )
                        .child(crate::release_notes::render(
                            "security-comment",
                            &comment.body,
                            theme,
                            &look.mono,
                        ))
                }))
                .when(*more, |el| {
                    el.child(look.muted(fl!("security-comments-more", count = MAX_COMMENTS)))
                })
                .children(on_github(fl!("security-reply")))
        }
    }
}
