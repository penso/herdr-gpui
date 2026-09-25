//! The workspace a menu row targets: what may be created or deleted on it,
//! which sibling workspaces close with it, and the dialogs that carry those
//! requests to the daemon and report what came back.

use super::{
    Page, Removal, Submission, WorkspaceAction, WorkspaceMenuAction, dialog_buttons,
    endpoint_error, error_alert, listener, state::Deletion, submit,
};
use crate::{HerdrWindow, NavigationTarget};
use gpui_kit::{
    component::{
        ActiveTheme as _, Disableable as _,
        alert::Alert,
        button::{Button, ButtonVariants as _},
        dialog::Dialog,
        form::{field as form_field, v_form},
        input::Input,
        menu::PopupMenuItem,
        v_flex,
    },
    prelude::*,
    *,
};
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
    pub(super) branch: Option<String>,
}

impl WorkspaceTarget {
    pub(super) fn new(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> Self {
        Self {
            boot_id: snapshot.boot_id.clone(),
            id: workspace.workspace_id.clone(),
            label: workspace.label.clone(),
            worktree: workspace.worktree.clone(),
            close_members: close_members(snapshot, workspace),
            branch: workspace.branch.clone(),
        }
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

    /// The worktree key this workspace heads, when other checkouts hang off it.
    pub(super) fn group_key(&self) -> Option<&str> {
        self.worktree
            .as_ref()
            .filter(|tree| !tree.is_linked_worktree && self.close_members.len() > 1)
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
                (
                    Method::WorkspaceClose,
                    serde_json::json!({"workspace_id": self.id, "close_group": true}),
                )
            }
            WorkspaceAction::NewWorktree => {
                self.validate_repository(snapshot)?;
                let mut params = serde_json::json!({"workspace_id": self.id, "base": "HEAD", "focus": true, "trust_repository": false});
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
}

impl NewWorktreeUnavailable {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Disconnected => "Not connected, so no worktree can be created",
            Self::NoWorkspace => "No workspace is focused to create a worktree from",
            Self::NotGit => "This workspace is not a Git repository",
            Self::MainCheckoutClosed => "Open this repository's main checkout to create a worktree",
        }
    }
}

/// The workspace a new worktree for the focused one is created from.
fn new_worktree_source(snapshot: &ClientShellSnapshot) -> Result<String, NewWorktreeUnavailable> {
    let focused = snapshot
        .workspaces
        .iter()
        .find(|w| Some(&w.workspace_id) == snapshot.focused_workspace_id.as_ref())
        .ok_or(NewWorktreeUnavailable::NoWorkspace)?;
    let source = match &focused.worktree {
        Some(tree) if tree.is_linked_worktree => snapshot
            .workspaces
            .iter()
            .find(|w| {
                w.worktree
                    .as_ref()
                    .is_some_and(|other| other.key == tree.key && !other.is_linked_worktree)
            })
            .ok_or(NewWorktreeUnavailable::MainCheckoutClosed)?,
        _ => focused,
    };
    if !WorkspaceTarget::new(snapshot, source).can_create() {
        return Err(NewWorktreeUnavailable::NotGit);
    }
    Ok(source.workspace_id.clone())
}

fn close_members(snapshot: &ClientShellSnapshot, workspace: &ClientShellWorkspace) -> Vec<String> {
    let mut members: Vec<_> = snapshot
        .workspaces
        .iter()
        .filter(|w| {
            w.workspace_id == workspace.workspace_id
                || workspace.worktree.as_ref().is_some_and(|tree| {
                    !tree.is_linked_worktree
                        && w.worktree
                            .as_ref()
                            .is_some_and(|other| tree.key == other.key)
                })
        })
        .map(|w| w.workspace_id.clone())
        .collect();
    members.sort();
    members
}

impl HerdrWindow {
    /// Captures the workspace `id` as the target of a new menu page. False when
    /// another page is open or the workspace is gone.
    fn begin_workspace(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.menu_is_open() || !self.live.status.is_connected() {
            return false;
        }
        let Some(target) = self.live.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .workspaces
                .iter()
                .find(|w| w.workspace_id == id)
                .map(|workspace| WorkspaceTarget::new(snapshot, workspace))
        }) else {
            return false;
        };
        self.hover = None;
        self.hover_menu = None;
        if !self.begin_menu(window, cx) {
            return false;
        }
        self.menu.target = Some(target);
        true
    }

    pub(crate) fn open_workspace_menu(
        &mut self,
        id: &str,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.begin_workspace(id, window, cx) {
            return;
        }
        self.refresh_workspace_pr();
        let Some(target) = &self.menu.target else {
            return;
        };
        let label = target.label.clone();
        let branch = target
            .branch
            .as_deref()
            .filter(|branch| !branch.trim().is_empty())
            .map(str::to_owned);
        let items = self.workspace_items();
        let github = self.menu.github.connected();
        let weak = cx.weak_entity();
        self.show_popup(Page::Workspace, anchor, window, cx, move |menu, _, _| {
            let menu = menu
                .min_w(px(260.))
                .max_w(px(340.))
                .item(
                    PopupMenuItem::element(move |_, cx| {
                        v_flex()
                            .debug_selector(|| "workspace-menu-header".into())
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(label.clone()),
                            )
                            .children(branch.clone().map(|branch| {
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(branch)
                            }))
                    })
                    .disabled(true),
                )
                .separator();
            let menu = items.into_iter().fold(menu, |menu, (action, label)| {
                menu.item(
                    PopupMenuItem::new(label)
                        .when_some(action.icon(), |item, icon| item.icon(icon))
                        .on_click(listener(&weak, move |this, window, cx| {
                            this.activate_workspace_menu(action, window, cx)
                        })),
                )
            });
            if !github {
                return menu;
            }
            let pr = weak.clone();
            menu.separator().item(
                PopupMenuItem::element(move |_, cx| {
                    pr.upgrade()
                        .map(|view| view.read(cx).render_workspace_pr(cx))
                        .unwrap_or_else(|| div().into_any_element())
                })
                .on_click(listener(&weak, |this, window, cx| {
                    this.activate_workspace_menu(WorkspaceMenuAction::PullRequest, window, cx)
                })),
            )
        });
    }

    /// Opens the new worktree dialog for the focused workspace, as its menu's
    /// "New worktree" row would. A linked checkout offers no such row, so its
    /// repository's main checkout seeds the worktree instead.
    /// When there is none, a flash says why rather than the shortcut doing
    /// nothing visible.
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
        if self.begin_workspace(&id, window, cx) {
            self.open_workspace_dialog(WorkspaceAction::NewWorktree, window, cx);
        }
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
        }
        if target.can_delete() {
            items.push((
                Dialog(WorkspaceAction::DeleteWorktree),
                "Delete worktree checkout",
            ));
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

    pub(super) fn toggle_selected_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.dismiss_menu(window, cx);
    }

    pub(crate) fn open_workspace_dialog(
        &mut self,
        action: WorkspaceAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = &self.menu.target else {
            return;
        };
        let text = match action {
            WorkspaceAction::Rename => Some((target.label.clone(), "Workspace name")),
            // Propose the daemon's own branch shape, selected so typing replaces it.
            WorkspaceAction::NewWorktree => Some((crate::worktree::proposed_branch(), "Branch")),
            WorkspaceAction::Close
            | WorkspaceAction::DeleteWorktree
            | WorkspaceAction::OpenWorktree => None,
        };
        self.menu.input = None;
        if let Some((text, placeholder)) = text {
            self.set_menu_input(text, placeholder, window, cx);
        }
        self.menu.pr.clear();
        self.menu.pr_connection = None;
        self.menu.error = None;
        if action == WorkspaceAction::Close
            && let Some(snapshot) = &self.live.snapshot
            && let Some(target) = &self.menu.target
        {
            self.menu.close_check = Some(super::workspace_close::CloseCheck::start(
                snapshot,
                target,
                self.selected_endpoint == 0 && self.live.local_daemon_peer,
                cx,
            ));
        }
        if action == WorkspaceAction::DeleteWorktree
            && let Some(target) = &self.menu.target
        {
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
        self.show_dialog(
            Page::Dialog(action),
            window,
            cx,
            move |this, dialog, weak, _, cx| this.workspace_dialog(action, dialog, weak, cx),
        );
        match action {
            WorkspaceAction::OpenWorktree => self.open_existing_worktrees(window, cx),
            WorkspaceAction::NewWorktree => {
                self.open_worktree_source(window, cx);
                // The branch proposal stays selected for when it gets focus,
                // but the name is what most people change, so the form opens on it.
                self.focus_menu_input(window, cx);
                if let Some(source) = &self.menu.worktree {
                    let name = source.name.clone();
                    name.update(cx, |name, cx| name.focus(window, cx));
                }
            }
            _ => self.focus_menu_input(window, cx),
        }
        cx.notify();
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
                self.toggle_selected_group(window, cx)
            }
            WorkspaceMenuAction::PullRequest => {
                self.open_workspace_pr(cx);
                self.dismiss_menu(window, cx);
            }
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
    pub(super) fn checkout_preview(&self, cx: &App) -> String {
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
        let text = self.menu.input_text(cx);
        let branch = text.trim();
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
                self.set_menu_input("", "close", window, cx);
                self.focus_menu_input(window, cx);
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
            self.dismiss_menu(window, cx);
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
        let text = self.menu.input_text(cx);
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
                text.as_str()
            };
            let (method, mut params) = target.request(snapshot, action, text)?;
            // The daemon names the workspace as it creates it, so no rename follows.
            if action == WorkspaceAction::NewWorktree
                && let Some(name) = name
            {
                params["label"] = name.into();
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

    /// A workspace dialog, rebuilt every frame from the window's state.
    fn workspace_dialog(
        &self,
        action: WorkspaceAction,
        dialog: Dialog,
        weak: &WeakEntity<HerdrWindow>,
        cx: &App,
    ) -> Dialog {
        let Some(target) = &self.menu.target else {
            return dialog;
        };
        let muted = cx.theme().muted_foreground;
        let deletion = self.menu.deletion.as_ref();
        let force = deletion.is_some_and(|deletion| deletion.force);
        let creating = self.menu.creation.is_some();
        let text = self.menu.input_text(cx);
        // Until the daemon names the checkout there is nothing to confirm, and a
        // request already in flight leaves nothing to press either.
        let armed =
            (action != WorkspaceAction::DeleteWorktree
                || deletion.is_some_and(|deletion| deletion.ready()))
                && (action != WorkspaceAction::OpenWorktree
                    || self.menu.worktree_open.as_ref().is_some_and(|picker| {
                        picker.pending.is_none() && !picker.filtered.is_empty()
                    }))
                && (action != WorkspaceAction::Close
                    || self
                        .menu
                        .close_check
                        .as_ref()
                        .is_some_and(|check| check.ready(&text)))
                && !creating;
        let destructive = matches!(
            action,
            WorkspaceAction::Close | WorkspaceAction::DeleteWorktree
        );
        let (title, submit_label) = match action {
            WorkspaceAction::Rename => ("Rename workspace", "Rename"),
            WorkspaceAction::Close => (target.close_label(), target.close_label()),
            WorkspaceAction::NewWorktree => ("New worktree", "Create"),
            WorkspaceAction::OpenWorktree => ("Open worktree", "Open"),
            WorkspaceAction::DeleteWorktree if force => ("Force delete checkout?", "Force remove"),
            WorkspaceAction::DeleteWorktree => ("Delete worktree checkout?", "Remove"),
        };
        let field = || {
            self.menu
                .input
                .as_ref()
                .map(|input| Input::new(input).disabled(creating))
        };
        let mut body = if matches!(
            action,
            WorkspaceAction::NewWorktree | WorkspaceAction::OpenWorktree
        ) {
            super::worktree_render::list_keys(v_flex().gap_3(), weak)
        } else {
            v_flex().gap_3()
        };
        body = match action {
            WorkspaceAction::OpenWorktree => body.child(self.render_existing_worktrees(weak, cx)),
            WorkspaceAction::Rename => body
                .child(div().text_color(muted).child("Edit the workspace label."))
                .children(field()),
            WorkspaceAction::Close => body.child(div().text_color(muted).child(format!(
                "Closes {} workspace(s) and terminates their running terminals. Checkout files and branches are not deleted.",
                target.close_members.len()
            ))),
            WorkspaceAction::NewWorktree => {
                let form = v_flex()
                    .gap_3()
                    .child(
                        v_form()
                            .label_layout(Axis::Horizontal)
                            .children(self.menu.worktree.as_ref().map(|source| {
                                form_field().label("Name").child(
                                    div()
                                        .debug_selector(|| "worktree-name".into())
                                        .child(Input::new(&source.name).disabled(creating)),
                                )
                            }))
                            .children(field().map(|input| form_field().label("Branch").child(input))),
                    )
                    .child("Creates this folder from HEAD, without granting repository trust:")
                    .child(
                        div()
                            .debug_selector(|| "dialog-checkout".into())
                            .text_sm()
                            .text_color(muted)
                            .child(self.checkout_preview(cx)),
                    );
                body.child(self.render_worktree_tabs(weak, cx)).child(
                    if self.worktree_listing() {
                        self.render_worktree_items(weak, cx).into_any_element()
                    } else {
                        form.into_any_element()
                    },
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
                        .text_sm()
                        .child(
                            deletion
                                .and_then(|deletion| deletion.path.as_deref())
                                .unwrap_or("Waiting for the daemon to name the checkout...")
                                .to_owned(),
                        ),
                )
                .child(div().text_color(muted).child(if force {
                    "Modified and untracked files, including submodule contents, are discarded. The branch is not deleted. The Herdr workspace will close."
                } else {
                    "The branch is not deleted. The Herdr workspace will close."
                })),
        };
        if action == WorkspaceAction::Close {
            let report = self
                .menu
                .close_check
                .as_ref()
                .and_then(|check| check.report.as_ref());
            let status = match report {
                None => "Checking for uncommitted files and unpushed commits...".to_owned(),
                Some(report) => {
                    let mut warnings = Vec::new();
                    if report.dirty {
                        warnings.push("Uncommitted files are present.");
                    }
                    if report.unpushed {
                        warnings.push("Unpushed commits are present.");
                    }
                    if report.unknown {
                        warnings.push("Git status could not be verified for every checkout.");
                    }
                    if report.needs_consent() {
                        warnings.push(
                            "Type close to consent to closing anyway, or Cancel to keep working.",
                        );
                    } else {
                        warnings.push(
                            "No uncommitted files or unpushed commits found (using local remote-tracking refs).",
                        );
                    }
                    warnings.join(" ")
                }
            };
            body = body
                .child(if report.is_some_and(|report| report.needs_consent()) {
                    Alert::warning("close-git-status", status)
                } else {
                    Alert::info("close-git-status", status)
                })
                .children(field());
        }
        body = body.children(error_alert("dialog-error", self.menu.error.as_ref()));
        if creating {
            // Dismissing only closes the panel; the daemon keeps the queued work.
            body = body.child(
                div()
                    .debug_selector(|| "dialog-waiting".into())
                    .text_color(muted)
                    .child("Waiting for the daemon. Dismissing does not cancel it."),
            );
        }
        // GitHub rows create their own checkouts without a submit button.
        let listing = self.worktree_listing();
        let confirm = (!listing).then(|| {
            Button::new("dialog-submit")
                .label(if creating { "Waiting..." } else { submit_label })
                .loading(creating)
                .disabled(!armed)
                .map(|button| {
                    if destructive {
                        button.danger()
                    } else {
                        button.primary()
                    }
                })
                .on_click(listener(weak, |this, window, cx| {
                    this.submit_workspace_dialog(window, cx)
                }))
        });
        dialog
            .title(
                v_flex().min_w_0().child(title).child(
                    div()
                        .truncate()
                        .text_sm()
                        .text_color(muted)
                        .child(target.label.clone()),
                ),
            )
            .when(
                matches!(
                    action,
                    WorkspaceAction::NewWorktree | WorkspaceAction::OpenWorktree
                ),
                |dialog| dialog.w(px(560.)),
            )
            .child(body)
            .footer(dialog_buttons(weak, confirm))
            .on_ok(submit(weak, move |this, window, cx| {
                if this.menu.creation.is_some() {
                    return;
                }
                if this.worktree_listing() {
                    let selected = this.menu.worktree.as_ref().map(|source| source.selected);
                    if let Some(selected) = selected {
                        this.pick_worktree_row(selected, cx);
                    }
                } else if !this.worktree_search_focused(window, cx) {
                    this.submit_workspace_dialog(window, cx);
                }
            }))
    }
}
