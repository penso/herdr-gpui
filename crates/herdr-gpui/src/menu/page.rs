//! What a menu popup is currently showing, and the workspace actions a row
//! can trigger. Closed sets, so a page is never a string tag.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Page {
    Menu,
    About,
    Preferences,
    Devices,
    /// Local sessions and remote devices, with the state of each.
    Sessions,
    /// Plan usage details for one agent on the selected host.
    Usage(crate::usage::Provider),
    AddDevice,
    /// A saved SSH device's context menu, from its sidebar host header.
    Host,
    RenameDevice,
    RemoveDevice,
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
    /// Embedded icon for the row, so each action is recognizable before reading.
    /// The pull request section draws its own header rather than a menu row.
    pub(crate) fn icon(self) -> Option<&'static str> {
        Some(match self {
            Self::Dialog(WorkspaceAction::Rename) => "icons/pencil.svg",
            Self::Dialog(WorkspaceAction::Close) => "icons/close.svg",
            Self::Dialog(WorkspaceAction::NewWorktree) => "icons/plus.svg",
            Self::Dialog(WorkspaceAction::OpenWorktree) => "icons/chevron-down.svg",
            Self::Dialog(WorkspaceAction::DeleteWorktree) => "icons/trash.svg",
            Self::Collapse => "icons/chevron-up.svg",
            Self::Expand => "icons/chevron-down.svg",
            Self::PullRequest => return None,
        })
    }
}
