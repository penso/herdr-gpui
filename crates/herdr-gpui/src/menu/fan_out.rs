//! Opening a fan-out from a workspace row: the repository's main checkout
//! creates every lane's worktree, branched from the row's own branch when the
//! row is a linked checkout.

use super::{Page, WorkspaceTarget, workspace::Source as Addressed};
use crate::{
    HerdrWindow,
    dialog_input::DialogInput,
    fan_out::{FanOut, Journal, Origin},
    teleport::host_for,
};
use gpui::{Context, Window};

/// What a fan-out branches from.
struct Source {
    workspace_id: String,
    repo_key: String,
    repo_label: String,
    base: String,
}

impl HerdrWindow {
    /// The repository checkout new lanes are created through, when the menu's
    /// workspace has one: its workspace, repository key and label, and base
    /// ref.
    fn fan_out_source(&self) -> Option<Source> {
        let target = self.menu.target.as_ref()?;
        let describe = |source: &WorkspaceTarget| {
            let tree = source.worktree.as_ref();
            Source {
                workspace_id: source.id.clone(),
                repo_key: tree.map(|tree| tree.key.clone()).unwrap_or_default(),
                repo_label: tree.map_or_else(|| source.label.clone(), |tree| tree.label.clone()),
                base: source.base_ref().unwrap_or_else(|_| "HEAD".to_owned()),
            }
        };
        // Lanes are created through the workspace, so one whose shell left
        // the repository Herdr keeps for it would branch that repository.
        if target.can_create() {
            return (target.source == Addressed::Workspace).then(|| describe(target));
        }
        self.linked_new_worktree_target()
            .filter(|target| target.source == Addressed::Workspace)
            .as_ref()
            .map(describe)
    }

    /// Whether the workspace a fan-out creates its lanes through is still in
    /// the repository the fan-out was opened for. Its shell may have moved
    /// since, while Herdr keeps the old repository for it.
    pub(crate) fn fan_out_origin_current(&self) -> bool {
        let Some(origin) = self.fan_out.as_ref().map(FanOut::origin) else {
            return false;
        };
        let Some(index) = self
            .endpoints
            .iter()
            .position(|endpoint| endpoint.id == origin.endpoint_id)
        else {
            return false;
        };
        self.endpoint_snapshot(index)
            .and_then(|snapshot| {
                snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == origin.workspace_id)
            })
            .is_some_and(
                |workspace| match super::workspace::known_worktree(workspace) {
                    Some(tree) => tree.key == origin.repo_key,
                    // A branch-only workspace has no key to compare.
                    None => workspace.worktree.is_none() && origin.repo_key.is_empty(),
                },
            )
    }

    /// The workspace menu's fan-out row: the comparison of a launched fan-out
    /// while there is one, otherwise a new fan-out from a Git workspace on a
    /// host that can be scripted.
    pub(crate) fn fan_out_item(&self) -> Option<&'static str> {
        if self
            .fan_out
            .as_ref()
            .is_some_and(|fan_out| !fan_out.composing())
        {
            return Some("Fan-out comparison...");
        }
        // Host scripts need a POSIX client; see `herdr_client::run_script`.
        let scriptable = cfg!(any(target_os = "linux", target_os = "macos"))
            && host_for(&self.endpoints[self.selected_endpoint].connection.target).is_ok();
        (scriptable && self.fan_out_source().is_some()).then_some("Fan out prompt...")
    }

    pub(crate) fn open_fan_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .fan_out
            .as_ref()
            .is_some_and(|fan_out| !fan_out.composing())
        {
            self.menu.page = Some(Page::FanOut);
            self.menu.error = None;
            self.menu.input = None;
            window.focus(&self.menu.focus, cx);
            cx.notify();
            return;
        }
        let Some(Source {
            workspace_id,
            repo_key,
            repo_label,
            base,
        }) = self.fan_out_source()
        else {
            return;
        };
        let selected = &self.endpoints[self.selected_endpoint];
        let Ok(host) = host_for(&selected.connection.target) else {
            return;
        };
        let origin = Origin {
            endpoint_id: selected.id.clone(),
            endpoint_label: selected.label.clone(),
            host,
            workspace_id,
            repo_key,
            repo_label,
            base,
        };
        self.menu.page = Some(Page::FanOut);
        self.menu.error = None;
        self.menu.input = Some(DialogInput::default());
        self.fan_out = Some(FanOut::start(origin, Journal::get(cx)));
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
