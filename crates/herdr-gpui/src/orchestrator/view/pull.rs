//! A pull request's own pages: its conversation (drawn by `conversation`,
//! commented on and merged through `pr_actions`, which sends each write once
//! and never retries it) and its checks, from the status the worker looked
//! up by number.

use super::{
    OrchestratorView,
    dispatch::Confirm,
    look::{GREEN, MAGENTA, RED, YELLOW},
};
use crate::{
    config::corners,
    orchestrator::Item,
    pr_actions,
    pull_request::{MergeMethod, Outcome, PullRequest, State},
};
use gpui::{prelude::*, *};

impl OrchestratorView {
    /// The looked-up status of the open pull request, once it arrived.
    pub(super) fn open_pull_request(&self) -> Option<&PullRequest> {
        let number = self
            .item(&self.detail.as_ref()?.key)?
            .pull_request
            .as_ref()?
            .number;
        let lookup = self.snapshot.pull_request.as_ref()?;
        if lookup.number != number {
            return None;
        }
        lookup.result.as_ref().as_ref().ok()?.as_ref()
    }

    /// Follows the open pull request, and reads its conversation when the
    /// Conversation page shows and nothing was read yet.
    pub(super) fn follow_pull_request(&mut self) {
        let target = self
            .open_pull_request()
            .and_then(|pr| pr_actions::Target::try_from(pr).ok());
        self.pr.track(target);
        let wants =
            matches!(&self.detail, Some(detail) if detail.tab == super::DetailTab::Conversation);
        if wants
            && self.pr.target().is_some()
            && self.pr.comments().is_none()
            && !self.pr.loading()
            && self.pr.comments_error().is_none()
            && self.pr.running().is_none()
            && let Some(token) = self.request.token.clone()
        {
            let _ = self.pr.load_comments(token);
        }
    }

    pub(super) fn post_comment(&mut self, cx: &mut Context<Self>) {
        let text = self.comment.read(cx).text().trim().to_owned();
        let (Some(token), Some(pr)) = (self.request.token.clone(), self.open_pull_request()) else {
            return;
        };
        let methods = pr.merge_methods.clone();
        if text.is_empty() {
            return;
        }
        if self
            .pr
            .start(pr_actions::Action::Comment(text), &methods, token)
            .is_ok()
        {
            self.comment.update(cx, |input, cx| input.clear(cx));
        }
        cx.notify();
    }

    /// Merges the open pull request at the head the user saw.
    /// Merges `expected`, the pull request and head commit the user confirmed.
    /// A refresh that moved the head, or another pull request opened since,
    /// refuses the merge instead of naming a commit the user never saw.
    pub(super) fn merge(
        &mut self,
        method: MergeMethod,
        expected: &pr_actions::Target,
        cx: &mut Context<Self>,
    ) {
        if self.pr.target() != Some(expected) {
            self.notice = Some(crate::orchestrator::Notice {
                outcome: Err(std::sync::Arc::new(
                    crate::orchestrator::Error::PullRequestChanged,
                )),
            });
            cx.notify();
            return;
        }
        let (Some(token), Some(pr)) = (self.request.token.clone(), self.open_pull_request()) else {
            return;
        };
        let methods = pr.merge_methods.clone();
        let _ = self
            .pr
            .start(pr_actions::Action::Merge(method), &methods, token);
        cx.notify();
    }

    pub(super) fn render_checks(&self, item: &Item) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let mut column = div()
            .id("orchestrator-checks")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_1();
        let Some(pr) = self.open_pull_request() else {
            let text = match self.snapshot.pull_request.as_ref() {
                _ if self.request.token.is_none() => {
                    "Sign in to GitHub to read the checks.".to_owned()
                }
                Some(lookup)
                    if Some(lookup.number) == item.pull_request.as_ref().map(|pr| pr.number) =>
                {
                    match lookup.result.as_ref() {
                        Err(error) => error.to_string(),
                        Ok(_) => "GitHub reports no such pull request.".to_owned(),
                    }
                }
                _ => "Reading the checks\u{2026}".to_owned(),
            };
            return column.child(look.muted(text)).into_any_element();
        };
        column = column
            .child(look.label(format!(
                "CHECKS \u{00b7} {}",
                pr.checks_summary.to_uppercase()
            )))
            .children(pr.check_list().enumerate().map(|(index, (name, outcome))| {
                let (mark, hue) = outcome_mark(outcome);
                div()
                    .id(("orchestrator-check", index))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(look.small())
                    .child(div().w(px(14.)).text_color(rgb(look.hue(hue))).child(mark))
                    .child(div().flex_1().min_w_0().truncate().child(name.to_owned()))
                    .child(look.muted(outcome.to_string()))
            }))
            .child(div().h_3())
            .child(look.label("REVIEW"))
            .child(div().text_size(look.small()).child(pr.review()))
            .child(div().h_3())
            .child(look.label("MERGE"))
            .child(
                div()
                    .text_size(look.small())
                    .text_color(rgb(theme.subtext()))
                    .child(format!("{} \u{00b7} {}", pr.lifecycle(), pr.merge_status())),
            );
        column.into_any_element()
    }

    /// The merge methods GitHub allows here, as a small menu under the button.
    pub(super) fn render_merge_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.merge_open {
            return None;
        }
        let pr = self.open_pull_request()?;
        let look = &self.look;
        let theme = &look.theme;
        Some(
            deferred(
                div()
                    .absolute()
                    .top(px(96.))
                    .right(px(16.))
                    .w(px(220.))
                    .p_1()
                    .flex()
                    .flex_col()
                    .rounded(px(corners::CONTROL))
                    .bg(rgb(theme.surface))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .shadow_lg()
                    .children(pr.merge_methods.iter().copied().map(|method| {
                        let hover = theme.active;
                        div()
                            .id(SharedString::from(format!(
                                "orchestrator-merge-{}",
                                method.graphql()
                            )))
                            .px_2()
                            .h(px(28.))
                            .flex()
                            .items_center()
                            .rounded(px(corners::SMALL))
                            .cursor_pointer()
                            .hover(move |style| style.bg(rgb(hover)))
                            .child(method.action())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.merge_open = false;
                                // The confirmation names what the menu showed.
                                if let Some(target) = this.pr.target().cloned() {
                                    this.ask(Confirm::Merge { method, target }, cx);
                                }
                            }))
                    }))
                    .when(pr.merge_methods.is_empty(), |el| {
                        el.child(look.muted("This repository allows no merge method."))
                    }),
            )
            .into_any_element(),
        )
    }

    /// Whether the open pull request can be merged from here.
    pub(super) fn mergeable(&self) -> bool {
        self.open_pull_request()
            .is_some_and(|pr| pr.state == State::Open && !pr.is_draft)
            && self.pr.target().is_some()
            && self.pr.running().is_none()
    }
}

fn outcome_mark(outcome: Outcome) -> (&'static str, usize) {
    match outcome {
        Outcome::Passed => ("\u{2713}", GREEN),
        Outcome::Failed => ("\u{2717}", RED),
        Outcome::Pending => ("\u{25cf}", YELLOW),
        Outcome::Skipped => ("\u{2013}", MAGENTA),
    }
}
