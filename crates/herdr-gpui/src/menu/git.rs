//! The titlebar's Git actions popup: commit, push, and pull request creation
//! for the focused local checkout, as a kit popup menu whose status rows read
//! the window's live state.
use super::{Page, dialog_buttons, error_alert, listener, pr::pr_color, submit};
use crate::{
    HerdrWindow,
    git::{Action, Status},
    pull_request::State as PrState,
};
use gpui_kit::{
    component::{
        ActiveTheme as _, Icon, IconName,
        button::{Button, ButtonVariants as _},
        dialog::Dialog,
        h_flex,
        input::Input,
        menu::PopupMenuItem,
        tag::Tag,
        v_flex,
    },
    prelude::*,
    *,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Commit,
    Push,
    PullRequest,
}

impl Row {
    fn icon(self) -> Icon {
        match self {
            Self::Commit => Icon::default().path("icons/pencil.svg"),
            Self::Push => Icon::new(IconName::ArrowUp),
            Self::PullRequest => Icon::default().path("icons/git-branch.svg"),
        }
    }
}

/// Stats as the titlebar and the popup header both show them.
pub(super) fn summary(status: Option<Status>) -> String {
    let Some(status) = status else {
        return "Checking working tree...".into();
    };
    if !status.dirty() {
        return "No uncommitted changes".into();
    }
    let mut parts = vec![format!("+{} -{}", status.additions, status.deletions)];
    if status.untracked > 0 {
        parts.push(format!(
            "{} untracked {}",
            status.untracked,
            if status.untracked == 1 {
                "entry"
            } else {
                "entries"
            }
        ));
    }
    parts.join(", ")
}

impl HerdrWindow {
    /// Follow the focused checkout and drain worker results. Called from the
    /// window's poll task, never from a render or input path.
    pub(crate) fn update_git(&mut self) -> bool {
        let now = std::time::Instant::now();
        let mut changed = self.git.track(self.git_input(), self.active, now);
        self.git
            .track_listed(self.listed_git_inputs(), self.active, now);
        changed |= self.git.poll(now);
        changed
    }

    /// Every listed local checkout, so the sidebar can mark the ones holding
    /// uncommitted work. Empty when the endpoint is not the owned local daemon.
    fn listed_git_inputs(&self) -> Vec<crate::pull_request::Input> {
        if !self.local_git_endpoint() {
            return Vec::new();
        }
        self.live
            .snapshot
            .iter()
            .flat_map(|snapshot| snapshot.workspaces.iter())
            .filter_map(|workspace| {
                crate::pull_request::repository_input(
                    workspace.worktree.as_ref(),
                    workspace.branch.as_deref(),
                )
                .ok()
            })
            .collect()
    }

    /// Local, owned daemon sockets only: the same trust boundary the PR lookup
    /// uses, because both run Git against the user's own checkouts.
    fn local_git_endpoint(&self) -> bool {
        self.selected_endpoint == 0
            && self.live.local_daemon_peer
            && self.live.status.is_connected()
    }

    /// The checkout the chrome acts on: the focused workspace's, when it is one
    /// this client may run Git in.
    fn git_input(&self) -> Option<crate::pull_request::Input> {
        if !self.local_git_endpoint() {
            return None;
        }
        let snapshot = self.live.snapshot.as_ref()?;
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| {
                Some(workspace.workspace_id.as_str()) == snapshot.focused_workspace_id.as_deref()
            })
            .or_else(|| {
                snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.focused)
            })?;
        crate::pull_request::repository_input(
            workspace.worktree.as_ref(),
            workspace.branch.as_deref(),
        )
        .ok()
    }

    pub(crate) fn open_git_menu(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_is_open() {
            self.close_menu(window, cx);
            return;
        }
        self.show_git_menu(anchor, window, cx);
    }

    /// The popup, rebuilt from the current rows. Its status rows render the
    /// window's live Git state, so a running action reports as it goes.
    fn show_git_menu(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git.tracked().is_none() || !self.begin_menu(window, cx) {
            return;
        }
        if self.menu.github.connected()
            && let Some(input) = self.git_input()
        {
            self.sync_pr_scope();
            self.menu.pr_cache.refresh(input, std::time::Instant::now());
        }
        let anchor = point(
            anchor.x,
            px(crate::titlebar::HEIGHT
                + crate::worktree_banner::reserved(env!("HERDR_BUILD_WORKTREE") == "1")
                + 6.),
        );
        let weak = cx.weak_entity();
        let rows = self.git_rows();
        let running = self.git.running().is_some();
        let branch = self.git.tracked().map(|input| input.branch.clone());
        let pr_url = self.git_pull_request().map(|pr| pr.url.clone());
        self.show_popup(Page::Git, anchor, window, cx, move |menu, _, _| {
            let menu = menu
                .min_w(px(300.))
                .max_w(px(360.))
                .when_some(branch, |menu, branch| menu.label(branch));
            let status = weak.clone();
            let menu = match pr_url {
                Some(url) => {
                    let pr = weak.clone();
                    menu.item(
                        PopupMenuItem::element(move |_, cx| {
                            pr.upgrade()
                                .map(|view| view.read(cx).render_git_pr(cx))
                                .unwrap_or_else(|| div().into_any_element())
                        })
                        .on_click(listener(
                            &weak,
                            move |this, window, cx| {
                                cx.open_url(&url);
                                this.dismiss_menu(window, cx);
                            },
                        )),
                    )
                }
                None => menu,
            };
            let menu = menu.item(
                PopupMenuItem::element(move |_, cx| {
                    status
                        .upgrade()
                        .map(|view| view.read(cx).render_git_status(cx))
                        .unwrap_or_else(|| div().into_any_element())
                })
                .disabled(true),
            );
            rows.into_iter()
                .fold(menu.separator(), |menu, (row, label)| {
                    menu.item(
                        PopupMenuItem::new(label)
                            .icon(row.icon())
                            .disabled(running)
                            .on_click(listener(&weak, move |this, window, cx| {
                                this.activate_git_row(row, window, cx)
                            })),
                    )
                })
        });
    }

    /// The pull request already prefetched for the focused branch, in whatever
    /// lifecycle it is in. Reading the cache never schedules work.
    pub(crate) fn git_pull_request(&self) -> Option<&crate::pull_request::PullRequest> {
        let input = self.git.tracked()?;
        self.menu
            .github
            .connected()
            .then(|| self.menu.pr_cache.peek(&input.repo_key, &input.branch))
            .flatten()
    }

    /// Only an open pull request can be opened; a merged or closed one leaves
    /// creating the next one as the action.
    fn git_open_pull_request(&self) -> Option<&crate::pull_request::PullRequest> {
        self.git_pull_request()
            .filter(|pr| pr.state == PrState::Open)
    }

    pub(super) fn git_rows(&self) -> Vec<(Row, String)> {
        if self.git.tracked().is_none() {
            return Vec::new();
        }
        vec![
            (Row::Commit, "Commit...".into()),
            (Row::Push, "Push".into()),
            match self.git_open_pull_request() {
                Some(pr) => (
                    Row::PullRequest,
                    format!("Open pull request #{}", pr.number),
                ),
                None => (Row::PullRequest, "Create pull request".into()),
            },
        ]
    }

    pub(super) fn activate_git_row(
        &mut self,
        row: Row,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git.running().is_some() {
            return;
        }
        let anchor = self.menu.anchor;
        match row {
            Row::Commit => {
                self.set_menu_input("", "Commit message", window, cx);
                self.show_dialog(Page::GitCommit, window, cx, |this, dialog, weak, _, cx| {
                    this.git_commit_dialog(dialog, weak, cx)
                });
                self.focus_menu_input(window, cx);
                return;
            }
            Row::Push => self.start_git(Action::Push),
            Row::PullRequest => {
                if let Some(url) = self.git_open_pull_request().map(|pr| pr.url.clone()) {
                    cx.open_url(&url);
                    self.dismiss_menu(window, cx);
                    return;
                }
                self.start_git(Action::CreatePullRequest);
            }
        }
        // Reopen, so the popup shows the action running and then its outcome.
        let error = self.menu.error.take();
        self.show_git_menu(anchor, window, cx);
        self.menu.error = error;
        cx.notify();
    }

    fn start_git(&mut self, action: Action) {
        let token = self
            .menu
            .github
            .profile
            .as_ref()
            .map(|profile| profile.token.clone());
        if let Err(error) = self.git.start(action, token) {
            self.menu.error = Some(error.to_string());
        } else {
            self.menu.error = None;
        }
    }

    pub(super) fn submit_git_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.menu.input_text(cx).trim().to_owned();
        self.start_git(Action::Commit(message));
        if self.menu.error.is_none() {
            let anchor = self.menu.anchor;
            self.show_git_menu(anchor, window, cx);
        }
        cx.notify();
    }

    /// The prefetched pull request: title, number, lifecycle, and its line
    /// counts, then how ready it is.
    fn render_git_pr(&self, cx: &App) -> AnyElement {
        let Some(pr) = self.git_pull_request() else {
            return div().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let color = pr_color(pr, cx);
        v_flex()
            .debug_selector(|| "git-menu-pr".into())
            .w_full()
            .gap_1()
            .py_1()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(pr.title.clone()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().text_color(color).child(format!("#{}", pr.number)))
                    .child(Tag::secondary().child(pr.lifecycle()))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(cx.theme().green)
                            .child(format!("+{}", crate::sidebar::compact(pr.additions))),
                    )
                    .child(
                        div()
                            .text_color(cx.theme().red)
                            .child(format!("-{}", crate::sidebar::compact(pr.deletions))),
                    ),
            )
            .when(pr.state == PrState::Open, |block| {
                block.child(div().text_color(muted).child(if pr.is_draft {
                    "Draft — not ready for review"
                } else {
                    "Ready for review"
                }))
            })
            .children(
                [
                    ("Review", pr.review().to_owned()),
                    ("Merge", pr.merge_status().to_owned()),
                    ("Checks", pr.checks_summary.clone()),
                ]
                .into_iter()
                .map(|(heading, label)| {
                    h_flex()
                        .gap_2()
                        .text_sm()
                        .child(
                            div()
                                .w(px(56.))
                                .flex_none()
                                .text_color(muted)
                                .child(heading),
                        )
                        .child(div().flex_1().min_w_0().child(label))
                }),
            )
            .into_any_element()
    }

    /// What a commit would include, and the running action or its outcome.
    fn render_git_status(&self, cx: &App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let summary = match self.git.status().filter(|status| status.dirty()) {
            None => div().text_color(muted).child(summary(self.git.status())),
            Some(status) => h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(muted)
                        .child("Not yet committed")
                        .when(status.untracked > 0, |label| {
                            label.child(format!(" ({} untracked)", status.untracked))
                        }),
                )
                .child(
                    div()
                        .text_color(cx.theme().green)
                        .child(format!("+{}", crate::sidebar::compact(status.additions))),
                )
                .child(
                    div()
                        .text_color(cx.theme().red)
                        .child(format!("-{}", crate::sidebar::compact(status.deletions))),
                ),
        };
        v_flex()
            .debug_selector(|| "git-menu-summary".into())
            .w_full()
            .gap_1()
            .child(summary)
            .children(self.git.running().map(|action| {
                div()
                    .text_color(cx.theme().warning)
                    .child(action.running_label())
            }))
            .children(self.git.outcome().map(|outcome| {
                h_flex()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(outcome.message.clone()))
                    .children(outcome.url.clone().map(|url| {
                        Button::new("git-menu-open")
                            .link()
                            .label("Open")
                            .on_click(move |_, _, cx| cx.open_url(&url))
                    }))
            }))
            .children(
                self.git
                    .error()
                    .map(str::to_owned)
                    .into_iter()
                    .chain(self.menu.error.clone())
                    .map(|error| div().text_color(cx.theme().danger).child(error)),
            )
            .into_any_element()
    }

    /// The commit dialog: what will be staged, the message, then the actions.
    fn git_commit_dialog(
        &self,
        dialog: Dialog,
        weak: &WeakEntity<HerdrWindow>,
        cx: &App,
    ) -> Dialog {
        let armed = !self.menu.input_text(cx).trim().is_empty();
        dialog
            .title(
                v_flex()
                    .child("Commit")
                    .children(self.git.tracked().map(|input| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(input.branch.clone())
                    })),
            )
            .child(
                v_flex()
                    .gap_3()
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("Stages every change in the checkout, then commits."),
                    )
                    .child(
                        div()
                            .debug_selector(|| "git-commit-summary".into())
                            .child(summary(self.git.status())),
                    )
                    .children(self.menu.input.as_ref().map(Input::new))
                    .children(error_alert("git-commit-error", self.menu.error.as_ref())),
            )
            .footer(dialog_buttons(
                weak,
                Some(
                    Button::new("git-commit-submit")
                        .primary()
                        .label("Commit")
                        .when(!armed, |button| button.ghost())
                        .on_click(listener(weak, |this, window, cx| {
                            this.submit_git_commit(window, cx)
                        })),
                ),
            ))
            .on_ok(submit(weak, |this, window, cx| {
                this.submit_git_commit(window, cx)
            }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{Page, PrState, Row, summary};
    use crate::git::Status;
    use crate::sidebar::layout_tests::REPO_KEY;
    use gpui_kit::{TestAppContext, point, px};
    use std::sync::Arc;

    fn status(additions: u64, deletions: u64, untracked: u64) -> Status {
        Status {
            additions,
            deletions,
            untracked,
        }
    }

    #[test]
    fn summary_reads_as_a_sentence_before_and_after_the_first_refresh() {
        assert_eq!(summary(None), "Checking working tree...");
        assert_eq!(summary(Some(Status::default())), "No uncommitted changes");
        assert_eq!(summary(Some(status(12, 3, 0))), "+12 -3");
        assert_eq!(summary(Some(status(12, 3, 1))), "+12 -3, 1 untracked entry");
        assert_eq!(summary(Some(status(0, 0, 4))), "+0 -0, 4 untracked entries");
    }

    #[gpui_kit::test]
    fn only_a_local_daemon_checkout_is_tracked(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                assert!(view.git_input().is_none(), "no connection, no checkout");
                view.live.status = crate::state::ConnectionStatus::Connected;
                view.live.local_daemon_peer = true;
                let mut snapshot = crate::sidebar::layout_tests::snapshot(6);
                snapshot.focused_workspace_id = Some("w3".into());
                view.live.snapshot = Some(Arc::new(snapshot));
                let input = view.git_input().unwrap();
                assert_eq!(input.repo_key, REPO_KEY);
                assert_eq!(input.branch, "develop");
                assert_eq!(input.checkout, None, "the checkout is resolved by Git");
                // A workspace without worktree metadata cannot be acted on.
                if let Some(snapshot) = view.live.snapshot.as_ref().map(Arc::clone) {
                    let mut snapshot = (*snapshot).clone();
                    snapshot.focused_workspace_id = Some("w0".into());
                    view.live.snapshot = Some(Arc::new(snapshot));
                }
                assert!(view.git_input().is_none());
                if let Some(snapshot) = view.live.snapshot.as_ref().map(Arc::clone) {
                    let mut snapshot = (*snapshot).clone();
                    snapshot.focused_workspace_id = Some("w3".into());
                    view.live.snapshot = Some(Arc::new(snapshot));
                }
                assert!(view.git_input().is_some());
                view.live.local_daemon_peer = false;
                assert!(view.git_input().is_none(), "remote peers run no local Git");
                view.live.local_daemon_peer = true;
                view.selected_endpoint = 0;
                view.live.status = crate::state::ConnectionStatus::Disconnected;
                assert!(view.git_input().is_none());
            })
        });
    }

    #[gpui_kit::test]
    fn a_cached_pull_request_is_named_and_only_an_open_one_can_be_opened(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        let input = crate::pull_request::Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "develop".into(),
        };
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                view.git = crate::git::Git::fixture(input.clone(), status(146, 42, 0));
                assert!(
                    view.git_pull_request().is_none(),
                    "a signed-out client shows no pull request"
                );
                view.menu.github = crate::github::Auth::connected_fixture();
                let pr = crate::pull_request::fixture().unwrap();
                view.menu
                    .pr_cache
                    .seed(input.clone(), pr, std::time::Instant::now());
                assert_eq!(view.git_pull_request().map(|pr| pr.number), Some(8));
                assert_eq!(
                    view.git_rows().last().map(|(_, label)| label.clone()),
                    Some("Open pull request #8".into())
                );
            })
        });
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                // A merged pull request still names the branch's history, but
                // the next action is creating another one.
                let mut merged = crate::pull_request::fixture().unwrap();
                merged.state = PrState::Merged;
                view.menu
                    .pr_cache
                    .seed(input, merged, std::time::Instant::now());
                assert_eq!(view.git_pull_request().map(|pr| pr.number), Some(8));
                assert_eq!(
                    view.git_rows().last().map(|(_, label)| label.clone()),
                    Some("Create pull request".into())
                );
                cx.notify();
            })
        });
    }

    #[gpui_kit::test]
    fn the_menu_commits_through_a_dialog_and_refuses_an_empty_message(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        let input = crate::pull_request::Input {
            checkout: None,
            repo_key: REPO_KEY.into(),
            branch: "develop".into(),
        };
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.git = crate::git::Git::fixture(input.clone(), status(12, 3, 1));
                view.open_git_menu(point(px(900.), px(20.)), window, cx);
                assert_eq!(view.menu.page, Some(Page::Git));
                let rows: Vec<_> = view
                    .git_rows()
                    .into_iter()
                    .map(|(_, label)| label)
                    .collect();
                assert_eq!(rows, ["Commit...", "Push", "Create pull request"]);
                view.activate_git_row(Row::Commit, window, cx);
                assert_eq!(view.menu.page, Some(Page::GitCommit));
                assert!(view.menu.input.is_some(), "the dialog opens with a field");
                view.submit_git_commit(window, cx);
                assert_eq!(
                    view.menu.error.as_deref(),
                    Some(crate::Error::GitCommitMessage.to_string().as_str())
                );
                assert_eq!(view.menu.page, Some(Page::GitCommit), "the dialog stays open");
                assert!(view.git.running().is_none());
                if let Some(input) = view.menu.input.clone() {
                    input.update(cx, |input, cx| {
                        input.set_value("fix: keep the popup open", window, cx)
                    });
                }
                view.submit_git_commit(window, cx);
                assert_eq!(view.menu.error, None);
                assert_eq!(view.menu.page, Some(Page::Git));
                assert!(matches!(
                    view.git.running(),
                    Some(crate::git::Action::Commit(message)) if message == "fix: keep the popup open"
                ));
                // A second action cannot start while the first is running.
                view.activate_git_row(Row::Push, window, cx);
                assert!(matches!(
                    view.git.running(),
                    Some(crate::git::Action::Commit(_))
                ));
                view.dismiss_menu(window, cx);
                assert_eq!(view.menu.page, None);
                assert!(
                    view.git.running().is_some(),
                    "dismissing the popup does not cancel queued work"
                );
            })
        });
    }
}
