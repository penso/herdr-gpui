//! Share the desktop's prepared PR cache, never credentials or a second lookup.
use super::*;
use crate::{SheprdWindow, pull_request::Origin};
use gpui::Context;
use sheprd_remote_control::control::pull_requests::{
    MAX_PULL_REQUESTS, UnopenedBranch, WorkspacePullRequest,
};

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
        let mut entries = Vec::new();
        let mut unopened = Vec::new();
        for workspace in &snapshot.workspaces {
            let Some((view, tree, branch)) = view
                .as_ref()
                .zip(workspace.worktree.as_ref())
                .zip(workspace.branch.as_ref())
                .map(|((view, tree), branch)| (view, tree, branch))
            else {
                continue;
            };
            // The same reading the sidebar's row makes of this checkout.
            if let Some(ahead) = self.git.ahead(&tree.key, branch)
                && self
                    .menu
                    .pr_cache
                    .unopened(view, &tree.key, branch, &ahead.head)
            {
                let badge = UnopenedBranch {
                    workspace_id: workspace.workspace_id.clone(),
                    cwd: workspace.new_workspace_cwd.clone(),
                    branch: branch.clone(),
                    commits: ahead.commits,
                };
                if badge.valid() && unopened.len() < MAX_PULL_REQUESTS {
                    unopened.push(badge);
                }
                continue;
            }
            let Some(pr) = self.menu.pr_cache.peek(view, &tree.key, branch) else {
                continue;
            };
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
            if entries.len() == MAX_PULL_REQUESTS {
                break;
            }
        }
        entries.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
        unopened.sort_by(|left, right| left.workspace_id.cmp(&right.workspace_id));
        let update = PullRequests {
            boot_id: snapshot.boot_id.clone(),
            entries,
            unopened,
        };
        if !update.valid() {
            return;
        }
        let supervisor = cx.global_mut::<Supervisor>();
        supervisor.pull_requests_checked = Some(Instant::now());
        if supervisor.pull_requests.as_deref() != Some(&update) {
            supervisor.pull_requests = Some(Arc::new(update));
            supervisor.poke = true;
        }
    }
}

#[cfg(test)]
mod tests;
