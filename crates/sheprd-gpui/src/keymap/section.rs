//! The topics bindings are filed under, shared by the shortcut reference and
//! the panel of keys that follow the prefix, so a command reads the same in
//! both.
// i18n: migrated

use crate::controls::Command;
use crate::i18n::fl;

/// A topic, in the order the reference shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Workspaces,
    Navigation,
    Application,
    /// The daemon's custom commands, which no catalog entry names.
    Commands,
}

impl Section {
    /// Every topic, in the order the reference shows them.
    pub(crate) const ALL: [Self; 4] = [
        Self::Workspaces,
        Self::Navigation,
        Self::Application,
        Self::Commands,
    ];

    pub(crate) fn title(self) -> String {
        match self {
            Self::Workspaces => fl!("menu-settings-workspaces"),
            Self::Navigation => fl!("menu-settings-navigation"),
            Self::Application => fl!("menu-settings-application"),
            Self::Commands => fl!("menu-settings-commands"),
        }
    }

    /// The topic of a catalog command. No wildcard: a new command has to
    /// choose one.
    pub(crate) fn of(command: Command) -> Self {
        match command {
            Command::Workspace
            | Command::NewWorktree
            | Command::OpenWorktree
            | Command::RemoveWorktree
            | Command::EditWorktreeNote
            | Command::Tab
            | Command::SplitRight
            | Command::SplitDown
            | Command::Zoom
            | Command::ClearPane
            | Command::Find
            | Command::FindNext
            | Command::FindPrevious
            | Command::CopyMode
            | Command::AnnotateSelection
            | Command::EditScrollback
            | Command::ClosePane
            | Command::CloseTab
            | Command::ReopenClosedTab
            | Command::NewBrowserTab
            | Command::OpenOrchestrator
            | Command::ReviewChanges
            | Command::GitMenu
            | Command::ToggleAnnotate
            | Command::SplitEditor
            | Command::MoveTabPrevious
            | Command::MoveTabNext
            | Command::RenameTab
            | Command::SwapLeft
            | Command::SwapRight
            | Command::SwapUp
            | Command::SwapDown
            | Command::ResizeLeft
            | Command::ResizeRight
            | Command::ResizeUp
            | Command::ResizeDown
            | Command::ResizeMode
            | Command::EqualizeSplits
            | Command::RenamePane
            | Command::MovePane
            | Command::RenameWorkspace
            | Command::CloseWorkspace => Self::Workspaces,
            Command::NextTab
            | Command::PreviousTab
            | Command::FocusLeft
            | Command::FocusRight
            | Command::FocusUp
            | Command::FocusDown
            | Command::NextPane
            | Command::PreviousPane
            | Command::TabNumber(_)
            | Command::WorkspacePicker
            | Command::GoToSymbol
            | Command::GoToFile
            | Command::WorktreeNotes
            | Command::LastPane
            | Command::Back
            | Command::Forward
            | Command::PreviousWorkspace
            | Command::NextWorkspace
            | Command::WorkspaceNumber(_)
            | Command::PreviousAgent
            | Command::NextAgent
            | Command::AgentNumber(_) => Self::Navigation,
            Command::NewWindow
            | Command::ToggleSidebar
            | Command::OpenCode
            | Command::OpenPx0
            | Command::RemotePair
            | Command::ToggleStatusBar
            | Command::ToggleModPanel
            | Command::FocusNextRegion
            | Command::FocusPreviousRegion
            | Command::ToggleZenMode
            | Command::TogglePassThrough
            | Command::DevicesOverview
            | Command::NewCoreTerminal
            | Command::MirrorCorePane
            | Command::ConvertToChat
            | Command::IncreaseFontSize
            | Command::DecreaseFontSize
            | Command::ResetFontSize
            | Command::Settings
            | Command::Keybinds
            | Command::Sessions
            | Command::Themes
            | Command::Palette
            | Command::ResumePalette
            | Command::Detach
            | Command::Reconnect
            | Command::Quit
            | Command::Logs
            | Command::About
            | Command::InstallBrowserSkill
            | Command::ToggleFullScreen
            | Command::CycleWindows
            | Command::CloseWindow
            | Command::ReloadConfig => Self::Application,
            Command::OpenNotificationTarget => Self::Navigation,
        }
    }
}
