//! What a menu popup is currently showing, and the workspace actions a row
//! can trigger. Closed sets, so a page is never a string tag.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Page {
    // keep-sorted start
    About,
    /// A cloud provider in the Add Device rail whose machines are set up in
    /// Settings rather than in the dialog.
    #[cfg(feature = "cloud")]
    AddCloud(crate::cloud::CloudProvider),
    /// Sign in to Coder and add one of its workspaces as a device.
    #[cfg(feature = "coder")]
    AddCoder,
    AddDevice,
    /// Picking a WSL distribution to save as a device.
    AddWsl,
    /// The one-time offer to install the agent skill for browser tabs.
    AgentSkill,
    /// The Linux window's application menu, from the native menu definitions.
    Application,
    AppUpdate,
    /// Reviewing, then following, a move of the pane menu's pane to another
    /// device.
    CarryPane,
    /// A checkout's agent checkpoints, and restoring one.
    Checkpoints,
    /// Go to Symbol and Go to File over the focused pane's checkout.
    CodeSearch,
    ConfirmClose,
    Devices,
    /// Picks a saved SSH device's terminal theme.
    DeviceTheme,
    Dialog(WorkspaceAction),
    /// One prompt sent to several agents, and their comparison.
    FanOut,
    /// Names a saved SSH device's port to forward to this computer.
    ForwardPort,
    /// Titlebar Git actions for the focused checkout, and its commit dialog.
    Git,
    GitCommit,
    GitHub,
    /// A group's "…" menu: opening tabs other than a terminal, closing
    /// tabs, and splitting.
    Group,
    /// A saved SSH device's context menu, from its sidebar host header.
    Host,
    Install,
    Keybinds,
    /// Confirming the processes chosen there should end.
    KillProcesses,
    Menu,
    /// Where the pane menu's pane can move: new places, then other tabs.
    MovePane,
    /// Where the tab menu's tab can move: a new workspace, then other workspaces.
    MoveTab,
    /// Asks before opening a workspace for a checkout a link named.
    OpenCheckout,
    /// Asks before opening a terminal hyperlink with an unlisted scheme.
    OpenLink,
    Palette,
    Pane,
    /// The processes under the pane menu's pane.
    PaneProcesses,
    PrComment,
    /// Choosing a merge method, and confirming it.
    PrMerge,
    /// The open pull request's checks and review conversation.
    PrReview,
    /// A remote file's host changed since its copy was synced, so a save
    /// was not uploaded.
    RemoteConflict,
    /// A remote folder is large or changed, and its copy asks what to do.
    RemoteFolder,
    /// Asks before a save to a remote file's copy is uploaded.
    RemoteUpload,
    RemoveDevice,
    /// Confirming a saved WSL distribution should be forgotten.
    RemoveWsl,
    RenameDevice,
    RenamePane,
    RenameTab,
    /// Local sessions and remote devices, with the state of each.
    Sessions,
    Tab,
    /// Moving a linked worktree to another host.
    Teleport,
    Themes,
    Update,
    /// Plan usage details for one agent on the selected host.
    Usage(crate::usage::Provider),
    /// A daemon refused the handshake until one side is updated.
    VersionMismatch,
    Workspace,
    /// Asks whether to trust a repository's worktree script before it runs.
    WorktreeScript,
    // keep-sorted end
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceAction {
    Rename,
    Close,
    NewWorktree,
    OpenWorktree,
    DeleteWorktree,
    /// Names a tab before `tab.create`, when `ui.prompt_new_tab_name` asks.
    NewTab,
    /// Names a workspace before `workspace.create`, when
    /// `ui.prompt_new_workspace_name` asks. Targets the source workspace.
    NewWorkspace,
    /// Writes the note this app keeps on the checkout; never a daemon request.
    Note,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceMenuAction {
    Dialog(WorkspaceAction),
    /// Fold or unfold the worktree group this workspace heads. Applied at once:
    /// it changes only the sidebar's own view, never the daemon's state.
    Collapse,
    Expand,
    PullRequest,
    Teleport,
    /// Teleport the work back to the host it came from.
    TeleportBack,
    /// Focus the copy the work was teleported to.
    GoToTeleported,
    /// Forget that this checkout's work was teleported away.
    ClearTeleported,
    Checkpoints,
    /// Send one prompt to several agents, or reopen their comparison.
    FanOut,
    /// Run one of the repository's worktree scripts in a new tab.
    Script(crate::worktree_scripts::ScriptKind),
    /// Show the repository's issues in the activity bar.
    Orchestrator,
}

impl Page {
    /// Context menus open where the pointer asked for them, rather than
    /// centred over a dimmed window as a dialog is.
    pub(crate) fn pointer_anchored(self) -> bool {
        matches!(
            self,
            Self::Workspace
                | Self::Tab
                | Self::RenameTab
                | Self::MoveTab
                | Self::Group
                | Self::Pane
                | Self::RenamePane
                | Self::MovePane
                | Self::CarryPane
                | Self::PaneProcesses
                | Self::KillProcesses
                | Self::Host
                | Self::RemoveDevice
                | Self::RemoveWsl
                | Self::Git
                | Self::GitCommit
                | Self::PrReview
                | Self::PrComment
                | Self::PrMerge
        )
    }

    /// The pane menu and the pages it opens, all about its one pane.
    pub(crate) fn pane_menu(self) -> bool {
        matches!(
            self,
            Self::Pane
                | Self::RenamePane
                | Self::MovePane
                | Self::CarryPane
                | Self::PaneProcesses
                | Self::KillProcesses
        )
    }
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
            Self::Dialog(WorkspaceAction::Note) => "icons/note.svg",
            Self::Dialog(WorkspaceAction::NewTab | WorkspaceAction::NewWorkspace) => {
                "icons/plus.svg"
            }
            Self::Collapse => "icons/chevron-up.svg",
            Self::Expand => "icons/chevron-down.svg",
            Self::Teleport | Self::GoToTeleported => "icons/teleport.svg",
            Self::TeleportBack => "icons/teleport-back.svg",
            Self::ClearTeleported => "icons/x.svg",
            Self::Checkpoints => "icons/refresh.svg",
            Self::FanOut => "icons/fan-out.svg",
            Self::Orchestrator => "icons/github.svg",
            Self::Script(crate::worktree_scripts::ScriptKind::Setup) => "icons/refresh.svg",
            Self::Script(_) => "icons/play.svg",
            Self::PullRequest => return None,
        })
    }
}
