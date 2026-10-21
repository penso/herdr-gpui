//! Drawing the Security section from the last read: the private
//! vulnerability reports in triage first, in one card across repositories,
//! then a card per repository with open alerts, then one line each for those
//! with none, those with Dependabot alerts off, those whose reports this
//! sign-in may not read, and those GitHub would not answer for.

use super::{Alert, Alerts, Item, Repo, Report, Reports, Severity};
use crate::{HerdrWindow, orchestrator::pane};
use gpui::{prelude::*, *};
use std::time::{Duration, SystemTime};

impl HerdrWindow {
    pub(crate) fn render_security_section(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let font = &self.config.ui;
        let small = px(font.size - 1.);
        let updated = self.security.read_at().map(|at| {
            format!(
                "Updated {}",
                ago(SystemTime::now().duration_since(at).unwrap_or_default())
            )
        });
        let header = div()
            .debug_selector(|| "security-section-header".into())
            .flex_none()
            .px_5()
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .text_size(px(font.size + 3.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Security"),
            )
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child("Private reports and Dependabot alerts"),
            )
            .child(div().flex_1())
            .children(updated.map(|updated| {
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(updated)
            }))
            .child(
                div()
                    .id("security-section-refresh")
                    .debug_selector(|| "security-section-refresh".into())
                    .px(px(10.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.active)))
                    .text_size(small)
                    .child(
                        svg()
                            .path("icons/refresh.svg")
                            .size(px(12.))
                            .text_color(rgb(theme.subtext())),
                    )
                    .child(if self.security.busy() {
                        "Updating…"
                    } else {
                        "Refresh"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.security.refresh();
                        this.poll_security(cx);
                        cx.notify();
                    })),
            );
        div()
            .id("security-section")
            .debug_selector(|| "security-section".into())
            .flex()
            .flex_col()
            .flex_1()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .text_size(px(font.size))
            .child(header)
            // The search narrows a list, so a page alone has none.
            .when(
                self.security.opened().is_none() || !self.security.list_hidden(),
                |el| el.child(self.render_security_toolbar(&self.security_look(), window, cx)),
            )
            .child(match self.security.opened().cloned() {
                // Like an issue's page: the list keeps the room and the page
                // sits in a pane its left edge resizes, or drags shut.
                Some(opened) if self.security.list_hidden() => div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_security_page(&opened, cx))
                    .into_any_element(),
                Some(opened) => div()
                    .id("security-split")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<pane::PaneDrag>, window, cx| {
                            match pane::Resize::of(event, window) {
                                pane::Resize::Close => this.close_security_item(cx),
                                pane::Resize::Width(width) => {
                                    this.security.pane_width = width;
                                    cx.notify();
                                }
                            }
                        },
                    ))
                    .child(self.render_security_list(&opened, cx))
                    .child(
                        div()
                            .debug_selector(|| "security-pane".into())
                            .flex_none()
                            .w(px(self.security.pane_width))
                            .h_full()
                            .flex()
                            .child(pane::handle(theme, "security-pane-handle"))
                            .child(self.render_security_page(&opened, cx)),
                    )
                    .into_any_element(),
                None => div()
                    .id("security-section-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.security.scroll)
                    .child(self.security_body(cx))
                    .into_any_element(),
            })
            .into_any_element()
    }

    fn security_body(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut body = div().p_5().flex().flex_col().gap_5();
        let repos = self.security.repos();
        if repos.is_empty() {
            let text = if self.security.busy() {
                "Reading the repositories…"
            } else if self.main_tokens().is_none() {
                "Sign in to GitHub in Herdr (account button, top right) to read security alerts and reports."
            } else {
                "No github.com repository is open in this window's workspaces."
            };
            return body.child(
                div()
                    .debug_selector(|| "security-section-empty".into())
                    .text_color(rgb(theme.muted))
                    .child(text),
            );
        }
        let narrowed = self.security_search.narrows();
        let reported = self.shown_reports();
        if !reported.is_empty() {
            // A queue longer than one page reads as "50+", not as 50.
            let more = !narrowed && repos.iter().any(Repo::more_reports);
            body = body.child(self.security_reported(&reported, more, cx));
        }
        let mut shown = !reported.is_empty();
        let mut quiet = Vec::new();
        let mut off = Vec::new();
        let mut hidden = Vec::new();
        let mut unreadable = Vec::new();
        let mut failed = Vec::new();
        for repo in repos {
            match &repo.reports {
                Ok(Reports::Hidden) => hidden.push(repo.name.as_str()),
                Err(error) => {
                    failed.push((repo.name.as_str(), format!("private reports: {error}")))
                }
                Ok(Reports::Off | Reports::Open { .. }) => {}
            }
        }
        for (index, repo) in repos.iter().enumerate() {
            match &repo.alerts {
                Ok(Alerts::Open { alerts, total }) if !alerts.is_empty() => {
                    let alerts = self.shown_alerts(repo);
                    if !alerts.is_empty() {
                        shown = true;
                        body = body.child(self.security_repo(index, repo, &alerts, *total, cx));
                    }
                }
                Ok(Alerts::Open { .. }) => quiet.push(repo.name.as_str()),
                Ok(Alerts::Off) => off.push(repo.name.as_str()),
                Ok(Alerts::Hidden) => unreadable.push(repo.name.as_str()),
                Err(error) => failed.push((repo.name.as_str(), error.to_string())),
            }
        }
        // What the search hides is no news about the rest.
        if narrowed {
            return body.when(!shown, |body| {
                body.child(note(
                    "security-section-no-match",
                    "Nothing matches the search and the pills picked.".into(),
                    theme.muted,
                ))
            });
        }
        if !quiet.is_empty() {
            body = body.child(note(
                "security-section-quiet",
                format!("No open alerts this account can see: {}", quiet.join(", ")),
                theme.muted,
            ));
        }
        if !off.is_empty() {
            body = body.child(note(
                "security-section-off",
                format!("Dependabot alerts are off: {}", off.join(", ")),
                theme.muted,
            ));
        }
        if !unreadable.is_empty() {
            body = body.child(note(
                "security-section-unreadable",
                format!(
                    "Dependabot alerts are not readable with this GitHub sign-in: {}",
                    unreadable.join(", ")
                ),
                theme.muted,
            ));
        }
        if !hidden.is_empty() {
            body = body.child(note(
                "security-section-hidden",
                format!(
                    "Private vulnerability reports are not readable with this GitHub sign-in: {}",
                    hidden.join(", ")
                ),
                theme.muted,
            ));
        }
        for (name, error) in failed {
            body = body.child(note(
                "security-section-failed",
                format!("{name}: {error}"),
                theme.ink(theme.palette[3]),
            ));
        }
        body
    }

    /// Reports outsiders filed, across repositories, in
    /// [`super::Security::reported`]'s order.
    fn security_reported(
        &self,
        reported: &[(usize, &Repo, &Report)],
        more: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let small = px(self.config.ui.size - 1.);
        let mut rows = div()
            .flex()
            .flex_col()
            .rounded(px(crate::config::corners::PANEL))
            .bg(rgb(theme.surface))
            .py_1();
        let now = SystemTime::now();
        let look = self.security_look();
        for &(index, repo, report) in reported {
            let (label, color) = match report.severity {
                Some(severity) => (severity.label(), severity_color(theme, severity)),
                None => ("Unrated", theme.muted),
            };
            let age = report
                .created_at
                .map(|at| ago(now.duration_since(at.into()).unwrap_or_default()));
            let detail = [
                Some(repo.name.clone()),
                Some(report.advisory.clone()).filter(|id| !id.is_empty()),
                report
                    .reporter
                    .as_ref()
                    .map(|login| format!("reported by {login}")),
                age,
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            let item = Item::Report {
                repo: repo.name.clone(),
                advisory: report.advisory.clone(),
            };
            let hover = theme.active;
            rows = rows.child(
                div()
                    .id(SharedString::from(format!(
                        "security-report-{index}-{}",
                        report.advisory
                    )))
                    .debug_selector(|| "security-report".into())
                    .px_3()
                    .py(px(6.))
                    .flex()
                    .items_start()
                    .gap_3()
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .child(
                        div()
                            .flex_none()
                            .w(px(72.))
                            .text_size(small)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color))
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().truncate().child(report.summary.clone()))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(small)
                                    .text_color(rgb(theme.muted))
                                    .child(detail),
                            ),
                    )
                    .child(self.security_start(
                        &look,
                        SharedString::from(format!("security-start-{index}-{}", report.advisory)),
                        item.clone(),
                        cx,
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_security_item(item.clone(), cx);
                    })),
            );
        }
        div()
            .debug_selector(|| "security-reported".into())
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        svg()
                            .path("icons/shield.svg")
                            .size(px(13.))
                            .text_color(rgb(severity_color(theme, Severity::Critical))),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Reported to you"),
                    )
                    .child(
                        div()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .child(format!(
                                "{}{} private vulnerability report{} in triage",
                                reported.len(),
                                if more { "+" } else { "" },
                                if reported.len() == 1 && !more {
                                    ""
                                } else {
                                    "s"
                                }
                            )),
                    ),
            )
            .child(rows)
    }

    fn security_repo(
        &self,
        index: usize,
        repo: &Repo,
        alerts: &[&Alert],
        total: u64,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let small = px(self.config.ui.size - 1.);
        let count = if total > alerts.len() as u64 {
            format!("{} of {total} open", alerts.len())
        } else {
            format!("{total} open")
        };
        let page = repo.alerts_url();
        let heading = div()
            .id(SharedString::from(format!("security-repo-{index}")))
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .child(
                svg()
                    .path("icons/shield.svg")
                    .size(px(13.))
                    .text_color(rgb(theme.subtext())),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(repo.name.clone()),
            )
            .child(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(format!("{} · {count}", repo.host)),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_security_page(&page, window, cx);
            }));
        let mut rows = div()
            .flex()
            .flex_col()
            .rounded(px(crate::config::corners::PANEL))
            .bg(rgb(theme.surface))
            .py_1();
        let look = self.security_look();
        for &alert in alerts {
            rows = rows.child(self.security_alert(&look, index, repo, alert, cx));
        }
        div()
            .debug_selector(move || format!("security-repo-{index}"))
            .flex()
            .flex_col()
            .gap_2()
            .child(heading)
            .child(rows)
    }

    fn security_alert(
        &self,
        look: &crate::orchestrator::Look,
        index: usize,
        repo: &Repo,
        alert: &Alert,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let small = px(self.config.ui.size - 1.);
        let color = severity_color(theme, alert.severity);
        let fix = alert.patched.as_ref().map_or_else(
            || "no fixed version yet".to_owned(),
            |version| format!("fixed in {version}"),
        );
        let detail = [alert.advisory.as_str(), alert.manifest.as_str(), &fix]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let item = Item::Alert {
            repo: repo.name.clone(),
            number: alert.number,
        };
        let hover = theme.active;
        div()
            .id(SharedString::from(format!(
                "security-alert-{index}-{}",
                alert.number
            )))
            .debug_selector(|| "security-alert".into())
            .px_3()
            .py(px(6.))
            .flex()
            .items_start()
            .gap_3()
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .child(
                div()
                    .flex_none()
                    .w(px(72.))
                    .text_size(small)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color))
                    .child(alert.severity.label()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(alert.package.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(small)
                                    .text_color(rgb(theme.muted))
                                    .child(alert.ecosystem.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(alert.summary.clone()),
                            ),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .child(detail),
                    ),
            )
            .child(self.security_start(
                look,
                SharedString::from(format!("security-start-{index}-{}", alert.number)),
                item.clone(),
                cx,
            ))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_security_item(item.clone(), cx);
            }))
    }
}

fn note(selector: &'static str, text: String, color: u32) -> Div {
    div()
        .debug_selector(move || selector.into())
        .text_color(rgb(color))
        .child(text)
}

/// Red for what needs fixing now, yellow for what can wait, as the
/// theme's own palette draws them.
pub(super) fn severity_color(theme: &crate::config::Theme, severity: Severity) -> u32 {
    match severity {
        Severity::Critical => theme.ink(theme.palette[1]),
        Severity::High => theme.ink(theme.palette[9]),
        Severity::Moderate => theme.ink(theme.palette[3]),
        Severity::Low => theme.muted,
    }
}

/// How long ago a read finished, to the minute.
pub(super) fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() / 60 {
        0 => "just now".into(),
        minutes @ 1..60 => format!("{minutes} min ago"),
        minutes => format!("{} h ago", minutes / 60),
    }
}
