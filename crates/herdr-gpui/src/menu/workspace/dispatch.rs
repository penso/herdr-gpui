//! Submitting a new worktree or workspace dialog whose host picker chose
//! another host, or a new machine: the work runs as a dispatch job instead of
//! a daemon request.

use super::chosen_label;
use crate::{
    HerdrWindow,
    dispatch::{Destination, Job, Target},
    menu::WorkspaceAction,
    teleport::Naming,
};

impl HerdrWindow {
    /// Where the dialog's picker sends the new checkout when it is not this
    /// host. A GitHub listing creates its own checkouts on this host.
    pub(in crate::menu) fn dispatch_choice(&self, action: WorkspaceAction) -> Option<Target> {
        let creates = match action {
            WorkspaceAction::NewWorktree => !self.worktree_listing(),
            WorkspaceAction::NewWorkspace => true,
            _ => false,
        };
        if !creates {
            return None;
        }
        match self.menu.dispatch.as_ref()?.destination() {
            Destination::Here => None,
            Destination::Host(host) => Some(Target::Host(host.endpoint_id.clone())),
            Destination::Sandbox(sandbox) => Some(Target::Sandbox(sandbox)),
        }
    }

    /// Start creating on `target`. The dialog stays open on the job's
    /// progress, and the window follows the result once it lands.
    pub(in crate::menu) fn submit_dispatch(
        &mut self,
        action: WorkspaceAction,
        target: &Target,
        name: Option<String>,
    ) -> crate::Result<()> {
        let endpoint_id = match target {
            Target::Host(endpoint_id) => endpoint_id.as_str(),
            Target::Sandbox(sandbox) => return self.submit_sandboxed(action, *sandbox, name),
        };
        if self.dispatch_job.is_some() {
            return Err(crate::dispatch::Error::Busy.into());
        }
        let target = self
            .menu
            .target
            .as_ref()
            .ok_or(crate::Error::StaleWorkspace)?;
        let tree = target
            .worktree
            .as_ref()
            .ok_or(crate::Error::WorkspaceRepositoryChanged)?;
        let destination = self.dispatch_destination(endpoint_id);
        let origin = self.dispatch_origin(&target.id, &tree.key, &tree.label);
        let (Some(origin), Some(destination)) = (origin, destination) else {
            let label = self
                .endpoints
                .iter()
                .find(|endpoint| endpoint.id == endpoint_id)
                .map_or_else(|| endpoint_id.to_owned(), |e| e.label.clone());
            return Err(crate::dispatch::Error::HostUnavailable(label).into());
        };
        let job = match self.dispatch_then(action, name)? {
            crate::dispatch::Then::Worktree { base, naming } => {
                Job::worktree(origin, destination, base, naming)
            }
            crate::dispatch::Then::Workspace { label } => {
                Job::workspace(origin, destination, label)
            }
        };
        self.dispatch_job = Some(job);
        self.menu.error = None;
        Ok(())
    }

    /// What the open dialog asks for, from its fields.
    fn dispatch_then(
        &self,
        action: WorkspaceAction,
        name: Option<String>,
    ) -> crate::Result<crate::dispatch::Then> {
        let target = self
            .menu
            .target
            .as_ref()
            .ok_or(crate::Error::StaleWorkspace)?;
        let text = self
            .menu
            .input
            .as_ref()
            .map_or("", |input| input.text.as_str())
            .trim();
        Ok(if action == WorkspaceAction::NewWorktree {
            if !text.is_empty() {
                crate::worktree::validate_branch(text)?;
            }
            crate::dispatch::Then::Worktree {
                base: target.base_ref()?,
                naming: Naming {
                    branch: (!text.is_empty()).then(|| text.to_owned()),
                    label: name,
                },
            }
        } else {
            crate::dispatch::Then::Workspace {
                label: chosen_label(text, self.menu.suggested_name.as_deref()).map(str::to_owned),
            }
        })
    }

    /// Start making a new machine from `sandbox`, then creating there.
    fn submit_sandboxed(
        &mut self,
        action: WorkspaceAction,
        sandbox: crate::dispatch::Sandbox,
        name: Option<String>,
    ) -> crate::Result<()> {
        if self.dispatch_job.is_some() {
            return Err(crate::dispatch::Error::Busy.into());
        }
        let target = self
            .menu
            .target
            .as_ref()
            .ok_or(crate::Error::StaleWorkspace)?;
        let tree = target
            .worktree
            .as_ref()
            .ok_or(crate::Error::WorkspaceRepositoryChanged)?;
        let origin = self
            .dispatch_origin(&target.id, &tree.key, &tree.label)
            .ok_or_else(|| {
                crate::dispatch::Error::HostUnavailable(
                    self.endpoints[self.selected_endpoint].label.clone(),
                )
            })?;
        let then = self.dispatch_then(action, name)?;
        #[cfg(feature = "cloud")]
        {
            self.dispatch_job = Some(Job::sandboxed(sandbox, origin, then)?);
            self.menu.error = None;
            Ok(())
        }
        #[cfg(not(feature = "cloud"))]
        {
            let _ = (origin, then);
            match sandbox {}
        }
    }
}
