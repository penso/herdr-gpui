//! A pull request's own pages: its conversation (read, commented on, and
//! merged through `pr_actions`, which sends each write once and never
//! retries it) and its checks, from the status the worker looked up by
//! number.

use super::{
    OrchestratorView,
    dispatch::Confirm,
    look::{BLUE, GREEN, MAGENTA, RED, YELLOW},
};
use crate::{
    config::corners,
    orchestrator::Item,
    pr_actions::{self, CommentKind, Review},
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

    pub(super) fn render_conversation(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let mut column = div()
            .id("orchestrator-conversation")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_2();
        if self.request.token.is_none() {
            column = column.child(look.muted("Sign in to GitHub to read the conversation."));
        } else if let Some(error) = self.pr.comments_error() {
            column = column.child(look.muted(error.to_owned()));
        } else if let Some(comments) = self.pr.comments() {
            if comments.is_empty() {
                column = column.child(look.muted("No comments yet."));
            }
            for (index, comment) in comments.iter().enumerate() {
                let badge = match &comment.kind {
                    CommentKind::Review(Review::Approved) => Some(("approved", GREEN)),
                    CommentKind::Review(Review::ChangesRequested) => {
                        Some(("changes requested", RED))
                    }
                    CommentKind::Review(_) => Some(("reviewed", BLUE)),
                    _ => None,
                };
                let path = match &comment.kind {
                    CommentKind::Thread { .. } => Some(comment.kind.label()),
                    _ => None,
                };
                column = column.child(
                    div()
                        .id(("orchestrator-comment", index))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .p_3()
                        .rounded(px(corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(look.icon("icons/user.svg", 13., theme.subtext()))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(comment.author.clone()),
                                )
                                .when_some(badge, |el, (text, hue)| el.child(look.badge(text, hue)))
                                .child(div().flex_1())
                                .child(look.muted(comment.created_at.clone())),
                        )
                        .when_some(path, |el, path| {
                            el.child(look.mono(path).text_color(rgb(look.hue(BLUE))))
                        })
                        .when(!comment.body.is_empty(), |el| {
                            el.child(
                                div()
                                    .text_color(rgb(theme.subtext()))
                                    .child(comment.body.clone()),
                            )
                        }),
                );
            }
        } else {
            column = column.child(look.muted("Reading the conversation\u{2026}"));
        }
        if let Some(outcome) = self.pr.outcome() {
            column = column.child(look.muted(outcome.message.clone()));
        }
        if let Some(error) = self.pr.error() {
            column = column.child(
                div()
                    .text_size(look.small())
                    .text_color(rgb(look.hue(RED)))
                    .child(error.to_owned()),
            );
        }
        let open = self
            .open_pull_request()
            .is_some_and(|pr| pr.state == State::Open);
        if open && self.pr.target().is_some() {
            let busy = self.pr.running().map(pr_actions::Action::running_label);
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pt_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .py(px(4.))
                            .rounded(px(corners::CONTROL))
                            .border_1()
                            .border_color(rgb(theme.active))
                            .child(self.comment.clone()),
                    )
                    .child(match busy {
                        Some(label) => look.muted(label).into_any_element(),
                        None => look
                            .button("orchestrator-comment-send", "Comment", false)
                            .on_click(cx.listener(|this, _, _, cx| this.post_comment(cx)))
                            .into_any_element(),
                    }),
            );
        }
        column.into_any_element()
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
