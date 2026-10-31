//! What a screen reader calls each popup page. Keyboard focus stays on the
//! popup's overlay while it is open, so the overlay carries the page's role
//! and title, and the page's own rows and fields sit under it.

// i18n: migrated
use super::{Page, WorkspaceAction};
use crate::i18n::fl;
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
            Self::About => fl!("menu-page-about-herdr-ui"),
            #[cfg(feature = "cloud")]
            Self::AddCloud(_) => fl!("menu-page-add-device"),
            #[cfg(feature = "coder")]
            Self::AddCoder => fl!("menu-page-add-coder-workspace"),
            Self::AddDevice => fl!("menu-page-add-device"),
            #[cfg(feature = "cloud")]
            Self::AddProvider => fl!("menu-page-add-cloud-provider"),
            Self::AddWsl => fl!("menu-page-add-wsl-distribution"),
            Self::AgentSkill => fl!("menu-page-install-the-agent-skill"),
            Self::Application => fl!("menu-page-menu"),
            Self::AppUpdate | Self::Update => fl!("menu-page-update"),
            Self::Automation => fl!("menu-page-automations"),
            Self::CarryPane => fl!("menu-page-move-pane-to-another-device"),
            Self::Checkpoints => fl!("menu-page-checkpoints"),
            Self::CodeSearch => fl!("menu-page-go-to-symbol-or-file"),
            Self::ConfirmClose => fl!("menu-page-close"),
            #[cfg(feature = "cloud")]
            Self::CreateCloud => fl!("menu-page-create-cloud-machines"),
            #[cfg(feature = "cloud")]
            Self::DestroyCloud => fl!("menu-page-destroy-cloud-devices"),
            Self::Devices => fl!("menu-page-devices"),
            Self::DeviceTheme => fl!("menu-page-device-theme"),
            Self::Dialog(action) => match action {
                WorkspaceAction::Rename => fl!("menu-page-rename-workspace"),
                WorkspaceAction::Close => fl!("menu-page-close-workspace"),
                WorkspaceAction::NewWorktree => fl!("menu-page-new-worktree"),
                WorkspaceAction::OpenWorktree => fl!("menu-page-open-worktree"),
                WorkspaceAction::DeleteWorktree => fl!("menu-page-delete-worktree"),
                WorkspaceAction::NewTab => fl!("menu-page-new-tab"),
                WorkspaceAction::NewWorkspace => fl!("menu-page-new-workspace"),
                WorkspaceAction::Note => fl!("menu-page-worktree-note"),
            },
            Self::FanOut => fl!("menu-page-fan-out"),
            Self::ForwardPort => fl!("menu-page-forward-port"),
            Self::Git => fl!("menu-page-git"),
            Self::GitCommit => fl!("menu-page-commit"),
            Self::GitHub => fl!("menu-page-github-account"),
            Self::GitLab => fl!("menu-page-gitlab-tokens"),
            Self::Group => fl!("menu-page-tab-actions"),
            Self::Host => fl!("menu-page-device"),
            Self::Install => fl!("menu-page-install"),
            Self::Keybinds => fl!("menu-page-keyboard-shortcuts"),
            Self::KillProcesses => fl!("menu-page-end-processes"),
            Self::Menu => fl!("menu-page-menu"),
            Self::MovePane => fl!("menu-page-move-pane"),
            Self::MoveTab => fl!("menu-page-move-tab"),
            Self::OpenCheckout => fl!("menu-page-open-checkout"),
            Self::OpenLink => fl!("menu-page-open-link"),
            Self::Palette => fl!("menu-page-command-palette"),
            Self::Pane => fl!("menu-page-pane"),
            Self::PaneProcesses => fl!("menu-page-processes"),
            Self::PrComment => fl!("menu-page-comment-on-pull-request"),
            Self::PrMerge => fl!("menu-page-merge-pull-request"),
            Self::PrReview => fl!("menu-page-pull-request"),
            Self::RemoteControl => fl!("menu-page-remote-control"),
            Self::RemoteDevices => fl!("menu-page-remote-control-phones"),
            Self::RemoveDevice => fl!("menu-page-remove-device"),
            Self::RemoveWsl => fl!("menu-page-remove-wsl-distribution"),
            Self::RenameDevice => fl!("menu-page-rename-device"),
            Self::RenamePane => fl!("menu-page-rename-pane"),
            Self::RenameTab => fl!("menu-page-rename-tab"),
            Self::RevokeDevice => fl!("menu-page-revoke-device"),
            Self::Sessions => fl!("menu-page-sessions"),
            Self::Tab => fl!("menu-page-tab"),
            Self::Teleport => fl!("menu-page-teleport"),
            Self::Themes => fl!("menu-page-themes"),
            Self::Usage(provider) => fl!("menu-page-usage", provider = provider.name()),
            Self::VersionMismatch => fl!("menu-page-version-mismatch"),
            Self::Workspace => fl!("menu-page-workspace"),
            Self::WorktreeScript => fl!("menu-page-trust-worktree-script"),
        })
    }
}

#[cfg(test)]
mod tests;
