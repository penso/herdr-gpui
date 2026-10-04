//! The workspace a menu row targets: what may be created or deleted on it,
//! which sibling workspaces close with it, and the dialogs that carry those
//! requests to the daemon and report what came back.

use super::{
    Page, Removal, Submission, WorkspaceAction, WorkspaceMenuAction, danger, endpoint_error,
    state::Deletion,
};
use crate::{HerdrWindow, NavigationTarget, dialog_input::DialogInput};
use gpui::{prelude::*, *};
use herdr_client::{
    Method,
    protocol::{ClientShellSnapshot, ClientShellWorkspace, ClientShellWorktree},
};

pub(crate) struct WorkspaceTarget {
    pub(super) boot_id: String,
    pub(super) id: String,
    pub(super) label: String,
    pub(super) worktree: Option<ClientShellWorktree>,
    pub(super) close_members: Vec<String>,
    /// Whether linked worktrees of this parent's repository are open, so its
    /// row folds a group even when another parent closes on its own.
    pub(super) heads_group: bool,
    pub(super) branch: Option<String>,
    /// The branch a new worktree starts from, when it is not this checkout's
    /// `HEAD`: a linked checkout asks its main checkout, the only source the
    /// daemon accepts, to branch from the linked checkout's branch instead.
    pub(super) base: Option<String>,
}

impl WorkspaceTarget {
    pub(super) fn new(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> Self {
        Self {
            boot_id: snapshot.boot_id.clone(),
            id: workspace.workspace_id.clone(),
            label: workspace.label.clone(),
            worktree: workspace.worktree.clone(),
            close_members: close_members(snapshot, workspace),
            heads_group: heads_group(snapshot, workspace),
            branch: workspace.branch.clone(),
            base: None,
        }
    }

    /// The target a new worktree for `workspace` is created through. A linked
    /// checkout resolves to its repository's open main checkout, based on the
    /// linked checkout's branch so the new branch starts where it stands.
    pub(super) fn for_new_worktree(
        snapshot: &ClientShellSnapshot,
        workspace: &ClientShellWorkspace,
    ) -> Result<Self, NewWorktreeUnavailable> {
        let Some(tree) = workspace
            .worktree
            .as_ref()
            .filter(|tree| tree.is_linked_worktree)
        else {
            let target = Self::new(snapshot, workspace);
            return if target.can_create() {
                Ok(target)
            } else {
                Err(NewWorktreeUnavailable::NotGit)
            };
        };
        let branch = workspace
            .branch
            .clone()
            .ok_or(NewWorktreeUnavailable::Detached)?;
        let main = snapshot
            .workspaces
            .iter()
            .find(|w| {
                w.worktree
                    .as_ref()
                    .is_some_and(|other| other.key == tree.key && !other.is_linked_worktree)
            })
            .ok_or(NewWorktreeUnavailable::MainCheckoutClosed)?;
        Ok(Self {
            base: Some(branch),
            ..Self::new(snapshot, main)
        })
    }

    /// What the new worktree dialog says it branches from.
    pub(super) fn base_label(&self) -> &str {
        self.base.as_deref().unwrap_or("HEAD")
    }

    pub(super) fn can_create(&self) -> bool {
        // Like the TUI, accept Git branches before worktree metadata is available.
        (self.worktree.is_some() || self.branch.is_some()) && !self.can_delete()
    }

    pub(super) fn can_delete(&self) -> bool {
        self.worktree
            .as_ref()
            .is_some_and(|tree| tree.is_linked_worktree)
    }

    pub(super) fn validate_repository(&self, snapshot: &ClientShellSnapshot) -> crate::Result<()> {
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == self.id)
            .filter(|_| snapshot.boot_id == self.boot_id)
            .ok_or(crate::Error::StaleWorkspace)?;
        if !self.can_create()
            || self.worktree != workspace.worktree
            || (workspace.worktree.is_none() && workspace.branch.is_none())
        {
            return Err(crate::Error::WorkspaceRepositoryChanged);
        }
        Ok(())
    }

    /// The worktree key this workspace heads, when linked checkouts hang off it.
    pub(super) fn group_key(&self) -> Option<&str> {
        self.worktree
            .as_ref()
            .filter(|_| self.heads_group)
            .map(|tree| tree.key.as_str())
    }

    pub(super) fn close_label(&self) -> &'static str {
        if self.close_members.len() > 1 {
            "Close group"
        } else {
            "Close"
        }
    }

    pub(super) fn request(
        &self,
        snapshot: &ClientShellSnapshot,
        action: WorkspaceAction,
        text: &str,
    ) -> crate::Result<(Method, serde_json::Value)> {
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|w| w.workspace_id == self.id)
            .filter(|_| snapshot.boot_id == self.boot_id)
            .ok_or(crate::Error::StaleWorkspace)?;
        let params = match action {
            WorkspaceAction::Rename => {
                let label = text.trim();
                if label.is_empty() {
                    return Err(crate::Error::EmptyWorkspaceLabel);
                }
                (
                    Method::WorkspaceRename,
                    serde_json::json!({"workspace_id": self.id, "label": label}),
                )
            }
            WorkspaceAction::Close => {
                if self.worktree != workspace.worktree
                    || self.close_members != close_members(snapshot, workspace)
                {
                    return Err(crate::Error::WorkspaceGroupChanged);
                }
                // The daemon closes every checkout of the repository for a
                // group close, so a parent beside another parent asks for
                // itself alone.
                (
                    Method::WorkspaceClose,
                    serde_json::json!({"workspace_id": self.id, "close_group": self.close_members.len() > 1}),
                )
            }
            WorkspaceAction::NewWorktree => {
                self.validate_repository(snapshot)?;
                // A full ref, so a tag of the same name cannot shadow the branch.
                let base = match &self.base {
                    Some(branch) => {
                        crate::worktree::validate_branch(branch)?;
                        format!("refs/heads/{branch}")
                    }
                    None => "HEAD".to_owned(),
                };
                let mut params = serde_json::json!({"workspace_id": self.id, "base": base, "focus": true, "trust_repository": false});
                if !text.trim().is_empty() {
                    crate::worktree::validate_branch(text.trim())?;
                    params["branch"] = text.trim().into();
                }
                (Method::WorktreeCreate, params)
            }
            WorkspaceAction::OpenWorktree => {
                self.validate_repository(snapshot)?;
                if text.is_empty() {
                    return Err(crate::Error::WorktreeSelection);
                }
                (
                    Method::WorktreeOpen,
                    serde_json::json!({"workspace_id": self.id,
                    "path": text, "focus": true, "trust_repository": false}),
                )
            }
            // The label, when there is one, is added by the dialog that knows
            // which name it proposed.
            WorkspaceAction::NewTab => (
                Method::TabCreate,
                serde_json::json!({"workspace_id": self.id, "focus": true}),
            ),
            WorkspaceAction::NewWorkspace => (
                Method::WorkspaceCreate,
                serde_json::json!({"focus": true, "source_workspace_id": self.id}),
            ),
            WorkspaceAction::DeleteWorktree => {
                if !self.can_delete() || self.worktree != workspace.worktree {
                    return Err(crate::Error::WorkspaceCheckoutChanged);
                }
                (
                    Method::WorktreeRemove,
                    serde_json::json!({"workspace_id": self.id, "force": false, "trust_repository": false}),
                )
            }
        };
        Ok(params)
    }
}

/// Why the new worktree shortcut found no workspace to branch from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NewWorktreeUnavailable {
    Disconnected,
    NoWorkspace,
    NotGit,
    MainCheckoutClosed,
    Detached,
}

impl NewWorktreeUnavailable {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Disconnected => "Not connected, so no worktree can be created",
            Self::NoWorkspace => "No workspace is focused to create a worktree from",
            Self::NotGit => "This workspace is not a Git repository",
            Self::MainCheckoutClosed => "Open this repository's main checkout to create a worktree",
            Self::Detached => "This checkout has no branch to start a worktree from",
        }
    }
}

/// The workspace focused when the new worktree shortcut is pressed, once it
/// is known to have a source for one.
fn new_worktree_source(snapshot: &ClientShellSnapshot) -> Result<String, NewWorktreeUnavailable> {
    let focused = snapshot
        .workspaces
        .iter()
        .find(|w| Some(&w.workspace_id) == snapshot.focused_workspace_id.as_ref())
        .ok_or(NewWorktreeUnavailable::NoWorkspace)?;
    WorkspaceTarget::for_new_worktree(snapshot, focused)?;
    Ok(focused.workspace_id.clone())
}

/// Why the open or remove worktree shortcut found no workspace to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorktreeShortcutUnavailable {
    Disconnected,
    NoWorkspace,
    NotGit,
    MainCheckoutClosed,
    NotLinkedWorktree,
}

impl WorktreeShortcutUnavailable {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Disconnected => "Not connected, so no worktree can be changed",
            Self::NoWorkspace => "No workspace is focused",
            Self::NotGit => "This workspace is not a Git repository",
            Self::MainCheckoutClosed => "Open this repository's main checkout to open a worktree",
            Self::NotLinkedWorktree => "This workspace is not a worktree checkout",
        }
    }
}

fn focused_workspace(
    snapshot: &ClientShellSnapshot,
) -> Result<&ClientShellWorkspace, WorktreeShortcutUnavailable> {
    snapshot
        .workspaces
        .iter()
        .find(|w| Some(&w.workspace_id) == snapshot.focused_workspace_id.as_ref())
        .ok_or(WorktreeShortcutUnavailable::NoWorkspace)
}

/// The workspace whose "Open worktree..." row the open worktree shortcut
/// opens. Herdr lists and opens checkouts through the repository's main
/// checkout, so a focused linked checkout defers to it, as the new worktree
/// shortcut does.
pub(super) fn worktree_open_source(
    snapshot: &ClientShellSnapshot,
) -> Result<String, WorktreeShortcutUnavailable> {
    let focused = focused_workspace(snapshot)?;
    let Some(tree) = focused
        .worktree
        .as_ref()
        .filter(|tree| tree.is_linked_worktree)
    else {
        return if WorkspaceTarget::new(snapshot, focused).can_create() {
            Ok(focused.workspace_id.clone())
        } else {
            Err(WorktreeShortcutUnavailable::NotGit)
        };
    };
    snapshot
        .workspaces
        .iter()
        .find(|w| {
            w.worktree
                .as_ref()
                .is_some_and(|other| other.key == tree.key && !other.is_linked_worktree)
        })
        .map(|main| main.workspace_id.clone())
        .ok_or(WorktreeShortcutUnavailable::MainCheckoutClosed)
}

/// The workspace whose checkout the remove worktree shortcut offers to
/// delete: the focused one, only when it is a linked worktree. A main
/// checkout is never a candidate, as Herdr refuses to remove it too.
pub(super) fn worktree_removal_target(
    snapshot: &ClientShellSnapshot,
) -> Result<String, WorktreeShortcutUnavailable> {
    let focused = focused_workspace(snapshot)?;
    if WorkspaceTarget::new(snapshot, focused).can_delete() {
        Ok(focused.workspace_id.clone())
    } else {
        Err(WorktreeShortcutUnavailable::NotLinkedWorktree)
    }
}

/// The name a new tab dialog proposes: the next number in its workspace, as
/// Herdr numbers an unnamed tab.
pub(super) fn suggested_tab_name(snapshot: &ClientShellSnapshot, workspace: &str) -> String {
    let count = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == workspace)
        .count();
    (count + 1).to_string()
}

/// The name a new workspace dialog proposes: the folder it opens in. Herdr
/// also consults Git there, which only the daemon's host can do, so leaving
/// this proposal unchanged still lets the daemon choose the real name.
pub(super) fn suggested_workspace_name(cwd: &str) -> String {
    let trimmed = cwd.trim_end_matches('/');
    match trimmed.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ if cwd.is_empty() => "workspace".to_owned(),
        _ => "/".to_owned(),
    }
}

/// The label a creation sends. An empty name or the unchanged proposal sends
/// none, so the daemon names the tab or workspace as Herdr's own prompt does.
pub(super) fn chosen_label<'a>(text: &'a str, suggested: Option<&str>) -> Option<&'a str> {
    let text = text.trim();
    (!text.is_empty() && Some(text) != suggested).then_some(text)
}

/// The workspaces closing `workspace` closes: its whole group when it is its
/// repository's only parent and linked worktrees hang off it, else itself.
/// Like the TUI, a parent beside another parent closes alone.
fn close_members(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> Vec<String> {
    let alone = || vec![workspace.workspace_id.clone()];
    let Some(tree) = workspace
        .worktree
        .as_ref()
        .filter(|tree| !tree.is_linked_worktree)
    else {
        return alone();
    };
    let mut members = Vec::new();
    for w in &snapshot.workspaces {
        match &w.worktree {
            Some(other) if other.key == tree.key => {
                if w.workspace_id != workspace.workspace_id && !other.is_linked_worktree {
                    return alone();
                }
                members.push(w.workspace_id.clone());
            }
            _ => {}
        }
    }
    members.sort();
    members
}

/// Whether `workspace` is a parent with linked worktrees of its repository open.
fn heads_group(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> bool {
    workspace
        .worktree
        .as_ref()
        .filter(|tree| !tree.is_linked_worktree)
        .is_some_and(|tree| {
            snapshot.workspaces.iter().any(|w| {
                w.worktree
                    .as_ref()
                    .is_some_and(|other| other.key == tree.key && other.is_linked_worktree)
            })
        })
}

impl HerdrWindow {
    /// The selected endpoint's workspace a workspace menu is open for. A
    /// hover menu can target a row that is not focused, so the row marks
    /// itself while the pointer is over the menu instead.
    pub(crate) fn workspace_menu_target(&self) -> Option<&str> {
        if self.menu.page != Some(Page::Workspace) {
            return None;
        }
        self.menu.target.as_ref().map(|target| target.id.as_str())
    }

    pub(crate) fn open_workspace_menu(
        &mut self,
        id: &str,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() || !self.live.status.is_connected() {
            return;
        }
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        let Some(workspace) = snapshot.workspaces.iter().find(|w| w.workspace_id == id) else {
            return;
        };
        self.hover = None;
        self.hover_menu = None;
        self.menu.reset();
        self.menu.endpoint_target = (
            self.selection_epoch,
            self.endpoints[self.selected_endpoint].generation,
        );
        self.menu.target = Some(WorkspaceTarget::new(snapshot, workspace));
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Workspace);
        self.refresh_workspace_pr();
        self.marked.clear();
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    /// Opens the new worktree dialog for the focused workspace, as its menu's
    /// "New worktree" row would. When there is no source for one, a flash says
    /// why rather than the shortcut doing nothing visible.
    pub(crate) fn open_new_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = match &self.live.snapshot {
            Some(snapshot) if self.live.status.is_connected() => new_worktree_source(snapshot),
            _ => Err(NewWorktreeUnavailable::Disconnected),
        };
        let id = match source {
            Ok(id) => id,
            Err(reason) => {
                self.show_flash(crate::window::Flash::warning(reason.message()), cx);
                return;
            }
        };
        self.open_workspace_menu(&id, Point::default(), window, cx);
        if self.menu.page == Some(Page::Workspace) {
            self.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
        }
    }

    /// Opens the open or delete worktree dialog the focused workspace's menu
    /// offers, as its row would, so deleting still asks for confirmation.
    /// When the focused workspace has no such row, a flash says why rather
    /// than the shortcut doing nothing visible.
    pub(crate) fn open_focused_worktree_dialog(
        &mut self,
        action: WorkspaceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match &self.live.snapshot {
            Some(snapshot) if self.live.status.is_connected() => match action {
                WorkspaceAction::OpenWorktree => worktree_open_source(snapshot),
                WorkspaceAction::DeleteWorktree => worktree_removal_target(snapshot),
                _ => return,
            },
            _ => Err(WorktreeShortcutUnavailable::Disconnected),
        };
        let id = match target {
            Ok(id) => id,
            Err(reason) => {
                self.show_flash(crate::window::Flash::warning(reason.message()), cx);
                return;
            }
        };
        self.open_workspace_menu(&id, Point::default(), window, cx);
        if self.menu.page == Some(Page::Workspace) {
            self.open_workspace_dialog(action, window, cx);
        }
    }

    /// Opens the focused workspace's rename or close dialog, as its menu's
    /// row would. Closing confirms here as it does from the menu.
    pub(crate) fn open_focused_workspace_dialog(
        &mut self,
        action: WorkspaceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_workspace_id.clone())
        else {
            return;
        };
        self.open_workspace_menu(&id, Point::default(), window, cx);
        if self.menu.page == Some(Page::Workspace) {
            self.open_workspace_dialog(action, window, cx);
        }
    }

    /// Asks for a name before `command` creates a tab or workspace, when the
    /// local Herdr config's `ui.prompt_new_tab_name` or
    /// `ui.prompt_new_workspace_name` says to. Returns whether the dialog
    /// opened; otherwise the caller creates at once.
    pub(crate) fn open_name_prompt(
        &mut self,
        command: crate::controls::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use crate::controls::Command;
        let prompts = self
            .settings
            .shared
            .as_ref()
            .map(|shared| shared.name_prompts)
            .unwrap_or_default();
        let action = match command {
            Command::Tab if prompts.tab => WorkspaceAction::NewTab,
            Command::Workspace if prompts.workspace => WorkspaceAction::NewWorkspace,
            _ => return false,
        };
        // Only where creating at once would be allowed, so the prompt never
        // offers what the command itself would refuse.
        if self.activation_deadline.is_some()
            || !self.endpoints[self.selected_endpoint].surface_requested()
        {
            return false;
        }
        let Some(id) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_workspace_id.clone())
        else {
            return false;
        };
        self.open_workspace_menu(&id, Point::default(), window, cx);
        if self.menu.page != Some(Page::Workspace) {
            return false;
        }
        self.open_workspace_dialog(action, window, cx);
        true
    }

    pub(super) fn workspace_items(&self) -> Vec<(WorkspaceMenuAction, &'static str)> {
        use WorkspaceMenuAction::Dialog;
        let Some(target) = &self.menu.target else {
            return vec![];
        };
        let mut items = vec![
            (Dialog(WorkspaceAction::Rename), "Rename"),
            (Dialog(WorkspaceAction::Close), target.close_label()),
        ];
        if target.can_create() {
            items.push((Dialog(WorkspaceAction::NewWorktree), "New worktree"));
            items.push((Dialog(WorkspaceAction::OpenWorktree), "Open worktree..."));
        } else if self.linked_new_worktree_target().is_some() {
            items.push((Dialog(WorkspaceAction::NewWorktree), "New worktree"));
        }
        if target.can_delete() {
            items.push((
                Dialog(WorkspaceAction::DeleteWorktree),
                "Delete worktree checkout",
            ));
        }
        if self.teleport_mark().is_some() {
            items.push((WorkspaceMenuAction::GoToTeleported, "Go to teleported copy"));
            items.push((
                WorkspaceMenuAction::ClearTeleported,
                "Clear teleported mark",
            ));
        } else if self.can_teleport() {
            if self.teleport_origin().is_some() {
                items.push((WorkspaceMenuAction::TeleportBack, "Teleport back"));
            }
            items.push((WorkspaceMenuAction::Teleport, "Teleport..."));
        }
        // Only a workspace that heads a group of checkouts can fold anything.
        if let Some(key) = target.group_key() {
            items.push(if self.collapsed_repos_for_selection().contains(key) {
                (WorkspaceMenuAction::Expand, "Expand group")
            } else {
                (WorkspaceMenuAction::Collapse, "Collapse group")
            });
        }
        items
    }

    /// The collapsed set the sidebar paints for the selected endpoint.
    pub(super) fn collapsed_repos_for_selection(&self) -> &std::collections::HashSet<String> {
        if self.selected_endpoint == 0 {
            &self.collapsed_repos
        } else {
            &self.endpoints[self.selected_endpoint].collapsed_repos
        }
    }

    /// The collapsed set the sidebar paints for the selected endpoint.
    pub(super) fn collapsed_repos_mut(&mut self) -> &mut std::collections::HashSet<String> {
        if self.selected_endpoint == 0 {
            &mut self.collapsed_repos
        } else {
            &mut self.endpoints[self.selected_endpoint].collapsed_repos
        }
    }

    pub(super) fn toggle_selected_group(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self
            .menu
            .target
            .as_ref()
            .and_then(|target| target.group_key())
            .map(str::to_owned)
        else {
            return;
        };
        let collapsed = self.collapsed_repos_mut();
        if !collapsed.remove(&key) {
            collapsed.insert(key);
        }
        self.menu.reset();
        cx.notify();
    }

    /// The main-checkout target a linked checkout's menu creates through.
    fn linked_new_worktree_target(&self) -> Option<WorkspaceTarget> {
        let target = self
            .menu
            .target
            .as_ref()
            .filter(|target| target.can_delete())?;
        let snapshot = self.live.snapshot.as_ref()?;
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|w| w.workspace_id == target.id && snapshot.boot_id == target.boot_id)?;
        WorkspaceTarget::for_new_worktree(snapshot, workspace).ok()
    }

    pub(super) fn open_workspace_dialog(
        &mut self,
        action: WorkspaceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Every listing and request of the dialog goes through the main
        // checkout; only the base remembers the linked checkout.
        if action == WorkspaceAction::NewWorktree
            && let Some(target) = self.linked_new_worktree_target()
        {
            self.menu.target = Some(target);
        }
        let Some(target) = &self.menu.target else {
            return;
        };
        let workspace = self.live.snapshot.as_ref().and_then(|snapshot| {
            let workspace = snapshot
                .workspaces
                .iter()
                .find(|w| w.workspace_id == target.id && snapshot.boot_id == target.boot_id)?;
            Some((snapshot, workspace))
        });
        // Proposed selected, as Herdr does, so typing replaces it.
        self.menu.suggested_name = match (action, workspace) {
            (WorkspaceAction::NewTab, Some((snapshot, _))) => {
                Some(suggested_tab_name(snapshot, &target.id))
            }
            (WorkspaceAction::NewWorkspace, Some((_, workspace))) => {
                Some(suggested_workspace_name(&workspace.new_workspace_cwd))
            }
            _ => None,
        };
        self.menu.input = match action {
            WorkspaceAction::Rename => Some(DialogInput::new(target.label.clone())),
            WorkspaceAction::NewTab | WorkspaceAction::NewWorkspace => Some(DialogInput::new(
                self.menu.suggested_name.clone().unwrap_or_default(),
            )),
            // Propose the daemon's own branch shape, selected so typing replaces it.
            WorkspaceAction::NewWorktree => {
                Some(DialogInput::new(crate::worktree::proposed_branch()))
            }
            WorkspaceAction::Close
            | WorkspaceAction::DeleteWorktree
            | WorkspaceAction::OpenWorktree => None,
        };
        self.menu.page = Some(Page::Dialog(action));
        self.menu.pr.clear();
        self.menu.pr_connection = None;
        self.menu.error = None;
        if action == WorkspaceAction::OpenWorktree {
            self.open_existing_worktrees(window, cx);
            return;
        }
        if action == WorkspaceAction::NewWorktree {
            self.open_worktree_source(window, cx);
            cx.notify();
            return;
        }
        if action == WorkspaceAction::Close
            && let Some(snapshot) = &self.live.snapshot
        {
            self.menu.close_check = Some(super::workspace_close::CloseCheck::start(
                snapshot,
                target,
                self.selected_endpoint == 0 && self.live.local_daemon_peer,
                cx,
            ));
        }
        if action == WorkspaceAction::DeleteWorktree {
            let result = self.endpoints[self.selected_endpoint]
                .connection
                .request_dialog(
                    &target.boot_id,
                    Method::WorktreeList,
                    serde_json::json!({"workspace_id": target.id, "trust_repository": false}),
                );
            self.menu.deletion = Some(Deletion {
                pending: result.as_ref().ok().cloned(),
                path: None,
                force: self.removal.as_ref().is_some_and(|removal| {
                    removal.force
                        && removal.workspace == target.id
                        && removal.boot_id == target.boot_id
                }),
            });
            self.menu.error = result.err().map(|error| error.to_string());
        }
        cx.notify();
    }

    pub(super) fn workspace_menu_actions(&self) -> Vec<WorkspaceMenuAction> {
        let mut actions: Vec<_> = self
            .workspace_items()
            .into_iter()
            .map(|(action, _)| action)
            .collect();
        if self.pr_profile().is_some() && self.menu.pr.value.is_some() {
            actions.push(WorkspaceMenuAction::PullRequest);
        }
        actions
    }

    pub(super) fn activate_workspace_menu(
        &mut self,
        action: WorkspaceMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            WorkspaceMenuAction::Dialog(action) => self.open_workspace_dialog(action, window, cx),
            WorkspaceMenuAction::Collapse | WorkspaceMenuAction::Expand => {
                self.toggle_selected_group(cx)
            }
            WorkspaceMenuAction::PullRequest => self.open_workspace_pr(cx),
            WorkspaceMenuAction::Teleport => self.open_teleport(window, cx),
            WorkspaceMenuAction::GoToTeleported => self.go_to_teleported(window, cx),
            WorkspaceMenuAction::TeleportBack => self.teleport_back(window, cx),
            WorkspaceMenuAction::ClearTeleported => self.clear_teleport_mark(window, cx),
        }
    }

    /// The dialog closes as soon as a removal is queued, so its response is
    /// tracked on the window instead. Only a refusal is reported: success shows
    /// up as the workspace leaving the daemon's snapshot.
    pub(super) fn update_pending_removal(&mut self, cx: &mut Context<Self>) {
        let Some(removal) = &self.removal else {
            return;
        };
        let current = removal.endpoint
            == (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            )
            && self.live.status.is_connected()
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == removal.boot_id);
        if !current {
            self.removal = None;
            return;
        }
        let Some((id, Some(result))) = &self.live.dialog_response else {
            return;
        };
        if removal.pending.as_deref() != Some(id.as_str()) {
            return;
        }
        let result = result.clone();
        let Some(removal) = self.removal.take() else {
            return;
        };
        let (force, error) = match result {
            Err(error) => (removal.force, error.to_string()),
            Ok(response) => {
                let Some(error) = response.get("error") else {
                    return;
                };
                let (code, message) = endpoint_error(error);
                (
                    removal.force || code == "dirty_worktree_requires_force",
                    format!("{code}: {message}"),
                )
            }
        };
        if force {
            // Keep the record so reopening the dialog asks for the force removal
            // rather than repeating the refused one.
            self.removal = Some(Removal {
                pending: None,
                force,
                ..removal
            });
        }
        self.local_error = Some(format!("Remove worktree: {error}"));
        cx.notify();
    }

    /// The checkout the daemon would create for the branch currently drafted.
    /// Only a preview: the daemon derives the path it actually uses.
    pub(super) fn checkout_preview(&self) -> String {
        let repo = self
            .menu
            .target
            .as_ref()
            .and_then(|target| target.worktree.as_ref())
            .map(|worktree| worktree.label.as_str());
        let root = self
            .live
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.worktree_directory.as_str())
            .filter(|root| !root.is_empty());
        let branch = self
            .menu
            .input
            .as_ref()
            .map_or("", |input| input.text.trim());
        match (repo, root) {
            _ if branch.is_empty() => "The daemon names the checkout.".to_owned(),
            (Some(repo), Some(root)) => crate::worktree::checkout_preview(root, repo, branch),
            _ => "The daemon chooses the checkout.".to_owned(),
        }
    }

    /// Apply the daemon's answer to whichever worktree dialog is waiting for it,
    /// and to a removal whose dialog has already closed.
    pub(crate) fn update_workspace_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.update_pending_removal(cx);
        if let Some(check) = &mut self.menu.close_check
            && check.poll()
        {
            if check
                .report
                .as_ref()
                .is_some_and(|report| report.needs_consent())
            {
                self.menu.input = Some(DialogInput::new(String::new()));
            }
            cx.notify();
        }
        if self.menu.worktree_open.is_some() && !self.worktree_open_current() {
            self.dismiss_menu(window, cx);
            return;
        }
        if self
            .menu
            .worktree_open
            .as_ref()
            .is_some_and(|picker| picker.pending.is_some())
        {
            if let Some((id, Some(result))) = &self.live.dialog_response {
                self.menu.apply_worktree_list_response(id, result.clone());
                cx.notify();
            }
            return;
        }
        self.apply_checkout_list(cx);
        if self.menu.deletion.is_none() && self.menu.creation.is_none() {
            return;
        }
        if !self.menu_target_current()
            || !self.live.status.is_connected()
            || self.menu.target.as_ref().is_some_and(|target| {
                self.live
                    .snapshot
                    .as_ref()
                    .is_none_or(|snapshot| snapshot.boot_id != target.boot_id)
            })
        {
            self.menu.reset();
            return;
        }
        let Some((id, Some(result))) = &self.live.dialog_response else {
            return;
        };
        if self.menu.deletion.is_some() {
            let (id, result) = (id.clone(), result.clone());
            self.menu.apply_deletion_response(&id, result);
            return;
        }
        if self.menu.creation.as_deref() != Some(id.as_str()) {
            return;
        }
        let result = result.clone();
        self.menu.creation = None;
        self.apply_creation_response(result, window, cx);
    }

    /// Follow the checkout the daemon created or opened. The daemon switches its own
    /// session, but this client shell keeps its own location, so the new
    /// workspace is only selected (and revealed in the sidebar) once this
    /// client focuses it.
    pub(super) fn apply_creation_response(
        &mut self,
        result: crate::state::DialogResponse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A refused creation leaves the dialog open, so the row it was for is
        // released and the listing can be used again.
        let release = |this: &mut Self, error: String, cx: &mut Context<Self>| {
            if let Some(source) = &mut this.menu.worktree {
                source.pending = None;
            }
            this.menu.error = Some(error);
            cx.notify();
        };
        let response = match result {
            Ok(response) => response,
            Err(error) => return release(self, error.to_string(), cx),
        };
        if let Some(error) = response.get("error") {
            let (code, message) = endpoint_error(error);
            return release(self, format!("{code}: {message}"), cx);
        }
        let result = &response["result"];
        let opening = self.menu.page == Some(Page::Dialog(WorkspaceAction::OpenWorktree))
            || self
                .menu
                .worktree
                .as_ref()
                .and_then(|source| source.pending.as_ref())
                .is_some_and(|pending| pending.opens());
        let expected = if opening {
            "worktree_opened"
        } else {
            "worktree_created"
        };
        let created = (result["type"] == expected)
            .then(|| result["workspace"]["workspace_id"].as_str())
            .flatten()
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
        let Some(created) = created else {
            return release(
                self,
                "Unexpected daemon response. Review current workspace state before retrying."
                    .into(),
                cx,
            );
        };
        // The note names what the checkout is for, so it is taken from the
        // dialog's own pending row before dismissal drops it.
        if !opening {
            self.write_worktree_note(result, cx);
        }
        let endpoint = self.endpoints[self.selected_endpoint].id.clone();
        // A folded group would hide the new checkout the sidebar is about to select.
        let group = self
            .menu
            .target
            .as_ref()
            .and_then(|target| target.worktree.as_ref())
            .map(|worktree| worktree.key.clone())
            .or_else(|| {
                self.menu
                    .worktree_open
                    .as_ref()?
                    .source
                    .as_ref()
                    .map(|source| source.repo_key.clone())
            });
        self.dismiss_menu(window, cx);
        if let Some(group) = group {
            self.collapsed_repos_mut().remove(&group);
        }
        self.navigate_endpoint(&endpoint, NavigationTarget::Workspace(&created), cx);
    }

    pub(super) fn submit_workspace_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Page::Dialog(action)) = self.menu.page else {
            return;
        };
        if action == WorkspaceAction::OpenWorktree
            && self
                .menu
                .worktree_open
                .as_ref()
                .is_some_and(|picker| picker.search.read(cx).is_composing())
        {
            return;
        }
        if self
            .menu
            .input
            .as_ref()
            .is_some_and(|input| input.marked.is_some())
            || self
                .menu
                .worktree
                .as_ref()
                .is_some_and(|source| source.name.read(cx).is_composing())
        {
            return;
        }
        let name = self.worktree_name(cx);
        let result = (|| {
            if !self.menu_target_current() {
                return Err(crate::Error::StaleConnection);
            }
            if !self.live.status.is_connected() {
                return Err(crate::Error::NotConnected);
            }
            let target = self
                .menu
                .target
                .as_ref()
                .ok_or(crate::Error::StaleWorkspace)?;
            let snapshot = self
                .live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NoSnapshot)?;
            let text = if action == WorkspaceAction::OpenWorktree {
                self.menu
                    .worktree_open
                    .as_ref()
                    .filter(|picker| picker.pending.is_none())
                    .and_then(|picker| picker.entry(picker.selected))
                    .map(|entry| entry.path.as_str())
                    .ok_or(crate::Error::WorktreeSelection)?
            } else {
                self.menu
                    .input
                    .as_ref()
                    .map(|input| input.text.as_str())
                    .unwrap_or("")
            };
            let (method, mut params) = target.request(snapshot, action, text)?;
            // The daemon names the workspace as it creates it, so no rename follows.
            if action == WorkspaceAction::NewWorktree
                && let Some(name) = name
            {
                params["label"] = name.into();
            }
            if matches!(
                action,
                WorkspaceAction::NewTab | WorkspaceAction::NewWorkspace
            ) && let Some(label) = chosen_label(text, self.menu.suggested_name.as_deref())
            {
                params["label"] = label.into();
            }
            if action == WorkspaceAction::Close {
                let Some(check) = &self.menu.close_check else {
                    return Ok(Submission::Awaiting {
                        focus_changed: false,
                    });
                };
                if !check.current(snapshot, target) {
                    return Err(crate::Error::WorkspaceGroupChanged);
                }
                if !check.ready(text) {
                    return Ok(Submission::Awaiting {
                        focus_changed: false,
                    });
                }
            }
            if action == WorkspaceAction::DeleteWorktree {
                let deletion = self
                    .menu
                    .deletion
                    .as_ref()
                    .ok_or(crate::Error::MissingDeletion)?;
                if deletion.pending.is_some() {
                    return Ok(Submission::Awaiting {
                        focus_changed: false,
                    });
                }
                if !deletion.ready() {
                    return Err(crate::Error::DeletionLookup);
                }
                let force = deletion.force;
                params["force"] = force.into();
                let pending = self.endpoints[self.selected_endpoint]
                    .connection
                    .request_dialog(&target.boot_id, method, params)?;
                self.removal = Some(Removal {
                    endpoint: (
                        self.selection_epoch,
                        self.endpoints[self.selected_endpoint].generation,
                    ),
                    boot_id: target.boot_id.clone(),
                    workspace: target.id.clone(),
                    pending: Some(pending),
                    force,
                });
                return Ok(Submission::Queued {
                    focus_changed: true,
                });
            }
            if matches!(
                action,
                WorkspaceAction::NewWorktree | WorkspaceAction::OpenWorktree
            ) {
                if self.menu.creation.is_some() {
                    return Ok(Submission::Awaiting {
                        focus_changed: false,
                    });
                }
                // Correlated, so the daemon's failure reaches the dialog and the
                // created checkout can be focused once it exists.
                let id = self.endpoints[self.selected_endpoint]
                    .connection
                    .request_dialog(&target.boot_id, method, params)?;
                self.menu.creation = Some(id);
                self.menu.error = None;
                if let Some(source) = &mut self.menu.worktree {
                    source.superseded();
                }
                return Ok(Submission::Awaiting {
                    focus_changed: true,
                });
            }
            let handle = self.endpoints[self.selected_endpoint]
                .connection
                .handle
                .as_ref()
                .ok_or(crate::Error::NotConnected)?;
            handle
                .request(&target.boot_id, method, params)
                .map(|_| Submission::Queued {
                    focus_changed: action != WorkspaceAction::Rename,
                })
                .map_err(|source| crate::Error::Request { method, source })
        })();
        match result {
            Ok(submission) => {
                self.local_error = None;
                if submission.focus_changed() {
                    self.fence_focus_change(None);
                }
                match submission {
                    // The daemon's snapshot drops the workspace when the removal
                    // lands, so there is nothing left to wait for here.
                    Submission::Queued { .. } => self.dismiss_menu(window, cx),
                    Submission::Awaiting { .. } => cx.notify(),
                }
            }
            Err(error) => {
                self.menu.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    pub(super) fn render_workspace_dialog(
        &self,
        action: WorkspaceAction,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(target) = &self.menu.target else {
            return div();
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        let danger = danger(theme);
        let deletion = self.menu.deletion.as_ref();
        let force = deletion.is_some_and(|deletion| deletion.force);
        let creating = self.menu.creation.is_some();
        // Until the daemon names the checkout there is nothing to confirm, and a
        // request already in flight leaves nothing to press either.
        let armed = (action != WorkspaceAction::DeleteWorktree
            || deletion.is_some_and(|deletion| deletion.ready()))
            && (action != WorkspaceAction::OpenWorktree
                || self.menu.worktree_open.as_ref().is_some_and(|picker| {
                    picker.pending.is_none()
                        && !picker.filtered.is_empty()
                        && !picker.search.read(cx).is_composing()
                }))
            && (action != WorkspaceAction::Close
                || self.menu.close_check.as_ref().is_some_and(|check| {
                    check.ready(
                        self.menu
                            .input
                            .as_ref()
                            .map_or("", |input| input.text.as_str()),
                    )
                }))
            && !creating;
        let destructive = matches!(
            action,
            WorkspaceAction::Close | WorkspaceAction::DeleteWorktree
        );
        let (title, submit) = match action {
            WorkspaceAction::Rename => ("Rename workspace", "Rename"),
            WorkspaceAction::Close => (target.close_label(), target.close_label()),
            WorkspaceAction::NewWorktree => ("New worktree", "Create"),
            WorkspaceAction::OpenWorktree => ("Open worktree", "Open"),
            WorkspaceAction::NewTab => ("New tab", "Create"),
            WorkspaceAction::NewWorkspace => ("New workspace", "Create"),
            WorkspaceAction::DeleteWorktree if force => ("Force delete checkout?", "Force remove"),
            WorkspaceAction::DeleteWorktree => ("Delete worktree checkout?", "Remove"),
        };
        let mut body = div().flex().flex_col().gap(px(10.)).px(px(16.)).py(px(12.));
        body = match action {
            WorkspaceAction::OpenWorktree => body,
            WorkspaceAction::Rename => {
                body.child(div().text_color(rgb(theme.muted)).child("Edit the workspace label."))
            }
            WorkspaceAction::NewTab => body.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("Name the tab, or leave the suggestion for Herdr to number it."),
            ),
            WorkspaceAction::NewWorkspace => body.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("Name the workspace, or leave the suggestion for Herdr to name it."),
            ),
            WorkspaceAction::Close => body.child(div().text_color(rgb(theme.muted)).child(format!(
                "Closes {} workspace(s) and terminates their running terminals. Checkout files and branches are not deleted.",
                target.close_members.len()
            ))),
            WorkspaceAction::NewWorktree => {
                // Captions share a column so both fields start at the same edge.
                let row = |caption: &'static str, field: AnyElement| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex_none()
                                .w(px(font.size * 4.5))
                                .text_color(rgb(theme.muted))
                                .child(caption),
                        )
                        .child(div().flex_1().min_w_0().child(field))
                };
                body.children(self.menu.worktree.as_ref().map(|source| {
                    row(
                        "Name",
                        div()
                            .debug_selector(|| "worktree-name".into())
                            .child(source.name.clone())
                            .into_any_element(),
                    )
                }))
                .child(row("Branch", self.render_dialog_input(cx).into_any_element()))
                .child(format!(
                    "Creates this folder from {}, without granting repository trust:",
                    target.base_label()
                ))
                .child(
                    div()
                        .debug_selector(|| "dialog-checkout".into())
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .px(px(10.))
                        .py(px(6.))
                        .child(self.checkout_preview()),
                )
            }
            WorkspaceAction::DeleteWorktree => body
                .child(if force {
                    "This force removes the checkout folder:"
                } else {
                    "This removes the checkout folder:"
                })
                .child(
                    div()
                        .debug_selector(|| "dialog-path".into())
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .px(px(10.))
                        .py(px(6.))
                        .child(
                            deletion
                                .and_then(|deletion| deletion.path.as_deref())
                                .unwrap_or("Waiting for the daemon to name the checkout...")
                                .to_owned(),
                        ),
                )
                .child(div().text_color(rgb(theme.muted)).child(if force {
                    "Modified and untracked files, including submodule contents, are discarded. The branch is not deleted. The Herdr workspace will close."
                } else {
                    "The branch is not deleted. The Herdr workspace will close."
                })),
        };
        if action == WorkspaceAction::Close {
            body = body.child(div().debug_selector(|| "close-git-status".into()).text_color(danger).child(
                match self.menu.close_check.as_ref().and_then(|check| check.report.as_ref()) {
                    None => "Checking for uncommitted files and unpushed commits...".to_owned(),
                    Some(report) => {
                        let mut warnings = Vec::new();
                        if report.dirty { warnings.push("Uncommitted files are present."); }
                        if report.unpushed { warnings.push("Unpushed commits are present."); }
                        if report.unknown { warnings.push("Git status could not be verified for every checkout."); }
                        if report.needs_consent() { warnings.push("Type close to consent to closing anyway, or Cancel to keep working."); }
                        else { warnings.push("No uncommitted files or unpushed commits found (using local remote-tracking refs)."); }
                        warnings.join(" ")
                    }
                }
            ));
        }
        if self.menu.input.is_some() && action != WorkspaceAction::NewWorktree {
            body = body.child(self.render_dialog_input(cx));
        }
        if let Some(error) = &self.menu.error {
            body = body.child(
                div()
                    .debug_selector(|| "dialog-error".into())
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.active))
                    .px(px(10.))
                    .py(px(6.))
                    .text_color(danger)
                    .child(error.clone()),
            );
        }
        if creating {
            // Dismissing only closes the panel; the daemon keeps the queued work.
            body = body.child(
                div()
                    .debug_selector(|| "dialog-waiting".into())
                    .text_color(rgb(theme.muted))
                    .child("Waiting for the daemon. Dismissing does not cancel it."),
            );
        }
        let button = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(12.))
                .py(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .cursor_pointer()
        };
        // A picker owns the panel's height, so its list scrolls inside the
        // dialog instead of growing it past the window.
        let listing = action == WorkspaceAction::OpenWorktree || self.worktree_listing();
        div()
            .flex()
            .flex_col()
            .when(
                listing || action == WorkspaceAction::NewWorktree,
                |dialog| dialog.size_full().min_h_0(),
            )
            .child(
                div()
                    .debug_selector(|| "dialog-header".into())
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(12.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .when_some(
                        WorkspaceMenuAction::Dialog(action).icon(),
                        |header, icon| {
                            header.child(svg().path(icon).size(px(16.)).flex_none().text_color(
                                if destructive {
                                    danger
                                } else {
                                    rgb(theme.muted)
                                },
                            ))
                        },
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .debug_selector(|| "dialog-title".into())
                                    .when(action == WorkspaceAction::OpenWorktree, |title| {
                                        title.truncate()
                                    })
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(theme.muted))
                                    .child(target.label.clone()),
                            ),
                    )
                    .when(action == WorkspaceAction::OpenWorktree, |header| {
                        header.child(
                            div()
                                .id("open-worktree-escape")
                                .debug_selector(|| "open-worktree-escape".into())
                                .flex_none()
                                .px_2()
                                .py_1()
                                .rounded(px(crate::config::corners::CONTROL))
                                .cursor_pointer()
                                .text_color(rgb(theme.muted))
                                .hover(|button| {
                                    button
                                        .bg(rgb(theme.active))
                                        .text_color(rgb(theme.foreground))
                                })
                                .child("ESC")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.dismiss_menu(window, cx);
                                })),
                        )
                    }),
            )
            .when(action == WorkspaceAction::NewWorktree, |dialog| {
                dialog.child(self.render_worktree_tabs(cx))
            })
            .child(if action == WorkspaceAction::OpenWorktree {
                self.render_existing_worktrees(cx)
                    .when(creating || self.menu.error.is_some(), |picker| {
                        picker.child(
                            div()
                                .id("open-worktree-status")
                                .debug_selector(|| "open-worktree-status".into())
                                .flex_none()
                                .h(px(font.line_height() * 3. + 20.))
                                .overflow_y_scroll()
                                .child(body),
                        )
                    })
                    .into_any_element()
            } else if listing {
                self.render_worktree_items(cx).into_any_element()
            } else if action == WorkspaceAction::NewWorktree {
                // The branch form shares the listings' settled height, so it
                // fills the space above the footer and scrolls within it.
                div()
                    .id("new-worktree-form")
                    .debug_selector(|| "new-worktree-form".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body)
                    .into_any_element()
            } else {
                body.into_any_element()
            })
            .child(
                div()
                    .debug_selector(|| "dialog-footer".into())
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .px(px(16.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .child(
                        button("dialog-cancel")
                            .border_color(rgb(theme.active))
                            .hover(|button| button.bg(rgb(theme.active)))
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    // GitHub rows create their own checkouts without a submit button.
                    .when(
                        !listing || action == WorkspaceAction::OpenWorktree,
                        |footer| {
                            footer.child(
                                button("dialog-submit")
                                    // The primary action carries the fill; a
                                    // destructive one also carries the warning hue.
                                    .border_color(if !armed {
                                        rgb(theme.active)
                                    } else if destructive {
                                        danger
                                    } else {
                                        rgb(theme.foreground)
                                    })
                                    .when(armed, |button| button.bg(rgb(theme.active)))
                                    .text_color(if !armed {
                                        rgb(theme.muted)
                                    } else if destructive {
                                        danger
                                    } else {
                                        rgb(theme.foreground)
                                    })
                                    .child(if creating { "Waiting..." } else { submit })
                                    // Faded and inert until there is something
                                    // to submit, so it never reads as pressable.
                                    .when(!armed, |button| {
                                        button.opacity(0.4).cursor(CursorStyle::OperationNotAllowed)
                                    })
                                    .when(armed, |button| {
                                        button
                                            .hover(|button| button.bg(rgb(theme.active)))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.submit_workspace_dialog(window, cx);
                                            }))
                                    }),
                            )
                        },
                    ),
            )
    }
}
