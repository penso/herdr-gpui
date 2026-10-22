//! Herdr's `open_worktree` and `remove_worktree` shortcuts: each acts on the
//! focused workspace, as in the TUI, by opening the dialog that workspace's
//! menu row would open, confirmation included.

use super::{Page, WorkspaceAction, workspace::WorkspaceTarget};
use crate::HerdrWindow;
use gpui::{Context, Point, Window};
use herdr_client::protocol::ClientShellSnapshot;

/// A worktree shortcut, each opening one dialog of the workspace menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorktreeShortcut {
    /// "Open worktree...", listing the repository's other checkouts.
    Open,
    /// "Delete worktree checkout", asking for confirmation first.
    Remove,
}

/// Why a worktree shortcut found no workspace to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unavailable {
    Disconnected,
    NoWorkspace,
    NotGit,
    /// A linked checkout opens worktrees through its main checkout, which is
    /// not open.
    MainCheckoutClosed,
    /// Herdr removes only linked worktrees, never a repository's main checkout.
    MainCheckout,
}

impl Unavailable {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Disconnected => "Not connected, so no worktree can be changed",
            Self::NoWorkspace => "No workspace is focused",
            Self::NotGit => "This workspace is not a Git repository",
            Self::MainCheckoutClosed => "Open this repository's main checkout to open a worktree",
            Self::MainCheckout => "Only a linked worktree can be removed, not a main checkout",
        }
    }
}

impl WorktreeShortcut {
    fn action(self) -> WorkspaceAction {
        match self {
            Self::Open => WorkspaceAction::OpenWorktree,
            Self::Remove => WorkspaceAction::DeleteWorktree,
        }
    }

    /// The workspace whose menu row this shortcut stands in for. Herdr lists
    /// and opens checkouts only through a repository's main checkout, so a
    /// focused linked checkout opens through it, as a new worktree does.
    pub(super) fn target(self, snapshot: &ClientShellSnapshot) -> Result<String, Unavailable> {
        let focused = snapshot
            .workspaces
            .iter()
            .find(|w| Some(&w.workspace_id) == snapshot.focused_workspace_id.as_ref())
            .ok_or(Unavailable::NoWorkspace)?;
        // The same tests the menu uses to offer each row.
        let target = WorkspaceTarget::new(snapshot, focused);
        if !target.can_create() && !target.can_delete() {
            return Err(Unavailable::NotGit);
        }
        let linked = focused
            .worktree
            .as_ref()
            .filter(|tree| tree.is_linked_worktree);
        let row = match (self, linked) {
            (Self::Open, Some(tree)) => super::workspace::main_checkout(snapshot, tree)
                // A main checkout whose shell wandered off would list the
                // repository it moved to, so the dialog opens through the
                // linked checkout and names the repository's root instead.
                .or_else(|| {
                    snapshot
                        .workspaces
                        .iter()
                        .any(|w| super::workspace::is_main_of(w, tree))
                        .then_some(focused)
                })
                .ok_or(Unavailable::MainCheckoutClosed)?,
            (Self::Open, None) | (Self::Remove, Some(_)) => focused,
            (Self::Remove, None) => return Err(Unavailable::MainCheckout),
        };
        Ok(row.workspace_id.clone())
    }
}

impl HerdrWindow {
    /// Opens the dialog `shortcut` names for the focused workspace, as its
    /// menu row would, so removal still asks for a fresh confirmation. When
    /// there is no such row, a flash says why rather than the shortcut doing
    /// nothing visible.
    pub(crate) fn open_worktree_shortcut(
        &mut self,
        shortcut: WorktreeShortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match &self.live.snapshot {
            Some(snapshot) if self.live.status.is_connected() => shortcut.target(snapshot),
            _ => Err(Unavailable::Disconnected),
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
            self.open_workspace_dialog(shortcut.action(), window, cx);
        }
    }

    /// Asks to remove the focused `workspace`'s checkout because pull request
    /// `number` was merged, with the dialog its menu row opens, so the
    /// archive script and every refusal are handled as when asked directly.
    pub(crate) fn ask_merged_removal(
        &mut self,
        workspace: &str,
        number: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_menu(workspace, Point::default(), window, cx);
        if self.menu.page != Some(Page::Workspace) {
            return;
        }
        self.open_workspace_dialog(WorkspaceAction::DeleteWorktree, window, cx);
        if let Some(deletion) = &mut self.menu.deletion {
            deletion.merged = Some(number);
        }
    }
}
