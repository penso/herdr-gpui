//! The app's root `Error`: its own variants from `error.ftl`, and every
//! wrapped error through that error's own mapping.

use super::Localize;
use crate::{Error, i18n::fl};
use std::path::Path;

fn path(path: &Path) -> String {
    path.display().to_string()
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::AgentContext { operation, .. } => {
                fl!("error-agent-context", operation = *operation)
            }
            Self::Automations(error) => error.localize(),
            Self::Backdrop(error) => error.localize(),
            Self::Bonjour(error) => fl!("error-bonjour", error = error.to_string()),
            Self::BundleExists { path: file } => fl!("error-bundle-exists", path = path(file)),
            Self::BundleWrite { path: file, source } => fl!(
                "error-bundle-write",
                path = path(file),
                error = source.to_string()
            ),
            Self::Caffeine(error) => error.localize(),
            Self::Chat(error) => error.localize(),
            Self::Cleanup {
                source,
                path: file,
                cleanup,
            } => fl!(
                "error-cleanup",
                error = source.to_string(),
                path = path(file),
                cleanup = cleanup.to_string()
            ),
            Self::Client(error) => error.localize(),
            Self::Clipboard(error) => error.localize(),
            #[cfg(feature = "cloud")]
            Self::CloudBatch(error) => error.localize(),
            #[cfg(feature = "azure")]
            Self::CloudHostUnsupported => fl!("error-cloud-host-unsupported"),
            Self::CloudSetup(error) => error.localize(),
            Self::Code(error) => error.localize(),
            Self::CodeFileUnreadable => fl!("error-code-file-unreadable"),
            // A parser's own diagnostic, which names the line it stopped at.
            Self::ConfigFile { source, .. } => source.to_string(),
            Self::ConfigLoad(error) => error.to_string(),
            Self::ConnectionBusy => fl!("error-connection-busy"),
            Self::ConnectionNotReady => fl!("error-connection-not-ready"),
            Self::Control(error) => error.localize(),
            Self::Credentials(error) => error.localize(),
            Self::Daemon(error) => error.localize(),
            Self::DeletionLookup => fl!("error-deletion-lookup"),
            Self::DetachedHead => fl!("error-detached-head"),
            Self::Devices(error) => error.localize(),
            Self::Dispatch(error) => error.localize(),
            Self::Editor(error) => error.localize(),
            Self::EmptyTabName => fl!("error-empty-tab-name"),
            Self::EmptyWorkspaceLabel => fl!("error-empty-workspace-label"),
            Self::FileDrop(error) => error.localize(),
            Self::GitCommonDir { path: dir, source } => fl!(
                "error-git-common-dir",
                path = path(dir),
                error = source.to_string()
            ),
            Self::GitHub(error) => error.localize(),
            Self::GitHubSecretInConfig(key) => fl!("error-github-secret-in-config", key = *key),
            Self::ImageAnimationResize => fl!("error-image-animation-resize"),
            Self::ImageDecode(_) => fl!("error-image-decode"),
            Self::ImageDecodeLimit => fl!("error-image-decode-limit"),
            Self::ImageEncode(_) => fl!("error-image-encode"),
            Self::ImageFile { operation, .. } => fl!("error-image-file", operation = *operation),
            Self::ImageFileType => fl!("error-image-file-type"),
            Self::ImageFormat => fl!("error-image-format"),
            Self::ImageInputTooLarge { limit } => {
                fl!("error-image-input-too-large", limit = *limit)
            }
            Self::ImageReadTimeout => fl!("error-image-read-timeout"),
            Self::ImageSize => fl!("error-image-size"),
            Self::ImageTooLarge { limit } => fl!("error-image-too-large", limit = *limit),
            Self::InvalidAnnotation => fl!("error-invalid-annotation"),
            Self::InvalidBranchName => fl!("error-invalid-branch-name"),
            Self::InvalidBrowserTabs => fl!("error-invalid-browser-tabs"),
            Self::InvalidBrowserUrl => fl!("error-invalid-browser-url"),
            Self::InvalidCodeFile => fl!("error-invalid-code-file"),
            Self::InvalidExternalUrl => fl!("error-invalid-external-url"),
            Self::InvalidGroupLayouts => fl!("error-invalid-group-layouts"),
            Self::InvalidLocalPage => fl!("error-invalid-local-page"),
            Self::InvalidReviewCheckout => fl!("error-invalid-review-checkout"),
            Self::InvalidSidebarWidth => fl!("error-invalid-sidebar-width"),
            Self::InvalidStoredWidth => fl!("error-invalid-stored-width"),
            Self::InvalidWindowState => fl!("error-invalid-window-state"),
            Self::InvalidWorktreeNotes => fl!("error-invalid-worktree-notes"),
            // The system's own words, as `Json`, `Toml`, and `TomlEdit` are
            // their parser's.
            Self::Io(error) => error.to_string(),
            Self::Json(error) => error.to_string(),
            Self::Keymap(error) => error.localize(),
            #[cfg(windows)]
            Self::LaunchAtLogin(error) => error.localize(),
            Self::LinkedFileMissing { path: file } => {
                fl!("error-linked-file-missing", path = path(file))
            }
            Self::LinkOnAnotherHost { host } => {
                fl!("error-link-on-another-host", host = host.as_str())
            }
            Self::ListeningPorts(error) => error.localize(),
            Self::LocalAddresses(source) => {
                fl!("error-local-addresses", error = source.to_string())
            }
            Self::LocalGit(_) => fl!("error-local-git"),
            Self::LocalNetworkDenied => fl!("error-local-network-denied"),
            Self::LocalWorktreeNote => fl!("error-local-worktree-note"),
            Self::LogRead { path: file, source } => fl!(
                "error-log-read",
                path = path(file),
                error = source.to_string()
            ),
            Self::MalformedLink => fl!("error-malformed-link"),
            Self::MissingDeletion => fl!("error-missing-deletion"),
            Self::MissingHome => fl!("error-missing-home"),
            Self::MissingStateRoot => fl!("error-missing-state-root"),
            Self::Mods(error) => error.localize(),
            Self::NoSnapshot => fl!("error-no-snapshot"),
            Self::NoTab => fl!("error-no-tab"),
            Self::NotAGitRepository { path: dir } => {
                fl!("error-not-a-git-repository", path = path(dir))
            }
            Self::NotConnected => fl!("error-not-connected"),
            Self::NotificationMutes(error) => error.localize(),
            Self::NotInCheckout => fl!("error-not-in-checkout"),
            Self::OrchestratorItems(error) => {
                fl!("error-orchestrator-items", error = error.localize())
            }
            Self::Overview(error) => error.localize(),
            #[cfg(target_os = "macos")]
            Self::PageStack => fl!("error-page-stack"),
            Self::Palette(error) => error.localize(),
            Self::PaneCarry(error) => error.localize(),
            Self::PaneImageDecode(_) => fl!("error-pane-image-decode"),
            Self::PaneImageLimit => fl!("error-pane-image-limit"),
            Self::PaneMove(error) => error.localize(),
            Self::Path { path: file, source } => {
                fl!("error-path", path = path(file), error = source.localize())
            }
            #[cfg(target_os = "macos")]
            Self::PopupOpener => fl!("error-popup-opener"),
            Self::PortForward(error) => error.localize(),
            Self::PreferencesPath => fl!("error-preferences-path"),
            Self::Processes(error) => error.localize(),
            Self::Px0(error) => error.localize(),
            Self::RelativeConfigRoot => fl!("error-relative-config-root"),
            Self::Remote(error) => error.localize(),
            Self::RemoteAgentTools(source) => {
                fl!("error-remote-agent-tools", error = source.localize())
            }
            Self::RemoteControl(error) => error.localize(),
            Self::Request { method, source } => fl!(
                "error-request",
                method = method.to_string(),
                error = source.localize()
            ),
            Self::Review(error) => error.localize(),
            Self::ScreenshotImage => fl!("error-screenshot-image"),
            Self::ScreenshotWrite { path: file, source } => fl!(
                "error-screenshot-write",
                path = path(file),
                error = source.to_string()
            ),
            Self::SelectionOffscreen => fl!("error-selection-offscreen"),
            Self::SelectionSize => fl!("error-selection-size"),
            Self::SelectionStale => fl!("error-selection-stale"),
            Self::ServerKeybindings(error) => error.localize(),
            Self::Settings(error) => error.localize(),
            Self::SettingsSave(source) => fl!("error-settings-save", error = source.localize()),
            #[cfg(unix)]
            Self::SetupRelay(error) => error.localize(),
            Self::SkillInstall { path: dir, source } => fl!(
                "error-skill-install",
                path = path(dir),
                error = source.to_string()
            ),
            Self::Sound(error) => error.localize(),
            Self::SshConfig { path: file, source } => fl!(
                "error-ssh-config",
                path = path(file),
                error = source.to_string()
            ),
            Self::SshHosts(error) => error.localize(),
            Self::StaleCloseTarget => fl!("error-stale-close-target"),
            Self::StaleConnection => fl!("error-stale-connection"),
            Self::StalePane => fl!("error-stale-pane"),
            Self::StaleTab => fl!("error-stale-tab"),
            Self::StaleWorkspace => fl!("error-stale-workspace"),
            Self::StateFileSize { path: file, limit } => {
                fl!("error-state-file-size", path = path(file), limit = *limit)
            }
            Self::SystemLoad(error) => error.localize(),
            Self::TabLayout(error) => error.localize(),
            Self::Toml(error) => error.to_string(),
            Self::TomlEdit(error) => error.to_string(),
            Self::Tunnel(error) => error.localize(),
            Self::UnsupportedCommand => fl!("error-unsupported-command"),
            Self::Update(error) => error.localize(),
            Self::Usage(error) => error.localize(),
            Self::Vcs(error) => error.localize(),
            #[cfg(any(target_os = "macos", windows))]
            Self::WebView(error) => fl!("error-web-view", error = error.to_string()),
            #[cfg(any(target_os = "macos", windows))]
            Self::WindowHandle(error) => fl!("error-window-handle", error = error.to_string()),
            Self::WorkspaceCheckoutChanged => fl!("error-workspace-checkout-changed"),
            Self::WorkspaceGroupChanged => fl!("error-workspace-group-changed"),
            Self::WorkspaceRepositoryChanged => fl!("error-workspace-repository-changed"),
            Self::WorktreeIdentity(error) => error.localize(),
            Self::WorktreeList => fl!("error-worktree-list"),
            Self::WorktreeListDecode(_) => fl!("error-worktree-list-decode"),
            Self::WorktreeScripts(error) => error.localize(),
            Self::WorktreeSelection => fl!("error-worktree-selection"),
            Self::WslDrop(error) => error.localize(),
            // keep-sorted end
        }
    }
}
