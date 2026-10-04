use super::{Page, WorkspaceMenuAction};
use crate::{
    HerdrWindow,
    pull_request::{Input, Origin, repository_input},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::*;
use std::sync::Arc;

impl HerdrWindow {
    /// Where the selected device's checkouts live, if pull requests can be
    /// looked up for them: the verified local daemon, or a saved SSH device.
    /// Any other socket may be forwarded from an unknown machine.
    pub(crate) fn pr_origin(&self) -> Option<Origin> {
        match &self.endpoints[self.selected_endpoint].connection.target {
            herdr_client::ConnectTarget::Ssh { target, .. } => Some(Origin::Ssh(target.clone())),
            _ if self.selected_endpoint == 0 && self.live.local_daemon_peer => Some(Origin::Local),
            _ => None,
        }
    }

    pub(super) fn refresh_workspace_pr(&mut self) {
        self.menu.pr.clear();
        if self.pr_profile().is_none() {
            return;
        }
        let result = (|| {
            let target = self
                .menu
                .target
                .as_ref()
                .ok_or(crate::Error::StaleWorkspace)?;
            if target.worktree.is_none() || target.branch.as_deref().is_none_or(str::is_empty) {
                return Err(crate::Error::PrMetadata);
            }
            if self.pr_origin().is_none() {
                return Err(crate::Error::PrUntrustedEndpoint);
            }
            if !self.workspace_pr_target_current() {
                return Err(crate::Error::StaleWorkspace);
            }
            repository_input(target.worktree.as_ref(), target.branch.as_deref())
        })();
        match result {
            Ok(input) => {
                self.sync_pr_scope();
                self.menu.pr_connection = Some(Arc::downgrade(
                    &self.endpoints[self.selected_endpoint].connection.inbox,
                ));
                self.menu
                    .pr_cache
                    .present(&input, &mut self.menu.pr, std::time::Instant::now());
            }
            Err(error) => self.menu.pr.message = Some(error.to_string()),
        }
    }

    pub(super) fn sync_pr_scope(&mut self) {
        let endpoint = &self.endpoints[self.selected_endpoint];
        let same_connection = self
            .menu
            .pr_cache_connection
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .is_some_and(|old| Arc::ptr_eq(&old, &endpoint.connection.inbox));
        if !same_connection {
            self.menu.pr_cache.clear();
            self.menu.pr_cache_connection = Some(Arc::downgrade(&endpoint.connection.inbox));
        }
        if let (Some(snapshot), Some(profile), Some(origin)) =
            (&self.live.snapshot, self.pr_profile(), self.pr_origin())
        {
            let scope = (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
                snapshot.boot_id.clone(),
            );
            let token = profile.token.clone();
            self.menu.pr_cache.scope(scope, token, origin);
        }
    }

    fn workspace_pr_target_current(&self) -> bool {
        self.menu_target_current()
            && self.live.status.is_connected()
            && self.menu.pr_connection.as_ref().is_none_or(|old| {
                old.upgrade().is_some_and(|old| {
                    Arc::ptr_eq(
                        &old,
                        &self.endpoints[self.selected_endpoint].connection.inbox,
                    )
                })
            })
            && self.menu.target.as_ref().is_some_and(|target| {
                self.live.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.boot_id == target.boot_id
                        && snapshot.workspaces.iter().any(|workspace| {
                            workspace.workspace_id == target.id
                                && workspace.worktree == target.worktree
                                && workspace.branch == target.branch
                        })
                })
            })
    }

    pub(crate) fn update_workspace_pr(&mut self) -> bool {
        let mut changed = false;
        if self.pr_profile().is_none() {
            self.menu.pr_cache.clear();
            self.menu.pr.clear();
            if self.menu.workspace_selected == Some(WorkspaceMenuAction::PullRequest) {
                self.menu.workspace_selected = None;
                changed = true;
            }
            return changed;
        }
        let eligible = self.pr_origin().is_some()
            && self.live.status.is_connected()
            && self.live.snapshot.is_some();
        if eligible {
            self.sync_pr_scope();
            let now = std::time::Instant::now();
            if let Some(snapshot) = &self.live.snapshot {
                if self
                    .menu
                    .pr_snapshot
                    .as_ref()
                    .and_then(std::sync::Weak::upgrade)
                    .is_none_or(|old| !Arc::ptr_eq(&old, snapshot))
                {
                    self.menu.pr_snapshot = Some(Arc::downgrade(snapshot));
                    self.menu.pr_cache.retain(|input| {
                        snapshot.workspaces.iter().any(|workspace| {
                            workspace
                                .worktree
                                .as_ref()
                                .is_some_and(|tree| tree.key == input.repo_key)
                                && workspace.branch.as_ref() == Some(&input.branch)
                        })
                    });
                }
                if self.menu.pr_cache.scan_due(now) {
                    let priority = self
                        .menu
                        .target
                        .as_ref()
                        .filter(|_| self.menu.page == Some(Page::Workspace))
                        .map(|target| target.id.as_str());
                    let inputs = workspace_pr_inputs(snapshot, priority, self.menu.pr_cache.cursor);
                    self.menu.pr_cache.schedule(inputs, now);
                }
            }
            changed |= self.menu.pr_cache.poll(now);
        } else {
            self.menu.pr_cache.clear();
        }
        if self.menu.page == Some(Page::Workspace) && !self.workspace_pr_target_current() {
            if self.menu.pr.message.as_deref()
                != Some("Workspace changed or disconnected. Reopen the menu.")
            {
                self.menu.pr.clear();
                self.menu.pr.message =
                    Some("Workspace changed or disconnected. Reopen the menu.".into());
                changed = true;
            }
        } else if changed && self.menu.page == Some(Page::Workspace) {
            self.refresh_workspace_pr();
        }
        if self.menu.pr.value.is_none()
            && self.menu.workspace_selected == Some(WorkspaceMenuAction::PullRequest)
        {
            self.menu.workspace_selected = None;
            changed = true;
        }
        changed
    }

    pub(super) fn open_workspace_pr(&self, cx: &mut Context<Self>) {
        if self.pr_profile().is_some()
            && self.workspace_pr_target_current()
            && let Some(pr) = &self.menu.pr.value
        {
            cx.open_url(&pr.url);
        }
    }

    pub(super) fn render_workspace_pr(&self, text_width: Pixels, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let pr = &self.menu.pr;
        let mut section = div()
            .debug_selector(|| "workspace-pr".into())
            .mt(px(6.))
            .pt(px(4.))
            .pb(px(6.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex_none()
            .min_w_0();
        if let Some(value) = &pr.value {
            let action = WorkspaceMenuAction::PullRequest;
            let color = value.color(theme);
            section =
                section
                    .child(
                        div()
                            .id("workspace-pr-title")
                            .debug_selector(|| "workspace-pr-title".into())
                            .px(px(8.))
                            .py(px(6.))
                            .flex_none()
                            .rounded(px(crate::config::corners::CONTROL))
                            .cursor_pointer()
                            .when(self.menu.workspace_selected == Some(action), |row| {
                                row.bg(rgb(theme.active))
                            })
                            .on_hover(cx.listener(move |this, hovered, _, cx| {
                                if *hovered {
                                    this.menu.workspace_selected = Some(action);
                                } else if this.menu.workspace_selected == Some(action) {
                                    this.menu.workspace_selected = None;
                                }
                                cx.notify();
                            }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.activate_workspace_menu(action, window, cx);
                            }))
                            .child(
                                div()
                                    .w(text_width)
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(crate::sidebar::label_text(&format!(
                                        "#{} {}",
                                        value.number, value.title
                                    ))),
                            ),
                    )
                    .child(
                        div()
                            .ml(px(8.))
                            .w(text_width)
                            .flex_none()
                            .truncate()
                            .text_size(px((self.config.ui.size - 1.).max(8.)))
                            .text_color(rgb(theme.muted))
                            .child(format!(
                                "{} -> {}",
                                value.head_ref_name, value.base_ref_name
                            )),
                    )
                    .child(
                        div()
                            .mx(px(8.))
                            .mt(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_none()
                                    .px(px(6.))
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .bg(rgba((color << 8) | 0x20))
                                    .text_color(rgb(color))
                                    .child(value.lifecycle()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .text_color(rgb(theme.muted))
                                    .child(value.review()),
                            ),
                    )
                    .child(
                        div()
                            .mx(px(8.))
                            .mt(px(4.))
                            .text_color(rgb(theme.muted))
                            .child(value.merge_status()),
                    )
                    .child(
                        div()
                            .mx(px(8.))
                            .mt(px(4.))
                            .child(value.checks_summary.clone()),
                    )
                    .child(
                        div()
                            .mx(px(8.))
                            .mt(px(6.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(div().text_color(rgb(theme.ink(theme.palette[2]))).child(
                                crate::sidebar::label_text(&format!("+{}", value.additions)),
                            ))
                            .child(div().text_color(rgb(theme.ink(theme.palette[1]))).child(
                                crate::sidebar::label_text(&format!("-{}", value.deletions)),
                            ))
                            .child(div().text_color(rgb(theme.muted)).child(format!(
                                "{} {}",
                                value.changed_files,
                                if value.changed_files == 1 {
                                    "file"
                                } else {
                                    "files"
                                }
                            ))),
                    );
        }
        if pr.loading {
            section = section.child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_color(rgb(theme.muted))
                    .child("Checking GitHub..."),
            );
        } else if let Some(message) = &pr.message {
            section = section.child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_color(rgb(theme.muted))
                    .child(format!(
                        "{}{message}",
                        if pr.value.is_some() { "Stale: " } else { "" }
                    )),
            );
        } else if pr.value.is_none() {
            section = section.child(
                div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_color(rgb(theme.muted))
                    .child("No PR found for this origin and branch."),
            );
        }
        section
    }

    #[cfg(any(test, all(feature = "integration-test", target_os = "macos")))]
    pub(crate) fn workspace_pr_fixture(
        &mut self,
        value: crate::pull_request::PullRequest,
    ) -> crate::Result<()> {
        let target = self
            .menu
            .target
            .as_ref()
            .ok_or(crate::Error::StaleWorkspace)?;
        let input = repository_input(target.worktree.as_ref(), target.branch.as_deref())?;
        self.menu.github = crate::github::Auth::connected_fixture();
        self.live.local_daemon_peer = true;
        self.sync_pr_scope();
        self.menu
            .pr_cache
            .seed(input, value, std::time::Instant::now());
        self.refresh_workspace_pr();
        Ok(())
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub(crate) fn github_fixture(
        &mut self,
        waiting: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_menu(window, cx);
        self.menu.page = Some(Page::GitHub);
        self.menu.github = crate::github::Auth::fixture(waiting);
        cx.notify();
    }
}

fn workspace_pr_inputs<'a>(
    snapshot: &'a ClientShellSnapshot,
    open: Option<&'a str>,
    cursor: usize,
) -> impl Iterator<Item = Input> + 'a {
    let count = snapshot.workspaces.len();
    let start = cursor % count.max(1);
    let priority = open.or(snapshot.focused_workspace_id.as_deref());
    // Alternate priority and round-robin admission so focus changes cannot
    // starve the rest of the live workspace list. Cache scheduling deduplicates.
    snapshot
        .workspaces
        .iter()
        .filter(move |workspace| {
            cursor.is_multiple_of(2) && Some(workspace.workspace_id.as_str()) == priority
        })
        .chain(snapshot.workspaces.iter().cycle().skip(start).take(count))
        .filter_map(|workspace| {
            repository_input(workspace.worktree.as_ref(), workspace.branch.as_deref()).ok()
        })
}

#[cfg(test)]
mod tests;
