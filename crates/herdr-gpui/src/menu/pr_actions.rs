//! The Git popup's pull request pages: checks and the review conversation, a
//! comment dialog, and the merge dialog that chooses a method and confirms it.
//! Every request is started by the user here and run by `pr_actions`.
use super::{Page, accent, danger, git::Row};
use crate::{
    HerdrWindow,
    dialog_input::DialogInput,
    pr_actions::{Action, Target},
    pull_request::{MergeMethod, Outcome as CheckOutcome},
};
use gpui::{prelude::*, *};
use std::time::Instant;

impl HerdrWindow {
    /// Follow the focused branch's open pull request and drain the worker.
    /// A finished comment or merge changed the PR, so its lookup refreshes.
    pub(super) fn update_pr_actions(&mut self, now: Instant) -> bool {
        let target = self
            .git_open_pull_request()
            .and_then(|pr| Target::try_from(pr).ok());
        self.pr_actions.track(target);
        let mut changed = self.pr_actions.poll();
        if self.pr_actions.take_settled() {
            if self.menu.github.connected()
                && let Some(input) = self.git_input()
            {
                self.sync_pr_scope();
                self.menu.pr_cache.refresh(input, now);
            }
            changed = true;
        }
        changed
    }

    fn pr_token(&self) -> Option<std::sync::Arc<secrecy::SecretString>> {
        self.pr_profile().map(|profile| profile.token.clone())
    }

    pub(super) fn open_pr_review(&mut self) {
        self.menu.page = Some(Page::PrReview);
        self.menu.error = None;
        let result = self
            .pr_token()
            .ok_or(crate::Error::GitHubAuthentication)
            .and_then(|token| self.pr_actions.load_comments(token));
        if let Err(error) = result {
            self.menu.error = Some(error.to_string());
        }
    }

    pub(super) fn open_pr_comment(&mut self) {
        self.menu.page = Some(Page::PrComment);
        self.menu.error = None;
        self.menu.input = Some(DialogInput::default());
    }

    pub(super) fn open_pr_merge(&mut self) {
        let methods = self
            .git_open_pull_request()
            .map(|pr| pr.merge_methods.clone())
            .unwrap_or_default();
        self.menu.page = Some(Page::PrMerge);
        self.menu.error = None;
        self.menu.merge_method = methods.first().copied();
        self.menu.merge_target = self.pr_actions.target().cloned();
    }

    pub(super) fn submit_pr_comment(&mut self, cx: &mut Context<Self>) {
        if self
            .menu
            .input
            .as_ref()
            .is_some_and(|input| input.marked.is_some())
        {
            return;
        }
        let body = self
            .menu
            .input
            .as_ref()
            .map(|input| input.text.clone())
            .unwrap_or_default();
        let result = self
            .pr_token()
            .ok_or(crate::Error::GitHubAuthentication)
            .and_then(|token| self.pr_actions.start(Action::Comment(body), &[], token));
        match result {
            Ok(()) => {
                self.menu.error = None;
                self.menu.input = None;
                self.menu.page = Some(Page::Git);
            }
            Err(error) => self.menu.error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// The confirmation itself: only this button, or Enter on this dialog,
    /// sends a merge, and only for the head the dialog was opened with.
    pub(super) fn submit_pr_merge(&mut self, cx: &mut Context<Self>) {
        let result = (|| {
            let method = self.menu.merge_method.ok_or(crate::Error::PrMergeMethod)?;
            if self.menu.merge_target.is_none()
                || self.menu.merge_target.as_ref() != self.pr_actions.target()
            {
                return Err(crate::Error::PrMergeChanged);
            }
            let allowed = self
                .git_open_pull_request()
                .map(|pr| pr.merge_methods.clone())
                .unwrap_or_default();
            let token = self.pr_token().ok_or(crate::Error::GitHubAuthentication)?;
            self.pr_actions
                .start(Action::Merge(method), &allowed, token)
        })();
        match result {
            Ok(()) => {
                self.menu.error = None;
                self.menu.merge_target = None;
                self.menu.page = Some(Page::Git);
            }
            Err(error) => self.menu.error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// Up and down move between the methods the repository allows.
    pub(super) fn cycle_merge_method(&mut self, forward: bool, cx: &mut Context<Self>) {
        let methods = self
            .git_open_pull_request()
            .map(|pr| pr.merge_methods.clone())
            .unwrap_or_default();
        if methods.is_empty() {
            return;
        }
        let index = self
            .menu
            .merge_method
            .and_then(|method| methods.iter().position(|candidate| *candidate == method));
        let count = methods.len();
        let next = match (index, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
        };
        self.menu.merge_method = Some(methods[next]);
        cx.notify();
    }

    /// The popup's report of a running, finished, or refused PR action.
    pub(super) fn render_pr_action_status(&self, mut panel: Div, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        if let Some(action) = self.pr_actions.running() {
            panel = panel.child(
                div()
                    .debug_selector(|| "pr-action-running".into())
                    .p(px(8.))
                    .text_color(rgb(theme.ink(theme.palette[3])))
                    .child(action.running_label()),
            );
        }
        if let Some(outcome) = self.pr_actions.outcome() {
            panel = panel.child(
                div()
                    .debug_selector(|| "pr-action-outcome".into())
                    .p(px(8.))
                    .child(outcome.message.clone()),
            );
            if let Some(url) = outcome.url.clone() {
                panel = panel.child(
                    div()
                        .id("pr-action-open")
                        .px(px(8.))
                        .pb(px(8.))
                        .cursor_pointer()
                        .text_color(accent(theme))
                        .child("Open in browser")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            cx.open_url(&url);
                            this.dismiss_menu(window, cx);
                        })),
                );
            }
        }
        if let Some(error) = self.pr_actions.error() {
            panel = panel.child(
                div()
                    .debug_selector(|| "pr-action-error".into())
                    .p(px(8.))
                    .text_color(danger(theme))
                    .child(error.to_owned()),
            );
        }
        panel
    }

    /// A dialog's titled header: the row's icon, a title, and the PR.
    fn pr_dialog_header(&self, row: Row, title: &'static str) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        let subtitle = self
            .git_open_pull_request()
            .map(|pr| format!("#{} {}", pr.number, pr.title));
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .py(px(12.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                svg()
                    .path(row.icon())
                    .size(px(16.))
                    .flex_none()
                    .text_color(rgb(theme.muted)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(font.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .when_some(subtitle, |header, subtitle| {
                        header.child(
                            div()
                                .truncate()
                                .text_color(rgb(theme.muted))
                                .child(crate::sidebar::label_text(&subtitle)),
                        )
                    }),
            )
    }

    fn pr_dialog_error(&self) -> Option<Div> {
        let theme = &self.theme;
        self.menu.error.as_ref().map(|error| {
            div()
                .debug_selector(|| "pr-dialog-error".into())
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(theme.active))
                .px(px(10.))
                .py(px(6.))
                .text_color(danger(theme))
                .child(error.clone())
        })
    }

    fn pr_button(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        armed: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .px(px(12.))
            .py(px(6.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .cursor_pointer()
            .border_color(rgb(if armed {
                theme.foreground
            } else {
                theme.active
            }))
            .when(armed, |button| button.bg(rgb(theme.active)))
            .text_color(rgb(if armed { theme.foreground } else { theme.muted }))
            .hover(|button| button.bg(rgb(theme.active)))
            .child(label.into())
    }

    fn pr_footer(&self) -> Div {
        div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .px(px(16.))
            .py(px(12.))
            .border_t_1()
            .border_color(rgb(self.theme.active))
    }

    fn pr_back_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        self.pr_button("pr-dialog-back", "Back", false)
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.menu.error = None;
                this.menu.input = None;
                this.menu.merge_target = None;
                this.menu.page = Some(Page::Git);
                cx.notify();
            }))
    }

    pub(super) fn render_pr_review(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut body = div()
            .id("pr-review-body")
            .debug_selector(|| "pr-review-body".into())
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(16.))
            .py(px(12.))
            .max_h(px(380.))
            .overflow_y_scroll();
        let heading = |label: &'static str| {
            div()
                .pt(px(4.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(label)
        };
        body = body.child(heading("Checks"));
        let pr = self.git_open_pull_request();
        let mut checks = pr.into_iter().flat_map(|pr| pr.check_list()).peekable();
        if checks.peek().is_none() {
            body = body.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("No checks reported"),
            );
        }
        for (name, outcome) in checks {
            let color = match outcome {
                CheckOutcome::Passed => theme.palette[2],
                CheckOutcome::Failed => theme.palette[1],
                CheckOutcome::Pending => theme.palette[3],
                CheckOutcome::Skipped => theme.muted,
            };
            body =
                body.child(
                    div()
                        .debug_selector(|| "pr-review-check".into())
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .size(px(8.))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(theme.ink(color))),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(
                            crate::sidebar::label_text(if name.is_empty() {
                                "Unnamed check"
                            } else {
                                name
                            }),
                        ))
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.muted))
                                .child(outcome.to_string()),
                        ),
                );
        }
        if let Some(summary) = pr.map(|pr| pr.checks_summary.clone()) {
            body = body.child(div().text_color(rgb(theme.muted)).child(summary));
        }
        body = body.child(heading("Comments"));
        if self.pr_actions.loading() {
            body = body.child(
                div()
                    .debug_selector(|| "pr-review-loading".into())
                    .text_color(rgb(theme.muted))
                    .child("Loading comments..."),
            );
        } else if let Some(error) = self.pr_actions.comments_error() {
            body = body.child(div().text_color(danger(theme)).child(error.to_owned()));
        } else if let Some(comments) = self.pr_actions.comments() {
            if comments.is_empty() {
                body = body.child(div().text_color(rgb(theme.muted)).child("No comments yet"));
            }
            for comment in comments {
                body = body.child(
                    div()
                        .debug_selector(|| "pr-review-comment".into())
                        .pt(px(6.))
                        .flex()
                        .flex_col()
                        .child(div().truncate().text_color(rgb(theme.muted)).child(
                            crate::sidebar::label_text(&format!(
                                "{} {}",
                                comment.author,
                                comment.kind.label()
                            )),
                        ))
                        .when(!comment.body.is_empty(), |entry| {
                            entry.child(div().child(comment.body.clone()))
                        }),
                );
            }
        }
        if let Some(error) = self.pr_dialog_error() {
            body = body.child(error);
        }
        div()
            .flex()
            .flex_col()
            .child(self.pr_dialog_header(Row::Review, "Checks and comments"))
            .child(body)
            .child(
                self.pr_footer().child(self.pr_back_button(cx)).child(
                    self.pr_button("pr-review-comment-button", "Comment...", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_pr_comment();
                            cx.notify();
                        })),
                ),
            )
    }

    pub(super) fn render_pr_comment(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let armed = self
            .menu
            .input
            .as_ref()
            .is_some_and(|input| !input.text.trim().is_empty());
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .px(px(16.))
            .py(px(12.))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("Posts to the pull request's conversation as your GitHub account."),
            );
        if self.menu.input.is_some() {
            body = body.child(self.render_dialog_input(cx));
        }
        if let Some(error) = self.pr_dialog_error() {
            body = body.child(error);
        }
        div()
            .flex()
            .flex_col()
            .child(self.pr_dialog_header(Row::Comment, "Comment"))
            .child(body)
            .child(
                self.pr_footer().child(self.pr_back_button(cx)).child(
                    self.pr_button("pr-comment-submit", "Comment", armed)
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.submit_pr_comment(cx);
                        })),
                ),
            )
    }

    pub(super) fn render_pr_merge(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let font = &self.config.ui;
        let pr = self.git_open_pull_request();
        let mut body = div().flex().flex_col().gap(px(8.)).px(px(16.)).py(px(12.));
        if let Some(pr) = pr {
            body = body.child(
                div()
                    .debug_selector(|| "pr-merge-branches".into())
                    .text_color(rgb(theme.muted))
                    .child(crate::sidebar::label_text(&format!(
                        "Merge {} into {}",
                        pr.head_ref_name, pr.base_ref_name
                    ))),
            );
            // GitHub has the final word; this is what it last reported.
            for (heading, label) in [
                ("Review", pr.review().to_owned()),
                ("Merge", pr.merge_status().to_owned()),
                ("Checks", pr.checks_summary.clone()),
            ] {
                body = body.child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .child(
                            div()
                                .w(px(56.))
                                .flex_none()
                                .text_color(rgb(theme.muted))
                                .child(heading),
                        )
                        .child(div().flex_1().min_w_0().child(label)),
                );
            }
            body = body.child(
                div()
                    .pt(px(4.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Method"),
            );
            for method in pr.merge_methods.iter().copied() {
                let selected = self.menu.merge_method == Some(method);
                body = body.child(
                    div()
                        .id(SharedString::from(method.graphql()))
                        .debug_selector(move || format!("pr-merge-method-{}", method.graphql()))
                        .min_h(px(font.line_height() + 10.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .cursor_pointer()
                        .when(selected, |row| row.bg(rgb(theme.active)))
                        .hover(|row| row.bg(rgb(theme.active)))
                        .child(
                            div()
                                .size(px(8.))
                                .flex_none()
                                .rounded_full()
                                .border_1()
                                .border_color(rgb(theme.foreground))
                                .when(selected, |dot| dot.bg(rgb(theme.foreground))),
                        )
                        .child(method.action())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.menu.merge_method = Some(method);
                            cx.notify();
                        })),
                );
            }
        }
        if let Some(error) = self.pr_dialog_error() {
            body = body.child(error);
        }
        let armed = self.menu.merge_method.is_some() && self.pr_actions.running().is_none();
        let label = self.menu.merge_method.map_or("Merge", MergeMethod::action);
        div()
            .flex()
            .flex_col()
            .child(self.pr_dialog_header(Row::Merge, "Merge pull request"))
            .child(body)
            .child(
                self.pr_footer().child(self.pr_back_button(cx)).child(
                    self.pr_button("pr-merge-confirm", label, armed)
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.submit_pr_merge(cx);
                        })),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{Page, Row};
    use crate::{pr_actions::Action, pull_request::MergeMethod, sidebar::layout_tests::REPO_KEY};
    use gpui::{Entity, TestAppContext, VisualTestContext, point, px, size};

    fn input() -> crate::pull_request::Input {
        crate::pull_request::Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "develop".into(),
        }
    }

    /// A focused checkout with an open PR in the cache, tracked for actions,
    /// and no worker: nothing here reaches GitHub.
    fn open(
        cx: &mut TestAppContext,
        pr: crate::pull_request::PullRequest,
    ) -> (Entity<crate::HerdrWindow>, &mut VisualTestContext) {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(900.), px(700.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.git = crate::git::Git::fixture(input(), Default::default());
                view.menu.github = crate::github::Auth::connected_fixture();
                view.menu
                    .pr_cache
                    .seed(input(), pr, std::time::Instant::now());
                view.pr_actions.track(
                    view.git_open_pull_request()
                        .and_then(|pr| crate::pr_actions::Target::try_from(pr).ok()),
                );
                view.open_git_menu(point(px(860.), px(20.)), window, cx);
            })
        });
        (view, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
    }

    fn labels(view: &crate::HerdrWindow) -> Vec<String> {
        view.git_rows()
            .into_iter()
            .map(|(_, label)| label)
            .collect()
    }

    #[gpui::test]
    fn an_open_pull_request_offers_review_comment_and_merge(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, crate::pull_request::fixture().unwrap());
        cx.update(|_, cx| {
            assert_eq!(
                labels(view.read(cx)),
                [
                    "Commit...",
                    "Push",
                    "Open pull request #8",
                    "Checks and comments",
                    "Comment...",
                    "Merge..."
                ]
            );
        });
        // A draft cannot be merged; a repository allowing nothing offers nothing.
        for change in [
            |pr: &mut crate::pull_request::PullRequest| pr.is_draft = true,
            |pr: &mut crate::pull_request::PullRequest| pr.merge_methods.clear(),
        ] {
            let mut pr = crate::pull_request::fixture().unwrap();
            change(&mut pr);
            cx.update(|_, cx| {
                view.update(cx, |view, _| {
                    view.menu
                        .pr_cache
                        .seed(input(), pr, std::time::Instant::now());
                    let labels = labels(view);
                    assert!(labels.contains(&"Comment...".to_owned()));
                    assert!(!labels.contains(&"Merge...".to_owned()));
                })
            });
        }
        // A PR without its node identity cannot be acted on at all.
        let mut pr = crate::pull_request::fixture().unwrap();
        pr.id.clear();
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.menu
                    .pr_cache
                    .seed(input(), pr, std::time::Instant::now());
                view.update_pr_actions(std::time::Instant::now());
                assert_eq!(labels(view).len(), 3);
            })
        });
    }

    #[gpui::test]
    fn merging_takes_a_method_and_an_explicit_confirmation(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, crate::pull_request::fixture().unwrap());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.activate_git_row(Row::Merge, window, cx);
                assert_eq!(view.menu.page, Some(Page::PrMerge));
                assert_eq!(view.menu.merge_method, Some(MergeMethod::Merge));
                assert!(
                    view.pr_actions.running().is_none(),
                    "opening the dialog sends nothing"
                );
            })
        });
        draw(cx);
        let confirm = cx.debug_bounds("pr-merge-confirm").unwrap();
        let rebase = cx.debug_bounds("pr-merge-method-REBASE").unwrap();
        let panel = cx.debug_bounds("menu-panel").unwrap();
        assert!(rebase.bottom() <= confirm.top());
        assert!(confirm.right() <= panel.right() && confirm.bottom() <= panel.bottom());
        cx.simulate_keystrokes("down");
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).menu.merge_method, Some(MergeMethod::Squash));
        });
        cx.simulate_click(rebase.center(), Default::default());
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).menu.merge_method, Some(MergeMethod::Rebase));
            assert!(view.read(cx).pr_actions.running().is_none());
        });
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(
                view.pr_actions.running(),
                Some(&Action::Merge(MergeMethod::Rebase))
            );
            assert_eq!(view.menu.page, Some(Page::Git));
        });
        draw(cx);
        assert!(cx.debug_bounds("pr-action-running").is_some());
        // Nothing else starts while the merge runs.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.activate_git_row(Row::Comment, window, cx);
                assert_eq!(view.menu.page, Some(Page::Git));
            })
        });
    }

    #[gpui::test]
    fn a_branch_that_moved_after_the_dialog_opened_is_not_merged(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, crate::pull_request::fixture().unwrap());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.activate_git_row(Row::Merge, window, cx);
                let mut pushed = crate::pull_request::fixture().unwrap();
                pushed.head_ref_oid = "f".repeat(40);
                view.menu
                    .pr_cache
                    .seed(input(), pushed, std::time::Instant::now());
                view.update_pr_actions(std::time::Instant::now());
                view.submit_pr_merge(cx);
                assert!(view.pr_actions.running().is_none());
                assert_eq!(view.menu.page, Some(Page::PrMerge));
                assert_eq!(
                    view.menu.error.as_deref(),
                    Some(crate::Error::PrMergeChanged.to_string().as_str())
                );
            })
        });
    }

    #[gpui::test]
    fn commenting_refuses_an_empty_body_and_queues_a_written_one(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, crate::pull_request::fixture().unwrap());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.activate_git_row(Row::Comment, window, cx);
                assert_eq!(view.menu.page, Some(Page::PrComment));
                view.submit_pr_comment(cx);
                assert_eq!(
                    view.menu.error.as_deref(),
                    Some(crate::Error::PrCommentBody.to_string().as_str())
                );
                assert!(view.pr_actions.running().is_none());
                if let Some(dialog) = view.menu.input.as_mut() {
                    dialog.text = "Thanks, merging after CI".into();
                }
                cx.notify();
            })
        });
        draw(cx);
        let field = cx.debug_bounds("dialog-input").unwrap();
        let submit = cx.debug_bounds("pr-comment-submit").unwrap();
        assert!(field.bottom() <= submit.top());
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert_eq!(
                view.pr_actions.running(),
                Some(&Action::Comment("Thanks, merging after CI".into()))
            );
            assert_eq!(view.menu.page, Some(Page::Git));
            assert!(view.menu.input.is_none());
        });
    }

    #[gpui::test]
    fn the_review_page_lists_each_check_within_the_window(cx: &mut TestAppContext) {
        let (view, cx) = open(cx, crate::pull_request::fixture().unwrap());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.activate_git_row(Row::Review, window, cx);
                assert_eq!(view.menu.page, Some(Page::PrReview));
                assert!(view.pr_actions.loading(), "opening it asks for comments");
            })
        });
        for (width, height) in [(900., 700.), (360., 420.)] {
            cx.simulate_resize(size(px(width), px(height)));
            draw(cx);
            let panel = cx.debug_bounds("menu-panel").unwrap();
            assert!(panel.left() >= px(0.) && panel.right() <= px(width));
            assert!(panel.bottom() <= px(height));
            assert!(cx.debug_bounds("pr-review-check").is_some());
            assert!(cx.debug_bounds("pr-review-loading").is_some());
            let back = cx.debug_bounds("pr-dialog-back").unwrap();
            assert!(back.bottom() <= panel.bottom(), "{back:?} {panel:?}");
        }
        let checks: Vec<_> = cx.update(|_, cx| {
            view.read(cx)
                .git_open_pull_request()
                .unwrap()
                .check_list()
                .map(|(name, outcome)| format!("{name} {outcome}"))
                .collect()
        });
        assert_eq!(checks, ["test passed", "ci/lint failed", "build pending"]);
    }
}
