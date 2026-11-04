use crate::SheprdWindow;
use gpui::Context;
use std::{path::Path, sync::Arc};

impl SheprdWindow {
    /// Give the fake daemon's two project groups real, isolated repositories
    /// and keep their workers' databases inside the test's temporary folder.
    pub(crate) fn prepare_orchestrator_projects_for_tests(
        &mut self,
        root: &Path,
        cx: &mut Context<Self>,
    ) {
        if let Some(snapshot) = self.live.snapshot.as_mut() {
            for workspace in &mut Arc::make_mut(snapshot).workspaces {
                let folder = if workspace.worktree.is_some() {
                    "worktrees"
                } else {
                    "other"
                };
                workspace.new_workspace_cwd = root.join(folder).to_string_lossy().into_owned();
            }
        }
        let id = self.sections_view(cx);
        self.activity.section_view = Some(id);
        let specs = self
            .repo_specs()
            .into_iter()
            .map(|mut spec| {
                spec.request.data_root = Some(root.join("data"));
                spec
            })
            .collect();
        self.orchestrators[&id]
            .view()
            .update(cx, |view, cx| view.set_repos(specs, cx));
    }
}
