//! Share the desktop's prepared PR cache, never credentials or a second lookup,
//! and keep what answers phones' questions about each workspace beside it.
use super::*;
use crate::{
    SheprdWindow,
    pull_request::Origin,
    remote::changes::{Source, Sources},
};
use gpui::Context;
use sheprd_remote_control::control::pull_requests::{MAX_PULL_REQUESTS, WorkspacePullRequest};

impl SheprdWindow {
    pub(crate) fn publish_remote_pull_requests(&self, cx: &mut Context<Self>) {
        let Some(supervisor) = cx.try_global::<Supervisor>() else {
            return;
        };
        if !supervisor.config.enabled
            || supervisor
                .pull_requests_checked
                .is_some_and(|checked| checked.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        let Some(endpoint) = self.endpoints.first() else {
            return;
        };
        let live = if self.selected_endpoint == 0 {
            &self.live
        } else {
            &endpoint.live
        };
        if !live.local_daemon_peer || !live.status.is_connected() {
            return;
        }
        let Some(snapshot) = &live.snapshot else {
            return;
        };
        if !matches!(&supervisor.health, Health::Running(status) if status.boot_id == snapshot.boot_id)
        {
            return;
        }
        let view = self
            .endpoint_pr_view(0)
            .filter(|view| view.origin == Origin::Local);
        let tokens = self
            .endpoint_pr_profile(0)
            .map(|profile| self.tokens_over(profile));
        let mut sources = Sources {
            boot_id: snapshot.boot_id.clone(),
            ..Sources::default()
        };
        let mut entries = Vec::new();
        for workspace in &snapshot.workspaces {
            let found = view
                .as_ref()
                .zip(workspace.worktree.as_ref())
                .zip(workspace.branch.as_ref())
                .and_then(|((view, tree), branch)| {
                    let pr = self.menu.pr_cache.peek(view, &tree.key, branch)?;
                    Some((pr, branch))
                });
            sources.workspaces.insert(
                workspace.workspace_id.clone(),
                Source::new(
                    workspace.clone(),
                    found.as_ref().map(|(pr, _)| &**pr),
                    tokens.as_ref(),
                ),
            );
            let Some((pr, branch)) = found else {
                continue;
            };
            if entries.len() == MAX_PULL_REQUESTS {
                continue;
            }
            let badge = WorkspacePullRequest {
                workspace_id: workspace.workspace_id.clone(),
                cwd: workspace.new_workspace_cwd.clone(),
                branch: branch.clone(),
                number: pr.number,
                url: pr.url.clone(),
                tone: pr.tone(),
            };
            if badge.valid() {
                entries.push(badge);
            }
        }
        entries.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
        let update = PullRequests {
            boot_id: snapshot.boot_id.clone(),
            entries,
        };
        let supervisor = cx.global_mut::<Supervisor>();
        supervisor.set_change_sources(sources);
        if !update.valid() {
            return;
        }
        supervisor.pull_requests_checked = Some(Instant::now());
        if supervisor.pull_requests.as_deref() != Some(&update) {
            supervisor.pull_requests = Some(Arc::new(update));
            supervisor.poke = true;
        }
    }
}

#[cfg(test)]
mod tests;
