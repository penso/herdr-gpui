//! An item's status box, which leads its page the way a security alert's
//! callout does: one row per thing that decides what happens next (checks,
//! review, merge, blockers, the newest run), then the actions they call
//! for. Under it, the facts fold into one quiet line.

use super::{
    OrchestratorView,
    look::{GREEN, RED, YELLOW, agent_icon, ago},
    rows::{RunRow, Runs},
};
use crate::{
    config::corners,
    i18n::fl,
    orchestrator::{Access, Item},
    pull_request::{MergeState, Outcome, PullRequest, ReviewDecision},
};
use gpui::{prelude::*, *};

/// One row of the box: a colored mark, what it says, and a quieter detail.
pub(super) struct Row {
    pub(super) icon: &'static str,
    pub(super) hue: u32,
    pub(super) title: SharedString,
    pub(super) detail: Option<SharedString>,
}

impl OrchestratorView {
    pub(super) fn render_status_box(
        &self,
        item: &Item,
        runs: &Runs<'_>,
        item_runs: &[RunRow],
        cx: &mut Context<Self>,
    ) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let mut rows = Vec::new();
        if item.github_pull_request().is_some() {
            match self.open_pull_request() {
                Some(pr) => rows.extend(self.pull_request_rows(pr)),
                None => rows.push(Row {
                    icon: "icons/refresh.svg",
                    hue: theme.muted,
                    title: self.pull_request_waiting(item).into(),
                    detail: None,
                }),
            }
        }
        if !item.blocked_by.is_empty() {
            rows.push(Row {
                icon: "icons/error.svg",
                hue: look.hue(YELLOW),
                title: fl!("orchestrator-blocked").into(),
                detail: Some(
                    fl!(
                        "orchestrator-blocked-by",
                        items = item.blocked_by.join(", ")
                    )
                    .into(),
                ),
            });
        }
        rows.push(self.run_row(runs, item_runs.first().copied()));
        let actions = self.status_actions(item, runs, item_runs, cx);
        div()
            .debug_selector(|| "orchestrator-status".into())
            .flex_none()
            .flex()
            .flex_col()
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(theme.active))
            .overflow_hidden()
            .children(rows.into_iter().enumerate().map(|(index, row)| {
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(index > 0, |el| {
                        el.border_t_1().border_color(rgb(theme.active))
                    })
                    .child(look.icon(row.icon, 13., row.hue))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(row.title),
                    )
                    .children(
                        row.detail
                            .map(|detail| look.muted(detail).min_w_0().truncate()),
                    )
            }))
            .children(actions.map(|actions| {
                div()
                    .p_3()
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .bg(rgb(theme.surface))
                    .child(actions)
            }))
    }

    /// What stands in for a pull request's rows until GitHub answered.
    fn pull_request_waiting(&self, item: &Item) -> String {
        if self.request().is_none_or(|request| request.token.is_none()) {
            return fl!("orchestrator-status-sign-in");
        }
        self.lookup_problem(item)
            .unwrap_or_else(|| fl!("orchestrator-status-reading"))
    }

    pub(super) fn pull_request_rows(&self, pr: &PullRequest) -> [Row; 3] {
        let look = &self.look;
        let muted = look.theme.muted;
        let total = pr.check_list().count();
        let failing: Vec<&str> = pr
            .check_list()
            .filter(|(_, outcome)| *outcome == Outcome::Failed)
            .map(|(name, _)| name)
            .collect();
        let checks = match pr.checks_outcome() {
            Some(Outcome::Failed) => Row {
                icon: "icons/error.svg",
                hue: look.hue(RED),
                title: fl!(
                    "orchestrator-checks-failing",
                    failing = failing.len(),
                    total = total
                )
                .into(),
                detail: Some(failing.join(", ").into()),
            },
            Some(Outcome::Pending) => Row {
                icon: "icons/refresh.svg",
                hue: look.hue(YELLOW),
                title: fl!("orchestrator-checks-running").into(),
                detail: Some(pr.checks().into()),
            },
            Some(Outcome::Passed) => Row {
                icon: "icons/check.svg",
                hue: look.hue(GREEN),
                title: fl!("orchestrator-checks-passed").into(),
                detail: Some(pr.checks().into()),
            },
            Some(Outcome::Skipped) | None => Row {
                icon: "icons/pass.svg",
                hue: muted,
                title: fl!("orchestrator-checks-none").into(),
                detail: None,
            },
        };
        let (icon, hue) = match pr.review_decision {
            ReviewDecision::Approved => ("icons/check.svg", look.hue(GREEN)),
            ReviewDecision::ChangesRequested => ("icons/error.svg", look.hue(RED)),
            ReviewDecision::ReviewRequired => ("icons/user.svg", look.hue(YELLOW)),
            ReviewDecision::None => ("icons/user.svg", muted),
        };
        let review = Row {
            icon,
            hue,
            title: pr.review().into(),
            detail: None,
        };
        let (icon, hue) = match pr.merge_state_status {
            MergeState::Clean => ("icons/pass.svg", look.hue(GREEN)),
            MergeState::Dirty => ("icons/error.svg", look.hue(RED)),
            MergeState::Unknown => ("icons/pull-request.svg", muted),
            _ => ("icons/pull-request.svg", look.hue(YELLOW)),
        };
        let merge = Row {
            icon,
            hue,
            title: pr.merge_status().into(),
            detail: Some(
                fl!(
                    "orchestrator-merge-into",
                    state = pr.lifecycle(),
                    base = pr.base_ref_name.as_str()
                )
                .into(),
            ),
        };
        [checks, review, merge]
    }

    /// The newest run on the item, or that none has worked on it.
    fn run_row(&self, runs: &Runs<'_>, latest: Option<RunRow>) -> Row {
        let look = &self.look;
        let Some(latest) = latest else {
            return Row {
                icon: "icons/play.svg",
                hue: look.theme.muted,
                title: fl!("orchestrator-no-agent").into(),
                detail: None,
            };
        };
        let run = &runs.runs[latest.run];
        let workspace = run.workspace.as_ref();
        let host = workspace
            .and_then(|workspace| workspace.host.clone())
            .unwrap_or_else(|| fl!("orchestrator-host-local"));
        let detail = runs
            .live(latest.run)
            .and_then(|agent| agent.title.clone())
            .unwrap_or_else(|| match workspace {
                Some(workspace) if !workspace.branch.is_empty() => {
                    format!("{host} \u{00b7} {}", workspace.branch)
                }
                _ => host,
            });
        Row {
            icon: agent_icon(&run.agent),
            hue: look.status_hue(latest.status),
            title: fl!(
                "orchestrator-run-status",
                agent = run.agent.as_str(),
                status = latest.status.label()
            )
            .into(),
            detail: Some(detail.into()),
        }
    }

    /// The box's buttons, the merge methods under Merge once it is open.
    fn status_actions(
        &self,
        item: &Item,
        runs: &Runs<'_>,
        item_runs: &[RunRow],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let look = &self.look;
        let writable = self.snapshot().access == Some(Access::ReadWrite);
        let pull_request = item.pull_request.is_some();
        // A merge request has neither an agent to dispatch nor a `gh` review.
        let reviewable = item.github_pull_request().is_some();
        let live = item_runs
            .first()
            .filter(|latest| runs.live(latest.run).is_some())
            .map(|latest| latest.run);
        let mergeable = self.mergeable();
        let has_write_action = writable && (!pull_request || reviewable);
        if !has_write_action && live.is_none() && !mergeable {
            return None;
        }
        let key = item.key.canonical();
        let buttons = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .when(writable && !pull_request, |el| {
                el.child(
                    look.button(
                        "orchestrator-detail-dispatch",
                        fl!("orchestrator-dispatch-agent"),
                        item_runs.is_empty(),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_dispatch(key.clone(), window, cx);
                    })),
                )
            })
            .when(writable && reviewable, |el| {
                let key = item.key.canonical();
                el.child(
                    look.button(
                        "orchestrator-detail-review",
                        fl!("orchestrator-review-with-agent"),
                        true,
                    )
                    .debug_selector(|| "orchestrator-detail-review".into())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_review(key.clone(), window, cx);
                    })),
                )
            })
            .when_some(live, |el, run| {
                el.child(
                    look.button(
                        "orchestrator-open-run",
                        fl!("orchestrator-open-agent"),
                        false,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open_run(run, cx))),
                )
            })
            .when(mergeable, |el| {
                el.child(
                    look.button(
                        "orchestrator-detail-merge",
                        fl!("orchestrator-merge-menu"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.merge_open = !this.merge_open;
                        cx.notify();
                    })),
                )
            });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(buttons)
                .children(self.render_merge_methods(cx)),
        )
    }

    /// Who opened the item and when, its size, and its labels, in one line.
    pub(super) fn render_meta(&self, item: &Item) -> Div {
        let look = &self.look;
        let now = chrono::Utc::now();
        let mut parts = Vec::new();
        match (&item.author, item.created_at) {
            (Some(author), Some(_)) => parts.push(fl!(
                "orchestrator-opened-by",
                author = author.as_str(),
                ago = ago(item.created_at, now)
            )),
            (Some(author), None) => parts.push(author.clone()),
            (None, Some(_)) => {
                parts.push(fl!("orchestrator-opened", ago = ago(item.created_at, now)))
            }
            (None, None) => {}
        }
        if item.updated_at.is_some() {
            parts.push(fl!("orchestrator-updated", ago = ago(item.updated_at, now)));
        }
        if let Some(pr) = self.open_pull_request() {
            parts.push(fl!(
                "orchestrator-pull-request-size",
                additions = pr.additions,
                deletions = pr.deletions,
                files = pr.changed_files
            ));
        }
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(look.muted(parts.join(" \u{00b7} ")))
            .children(item.labels.iter().map(|label| look.chip(label.clone())))
    }
}
