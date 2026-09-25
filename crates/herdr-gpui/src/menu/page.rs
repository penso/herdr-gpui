//! What a menu popup or dialog is currently showing, and the workspace actions
//! a row can trigger. Closed sets, so a page is never a string tag. The kit
//! draws the surfaces; this enum is what fences input while one is open.

use gpui_kit::component::{Icon, IconName};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Page {
    Menu,
    About,
    Preferences,
    Devices,
    AddDevice,
    Keybinds,
    Themes,
    Palette,
    ConfirmClose,
    Update,
    AppUpdate,
    Install,
    Tab,
    RenameTab,
    Pane,
    RenamePane,
    Workspace,
    GitHub,
    /// Titlebar Git actions for the focused checkout, and its commit dialog.
    Git,
    GitCommit,
    Dialog(WorkspaceAction),
}

impl Page {
    /// Pages drawn as a kit popup menu rather than a modal dialog.
    pub(crate) fn is_popup(self) -> bool {
        matches!(
            self,
            Self::Menu | Self::Devices | Self::Tab | Self::Pane | Self::Workspace | Self::Git
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceAction {
    Rename,
    Close,
    NewWorktree,
    OpenWorktree,
    DeleteWorktree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceMenuAction {
    Dialog(WorkspaceAction),
    /// Fold or unfold the worktree group this workspace heads. Applied at once:
    /// it changes only the sidebar's own view, never the daemon's state.
    Collapse,
    Expand,
    PullRequest,
}

impl WorkspaceMenuAction {
    /// Icon for the row, so each action is recognizable before reading. The
    /// pull request section draws its own header rather than a menu row.
    pub(crate) fn icon(self) -> Option<Icon> {
        Some(match self {
            Self::Dialog(WorkspaceAction::Rename) => Icon::default().path("icons/pencil.svg"),
            Self::Dialog(WorkspaceAction::Close) => Icon::new(IconName::Close),
            Self::Dialog(WorkspaceAction::NewWorktree) => Icon::new(IconName::Plus),
            Self::Dialog(WorkspaceAction::OpenWorktree) => Icon::new(IconName::FolderOpen),
            Self::Dialog(WorkspaceAction::DeleteWorktree) => {
                Icon::default().path("icons/trash.svg")
            }
            Self::Collapse => Icon::new(IconName::ChevronUp),
            Self::Expand => Icon::new(IconName::ChevronDown),
            Self::PullRequest => return None,
        })
    }
}
