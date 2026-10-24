//! What a screen reader calls each popup page. Keyboard focus stays on the
//! popup's overlay while it is open, so the overlay carries the page's role
//! and title, and the page's own rows and fields sit under it.

use super::{Page, WorkspaceAction};
use gpui::{Role, SharedString};

impl Page {
    /// A list of commands is a menu; anything that asks or edits is a dialog.
    pub(crate) fn accessible_role(self) -> Role {
        match self {
            Self::Application
            | Self::Git
            | Self::Group
            | Self::Host
            | Self::Menu
            | Self::Pane
            | Self::Tab
            | Self::Workspace => Role::Menu,
            _ => Role::Dialog,
        }
    }

    pub(crate) fn accessible_name(self) -> SharedString {
        SharedString::from(match self {
            Self::About => "About Herdr UI",
            #[cfg(feature = "cloud")]
            Self::AddCloud(_) => "Add device",
            #[cfg(feature = "coder")]
            Self::AddCoder => "Add Coder workspace",
            Self::AddDevice => "Add device",
            #[cfg(feature = "cloud")]
            Self::AddProvider => "Add cloud provider",
            Self::AddWsl => "Add WSL distribution",
            Self::AgentSkill => "Install the agent skill",
            Self::Application => "Menu",
            Self::AppUpdate | Self::Update => "Update",
            Self::Automation => "Automations",
            Self::CarryPane => "Move pane to another device",
            Self::Checkpoints => "Checkpoints",
            Self::CodeSearch => "Go to symbol or file",
            Self::ConfirmClose => "Close",
            #[cfg(feature = "cloud")]
            Self::CreateCloud => "Create cloud machines",
            #[cfg(feature = "cloud")]
            Self::DestroyCloud => "Destroy cloud devices",
            Self::Devices => "Devices",
            Self::DeviceTheme => "Device theme",
            Self::Dialog(action) => match action {
                WorkspaceAction::Rename => "Rename workspace",
                WorkspaceAction::Close => "Close workspace",
                WorkspaceAction::NewWorktree => "New worktree",
                WorkspaceAction::OpenWorktree => "Open worktree",
                WorkspaceAction::DeleteWorktree => "Delete worktree",
                WorkspaceAction::NewTab => "New tab",
                WorkspaceAction::NewWorkspace => "New workspace",
                WorkspaceAction::Note => "Worktree note",
            },
            Self::FanOut => "Fan out",
            Self::ForwardPort => "Forward port",
            Self::Git => "Git",
            Self::GitCommit => "Commit",
            Self::GitHub => "GitHub account",
            Self::Group => "Tab actions",
            Self::Host => "Device",
            Self::Install => "Install",
            Self::Keybinds => "Keyboard shortcuts",
            Self::KillProcesses => "End processes",
            Self::Menu => "Menu",
            Self::MovePane => "Move pane",
            Self::MoveTab => "Move tab",
            Self::OpenCheckout => "Open checkout",
            Self::OpenLink => "Open link",
            Self::Palette => "Command palette",
            Self::Pane => "Pane",
            Self::PaneProcesses => "Processes",
            Self::PrComment => "Comment on pull request",
            Self::PrMerge => "Merge pull request",
            Self::PrReview => "Pull request",
            Self::RemoteControl => "Remote control",
            Self::RemoveDevice => "Remove device",
            Self::RemoveWsl => "Remove WSL distribution",
            Self::RenameDevice => "Rename device",
            Self::RenamePane => "Rename pane",
            Self::RenameTab => "Rename tab",
            Self::RevokeDevice => "Revoke device",
            Self::Sessions => "Sessions",
            Self::Tab => "Tab",
            Self::Teleport => "Teleport",
            Self::Themes => "Themes",
            Self::Usage(provider) => return format!("{} usage", provider.name()).into(),
            Self::VersionMismatch => "Version mismatch",
            Self::Workspace => "Workspace",
            Self::WorktreeScript => "Trust worktree script",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menus_are_menus_and_questions_are_dialogs() {
        assert_eq!(Page::Workspace.accessible_role(), Role::Menu);
        assert_eq!(Page::Host.accessible_role(), Role::Menu);
        assert_eq!(
            Page::Dialog(WorkspaceAction::DeleteWorktree).accessible_role(),
            Role::Dialog
        );
        assert_eq!(
            Page::Dialog(WorkspaceAction::DeleteWorktree).accessible_name(),
            "Delete worktree"
        );
        assert_eq!(Page::Keybinds.accessible_role(), Role::Dialog);
    }
}
