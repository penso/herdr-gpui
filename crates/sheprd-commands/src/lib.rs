//! The command catalog: every command the app offers in menus, the palette,
//! and key bindings, its name and default shortcuts per platform, and the
//! daemon request a command becomes against a snapshot. No GPUI; the app
//! binds the shortcuts and runs the commands that stay in the window.

mod request;
mod shortcuts;
pub use request::request;
pub use shortcuts::CTRL_SHORTCUTS;
#[cfg(any(test, feature = "test-support"))]
pub use shortcuts::platform_default;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum Command {
    NewWindow,
    Workspace,
    NewWorktree,
    /// The focused repository's "Open worktree..." dialog.
    OpenWorktree,
    /// The focused linked worktree's delete confirmation.
    RemoveWorktree,
    /// Go To, listing only the checkouts that have a note.
    WorktreeNotes,
    /// The focused checkout's note.
    EditWorktreeNote,
    Tab,
    SplitRight,
    SplitDown,
    NextTab,
    PreviousTab,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    NextPane,
    PreviousPane,
    Zoom,
    ClearPane,
    Find,
    FindNext,
    FindPrevious,
    CopyMode,
    AnnotateSelection,
    EditScrollback,
    ClosePane,
    CloseTab,
    /// Reopens the focused workspace's most recently closed tab.
    ReopenClosedTab,
    TabNumber(u8),
    ToggleSidebar,
    ToggleStatusBar,
    /// Shows or hides the panel of mod sections on the window's right.
    ToggleModPanel,
    /// Moves keyboard focus to the next of the terminal, the title bar, the
    /// sidebar, and the status bar, where Tab moves between controls.
    FocusNextRegion,
    FocusPreviousRegion,
    /// Hides the window's chrome and every group but the one in use.
    ToggleZenMode,
    /// Sends every key the app would claim to the focused pane, until the
    /// command's own shortcut ends it.
    TogglePassThrough,
    DevicesOverview,
    /// Starts a shell in herdr-core (Herdr 2) and shows it in a tab.
    NewCoreTerminal,
    /// Opens a watch-only view of the focused herdr-core pane in a new tab.
    MirrorCorePane,
    /// Moves the focused terminal agent into a chat, in the same pane and
    /// conversation.
    ConvertToChat,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    Settings,
    Keybinds,
    Sessions,
    Themes,
    WorkspacePicker,
    Palette,
    /// Reopen the palette as it was last left: query, filters, and selection.
    ResumePalette,
    /// Go to a definition in the focused pane's checkout.
    GoToSymbol,
    /// Go to a file in the focused pane's checkout.
    GoToFile,
    Detach,
    Reconnect,
    Quit,
    Logs,
    About,
    OpenNotificationTarget,
    NewBrowserTab,
    /// The focused workspace's repository's issues, pull requests, and runs.
    OpenOrchestrator,
    /// A review of the focused checkout's changes.
    ReviewChanges,
    /// The titlebar's Git menu for the focused checkout.
    GitMenu,
    /// Annotate mode on the page in view.
    ToggleAnnotate,
    InstallBrowserSkill,
    SplitEditor,
    OpenCode,
    OpenPx0,
    /// The remote control pairing dialog.
    RemotePair,
    ToggleFullScreen,
    CycleWindows,
    /// Closes the window it runs in, leaving the daemon and its terminals.
    CloseWindow,
    MoveTabPrevious,
    MoveTabNext,
    RenameTab,
    LastPane,
    /// Back through the panes this endpoint has focused.
    Back,
    Forward,
    SwapLeft,
    SwapRight,
    SwapUp,
    SwapDown,
    ResizeLeft,
    ResizeRight,
    ResizeUp,
    ResizeDown,
    ResizeMode,
    /// Evens every split in the focused tab, as a double-click on a border does.
    EqualizeSplits,
    RenamePane,
    MovePane,
    PreviousWorkspace,
    NextWorkspace,
    WorkspaceNumber(u8),
    RenameWorkspace,
    CloseWorkspace,
    PreviousAgent,
    NextAgent,
    AgentNumber(u8),
    ReloadConfig,
}

pub struct CommandInfo {
    pub command: Command,
    /// The key naming this command in the config file's `[keybindings]`.
    pub name: &'static str,
    /// The Fluent message naming the command for people, in the app's
    /// `locales/*/commands.ftl`. A numbered command passes its
    /// [`Command::number`] as `$number`.
    pub message: &'static str,
}

impl Command {
    /// The workspace, tab, or agent a numbered command focuses.
    pub fn number(self) -> Option<u8> {
        match self {
            Self::TabNumber(number) | Self::WorkspaceNumber(number) | Self::AgentNumber(number) => {
                Some(number)
            }
            _ => None,
        }
    }
}

pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        command: Command::OpenNotificationTarget,
        name: "open_notification_target",
        message: "commands-open-notification-target",
    },
    CommandInfo {
        command: Command::Logs,
        name: "logs",
        message: "commands-logs",
    },
    CommandInfo {
        command: Command::NewWindow,
        name: "new_window",
        message: "commands-new-window",
    },
    CommandInfo {
        command: Command::Workspace,
        name: "new_workspace",
        message: "commands-new-workspace",
    },
    CommandInfo {
        command: Command::NewWorktree,
        name: "new_worktree",
        message: "commands-new-worktree",
    },
    CommandInfo {
        command: Command::OpenWorktree,
        name: "open_worktree",
        message: "commands-open-worktree",
    },
    CommandInfo {
        command: Command::RemoveWorktree,
        name: "remove_worktree",
        message: "commands-remove-worktree",
    },
    CommandInfo {
        command: Command::WorktreeNotes,
        name: "worktree_notes",
        message: "commands-worktree-notes",
    },
    CommandInfo {
        command: Command::EditWorktreeNote,
        name: "edit_worktree_note",
        message: "commands-edit-worktree-note",
    },
    CommandInfo {
        command: Command::PreviousWorkspace,
        name: "previous_workspace",
        message: "commands-previous-workspace",
    },
    CommandInfo {
        command: Command::NextWorkspace,
        name: "next_workspace",
        message: "commands-next-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(1),
        name: "focus_workspace_1",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(2),
        name: "focus_workspace_2",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(3),
        name: "focus_workspace_3",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(4),
        name: "focus_workspace_4",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(5),
        name: "focus_workspace_5",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(6),
        name: "focus_workspace_6",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(7),
        name: "focus_workspace_7",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(8),
        name: "focus_workspace_8",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::WorkspaceNumber(9),
        name: "focus_workspace_9",
        message: "commands-focus-workspace",
    },
    CommandInfo {
        command: Command::RenameWorkspace,
        name: "rename_workspace",
        message: "commands-rename-workspace",
    },
    CommandInfo {
        command: Command::CloseWorkspace,
        name: "close_workspace",
        message: "commands-close-workspace",
    },
    CommandInfo {
        command: Command::Tab,
        name: "new_tab",
        message: "commands-new-tab",
    },
    CommandInfo {
        command: Command::SplitRight,
        name: "split_right",
        message: "commands-split-right",
    },
    CommandInfo {
        command: Command::SplitDown,
        name: "split_down",
        message: "commands-split-down",
    },
    CommandInfo {
        command: Command::NextTab,
        name: "next_tab",
        message: "commands-next-tab",
    },
    CommandInfo {
        command: Command::PreviousTab,
        name: "previous_tab",
        message: "commands-previous-tab",
    },
    CommandInfo {
        command: Command::MoveTabPrevious,
        name: "move_tab_previous",
        message: "commands-move-tab-previous",
    },
    CommandInfo {
        command: Command::MoveTabNext,
        name: "move_tab_next",
        message: "commands-move-tab-next",
    },
    CommandInfo {
        command: Command::RenameTab,
        name: "rename_tab",
        message: "commands-rename-tab",
    },
    CommandInfo {
        command: Command::FocusLeft,
        name: "focus_left",
        message: "commands-focus-left",
    },
    CommandInfo {
        command: Command::FocusRight,
        name: "focus_right",
        message: "commands-focus-right",
    },
    CommandInfo {
        command: Command::FocusUp,
        name: "focus_up",
        message: "commands-focus-up",
    },
    CommandInfo {
        command: Command::FocusDown,
        name: "focus_down",
        message: "commands-focus-down",
    },
    CommandInfo {
        command: Command::NextPane,
        name: "next_pane",
        message: "commands-next-pane",
    },
    CommandInfo {
        command: Command::PreviousPane,
        name: "previous_pane",
        message: "commands-previous-pane",
    },
    CommandInfo {
        command: Command::LastPane,
        name: "last_pane",
        message: "commands-last-pane",
    },
    CommandInfo {
        command: Command::Back,
        name: "back",
        message: "commands-back",
    },
    CommandInfo {
        command: Command::Forward,
        name: "forward",
        message: "commands-forward",
    },
    CommandInfo {
        command: Command::SwapLeft,
        name: "swap_pane_left",
        message: "commands-swap-pane-left",
    },
    CommandInfo {
        command: Command::SwapRight,
        name: "swap_pane_right",
        message: "commands-swap-pane-right",
    },
    CommandInfo {
        command: Command::SwapUp,
        name: "swap_pane_up",
        message: "commands-swap-pane-up",
    },
    CommandInfo {
        command: Command::SwapDown,
        name: "swap_pane_down",
        message: "commands-swap-pane-down",
    },
    CommandInfo {
        command: Command::ResizeLeft,
        name: "resize_pane_left",
        message: "commands-resize-pane-left",
    },
    CommandInfo {
        command: Command::ResizeRight,
        name: "resize_pane_right",
        message: "commands-resize-pane-right",
    },
    CommandInfo {
        command: Command::ResizeUp,
        name: "resize_pane_up",
        message: "commands-resize-pane-up",
    },
    CommandInfo {
        command: Command::ResizeDown,
        name: "resize_pane_down",
        message: "commands-resize-pane-down",
    },
    CommandInfo {
        command: Command::ResizeMode,
        name: "resize_mode",
        message: "commands-resize-mode",
    },
    CommandInfo {
        command: Command::EqualizeSplits,
        name: "equalize_splits",
        message: "commands-equalize-splits",
    },
    CommandInfo {
        command: Command::RenamePane,
        name: "rename_pane",
        message: "commands-rename-pane",
    },
    CommandInfo {
        command: Command::MovePane,
        name: "move_pane",
        message: "commands-move-pane",
    },
    CommandInfo {
        command: Command::Zoom,
        name: "toggle_zoom",
        message: "commands-toggle-zoom",
    },
    CommandInfo {
        command: Command::ClearPane,
        name: "clear_pane",
        message: "commands-clear-pane",
    },
    CommandInfo {
        command: Command::Find,
        name: "find",
        message: "commands-find",
    },
    CommandInfo {
        command: Command::FindNext,
        name: "find_next",
        message: "commands-find-next",
    },
    CommandInfo {
        command: Command::FindPrevious,
        name: "find_previous",
        message: "commands-find-previous",
    },
    CommandInfo {
        command: Command::CopyMode,
        name: "copy_mode",
        message: "commands-copy-mode",
    },
    CommandInfo {
        command: Command::AnnotateSelection,
        name: "annotate_selection",
        message: "commands-annotate-selection",
    },
    CommandInfo {
        command: Command::EditScrollback,
        name: "edit_scrollback",
        message: "commands-edit-scrollback",
    },
    CommandInfo {
        command: Command::ClosePane,
        name: "close_pane",
        message: "commands-close-pane",
    },
    CommandInfo {
        command: Command::CloseTab,
        name: "close_tab",
        message: "commands-close-tab",
    },
    CommandInfo {
        command: Command::ReopenClosedTab,
        name: "reopen_closed_tab",
        message: "commands-reopen-closed-tab",
    },
    CommandInfo {
        command: Command::TabNumber(1),
        name: "focus_tab_1",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(2),
        name: "focus_tab_2",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(3),
        name: "focus_tab_3",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(4),
        name: "focus_tab_4",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(5),
        name: "focus_tab_5",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(6),
        name: "focus_tab_6",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(7),
        name: "focus_tab_7",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(8),
        name: "focus_tab_8",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::TabNumber(9),
        name: "focus_tab_9",
        message: "commands-focus-tab",
    },
    CommandInfo {
        command: Command::PreviousAgent,
        name: "previous_agent",
        message: "commands-previous-agent",
    },
    CommandInfo {
        command: Command::NextAgent,
        name: "next_agent",
        message: "commands-next-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(1),
        name: "focus_agent_1",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(2),
        name: "focus_agent_2",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(3),
        name: "focus_agent_3",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(4),
        name: "focus_agent_4",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(5),
        name: "focus_agent_5",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(6),
        name: "focus_agent_6",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(7),
        name: "focus_agent_7",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(8),
        name: "focus_agent_8",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::AgentNumber(9),
        name: "focus_agent_9",
        message: "commands-focus-agent",
    },
    CommandInfo {
        command: Command::ToggleSidebar,
        name: "toggle_sidebar",
        message: "commands-toggle-sidebar",
    },
    CommandInfo {
        command: Command::ToggleStatusBar,
        name: "toggle_status_bar",
        message: "commands-toggle-status-bar",
    },
    CommandInfo {
        command: Command::ToggleModPanel,
        name: "toggle_mod_panel",
        message: "commands-toggle-mod-panel",
    },
    CommandInfo {
        command: Command::FocusNextRegion,
        name: "focus_next_region",
        message: "commands-focus-next-region",
    },
    CommandInfo {
        command: Command::FocusPreviousRegion,
        name: "focus_previous_region",
        message: "commands-focus-previous-region",
    },
    CommandInfo {
        command: Command::ToggleZenMode,
        name: "toggle_zen_mode",
        message: "commands-toggle-zen-mode",
    },
    CommandInfo {
        command: Command::TogglePassThrough,
        name: "toggle_pass_through",
        message: "commands-toggle-pass-through",
    },
    CommandInfo {
        command: Command::DevicesOverview,
        name: "devices_overview",
        message: "commands-devices-overview",
    },
    CommandInfo {
        command: Command::NewCoreTerminal,
        name: "new_core_terminal",
        message: "commands-new-core-terminal",
    },
    CommandInfo {
        command: Command::MirrorCorePane,
        name: "mirror_core_pane",
        message: "commands-mirror-core-pane",
    },
    CommandInfo {
        command: Command::ConvertToChat,
        name: "convert_to_chat",
        message: "commands-convert-to-chat",
    },
    CommandInfo {
        command: Command::IncreaseFontSize,
        name: "increase_font_size",
        message: "commands-increase-font-size",
    },
    CommandInfo {
        command: Command::DecreaseFontSize,
        name: "decrease_font_size",
        message: "commands-decrease-font-size",
    },
    CommandInfo {
        command: Command::ResetFontSize,
        name: "reset_font_size",
        message: "commands-reset-font-size",
    },
    CommandInfo {
        command: Command::Settings,
        name: "settings",
        message: "commands-settings",
    },
    CommandInfo {
        command: Command::ReloadConfig,
        name: "reload_config",
        message: "commands-reload-config",
    },
    CommandInfo {
        command: Command::Keybinds,
        name: "keybindings",
        message: "commands-keybindings",
    },
    CommandInfo {
        command: Command::Sessions,
        name: "sessions",
        message: "commands-sessions",
    },
    CommandInfo {
        command: Command::Themes,
        name: "themes",
        message: "commands-themes",
    },
    CommandInfo {
        command: Command::WorkspacePicker,
        name: "workspace_picker",
        message: "commands-workspace-picker",
    },
    CommandInfo {
        command: Command::Palette,
        name: "command_palette",
        message: "commands-command-palette",
    },
    CommandInfo {
        command: Command::ResumePalette,
        name: "resume_palette",
        message: "commands-resume-palette",
    },
    CommandInfo {
        command: Command::GoToSymbol,
        name: "go_to_symbol",
        message: "commands-go-to-symbol",
    },
    CommandInfo {
        command: Command::GoToFile,
        name: "go_to_file",
        message: "commands-go-to-file",
    },
    CommandInfo {
        command: Command::Detach,
        name: "detach",
        message: "commands-detach",
    },
    CommandInfo {
        command: Command::Reconnect,
        name: "reconnect",
        message: "commands-reconnect",
    },
    CommandInfo {
        command: Command::Quit,
        name: "quit",
        message: "commands-quit",
    },
    CommandInfo {
        command: Command::About,
        name: "about",
        message: "commands-about",
    },
    CommandInfo {
        command: Command::NewBrowserTab,
        name: "new_browser_tab",
        message: "commands-new-browser-tab",
    },
    CommandInfo {
        command: Command::OpenOrchestrator,
        name: "open_orchestrator",
        message: "commands-open-orchestrator",
    },
    CommandInfo {
        command: Command::ReviewChanges,
        name: "review_changes",
        message: "commands-review-changes",
    },
    CommandInfo {
        command: Command::GitMenu,
        name: "git_menu",
        message: "commands-git-menu",
    },
    CommandInfo {
        command: Command::ToggleAnnotate,
        name: "toggle_annotate",
        message: "commands-toggle-annotate",
    },
    CommandInfo {
        command: Command::InstallBrowserSkill,
        name: "install_browser_skill",
        message: "commands-install-browser-skill",
    },
    CommandInfo {
        command: Command::SplitEditor,
        name: "split_editor",
        message: "commands-split-editor",
    },
    CommandInfo {
        command: Command::OpenCode,
        name: "open_code",
        message: "commands-open-code",
    },
    CommandInfo {
        command: Command::OpenPx0,
        name: "open_px0",
        message: "commands-open-px0",
    },
    CommandInfo {
        command: Command::RemotePair,
        name: "remote_pair",
        message: "commands-remote-pair",
    },
    CommandInfo {
        command: Command::ToggleFullScreen,
        name: "toggle_full_screen",
        message: "commands-toggle-full-screen",
    },
    CommandInfo {
        command: Command::CycleWindows,
        name: "cycle_windows",
        message: "commands-cycle-windows",
    },
    CommandInfo {
        command: Command::CloseWindow,
        name: "close_window",
        message: "commands-close-window",
    },
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
