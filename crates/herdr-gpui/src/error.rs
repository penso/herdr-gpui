//! Internal failures retain their categories and sources until presentation.
pub use crate::updater::UpdateError;
pub use clipboard::ClipboardError;
pub use github::{GitHubAccountsError, GitHubJsonError};
use std::{io, path::PathBuf};

#[cfg(any(target_os = "macos", windows))]
use wry::raw_window_handle::HandleError as WindowHandleError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not {operation} the agent context note for the new checkout.")]
    AgentContext {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Automations(#[from] crate::automations::Error),
    #[error(transparent)]
    Backdrop(#[from] crate::backdrop::Error),
    #[error("Searching the local network failed: {0}")]
    Bonjour(#[from] mdns_sd::Error),
    #[error("{} already exists; choose a new file name", path.display())]
    BundleExists { path: PathBuf },
    #[error("Could not write {}: {source}", path.display())]
    BundleWrite {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Caffeine(#[from] crate::caffeine::Error),
    #[error(transparent)]
    Chat(#[from] crate::chat::Error),
    #[error("{source}; removing {}: {cleanup}", path.display())]
    Cleanup {
        #[source]
        source: io::Error,
        path: PathBuf,
        cleanup: io::Error,
    },
    #[error("{0}")]
    Client(#[from] herdr_client::Error),
    #[error("HERDR_GITHUB_OAUTH_CLIENT_ID must be UTF-8")]
    ClientIdEncoding,
    #[error(transparent)]
    Clipboard(#[from] ClipboardError),
    #[cfg(feature = "cloud")]
    #[error(transparent)]
    CloudBatch(#[from] crate::cloud::batch::Error),
    /// Only CPU and memory are sampled on an Azure machine so far.
    #[cfg(feature = "azure")]
    #[error("This is not available for cloud machines yet.")]
    CloudHostUnsupported,
    #[error(transparent)]
    CloudSetup(#[from] crate::cloud_setup::Error),
    #[error(transparent)]
    Code(#[from] crate::code_server::Error),
    #[error("This file is missing, larger than 1 MiB, or not UTF-8 text")]
    CodeFileUnreadable,
    #[error("{source}")]
    ConfigFile {
        uri: Option<String>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{0}")]
    ConfigLoad(#[source] config_loader::ConfigError),
    #[error("Connection is busy. Try again.")]
    ConnectionBusy,
    #[error("The connection is not ready. Try again.")]
    ConnectionNotReady,
    #[error(transparent)]
    Control(#[from] crate::control::ControlError),
    #[error(transparent)]
    Credentials(#[from] herdr_credentials::Error),
    #[error(transparent)]
    Daemon(#[from] crate::daemon::Error),
    #[error("Checkout lookup failed. Dismiss and reopen the menu.")]
    DeletionLookup,
    #[error("HEAD is detached. Check a branch out first.")]
    DetachedHead,
    #[error(
        "This host is already being added, in another Add Device dialog or a \u{201c}Set up\u{201d} workspace that is still running. Finish or close it, then try again."
    )]
    DeviceAdding,
    #[error("This host and session are already saved as \u{201c}{0}\u{201d}.")]
    DeviceExists(String),
    #[error("Removing the device failed ({status}){}", if detail.is_empty() { String::new() } else { format!(": {detail}") })]
    DeviceRemove {
        status: std::process::ExitStatus,
        detail: String,
    },
    #[error("Renaming the device failed ({status}){}", if detail.is_empty() { String::new() } else { format!(": {detail}") })]
    DeviceRename {
        status: std::process::ExitStatus,
        detail: String,
    },
    /// Herdr's `machine add` refused to save the host until its running
    /// server is restarted, which it only does after a confirmation.
    #[error(
        "The Herdr server on this host was started in a way that may not survive a dropped SSH connection, so Herdr has to restart it before saving the device. Restarting stops the panes running there."
    )]
    DeviceServerRestart,
    #[error("Saving the device failed ({status}){}", if detail.is_empty() { String::new() } else { format!(": {detail}") })]
    DeviceSetup {
        status: std::process::ExitStatus,
        detail: String,
    },
    #[error("{0}")]
    DeviceSetupInput(&'static str),
    #[error("Saving the device timed out")]
    DeviceSetupTimeout,
    #[error("Another host is still being set up. Wait for it to finish.")]
    DispatchBusy,
    #[error("{0} cannot be reached by script. Choose another host.")]
    DispatchHostUnavailable(String),
    #[error("keybindings: {keystroke:?} is bound to both {first} and {second}")]
    DuplicateKeystroke {
        keystroke: String,
        first: &'static str,
        second: &'static str,
    },
    #[error("pane_keys: {0:?} is listed twice")]
    DuplicatePaneKey(String),
    #[error(transparent)]
    Editor(#[from] crate::editor::Error),
    #[error("pane_keys.{0:?}: text: needs something to send")]
    EmptyPaneKeyText(String),
    #[error("Enter a tab name.")]
    EmptyTabName,
    #[error("Workspace label must not be empty.")]
    EmptyWorkspaceLabel,
    #[error("Dropped paths must not contain control characters.")]
    FileDropControl,
    #[error("Dropped paths must not be empty.")]
    FileDropEmptyPath,
    #[error("Dropped paths must be UTF-8.")]
    FileDropEncoding,
    #[error("File drop exceeds 256 paths or 64 KiB of quoted text.")]
    FileDropSize,
    #[error("Could not run git in {}: {source}", path.display())]
    GitCommonDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    GitHubAccounts(#[from] GitHubAccountsError),
    #[error(
        "GitHub authentication required. Use menu > GitHub sign-in or set GH_TOKEN / GITHUB_TOKEN."
    )]
    GitHubAuthentication,
    #[error("GitHub authorization failed. Check OAuth application settings.")]
    GitHubAuthorization,
    #[error("GitHub CLI (gh) was not found. Install it and run `gh auth login`, or sign in here.")]
    GitHubCliMissing,
    #[error("Could not run the GitHub CLI (gh).")]
    GitHubCliProcess(#[source] Box<Error>),
    #[error(
        "GitHub rejected the GitHub CLI's token. Run `gh auth login` to renew it, then try again."
    )]
    GitHubCliRejected,
    #[error("GitHub CLI is not signed in to github.com. Run `gh auth login`, then try again.")]
    GitHubCliSignedOut,
    #[error("GitHub CLI did not answer within 10 seconds. Check `gh auth status`, then try again.")]
    GitHubCliTimeout,
    #[error("GitHub authorization denied.")]
    GitHubDenied,
    #[error("Invalid GitHub device authorization response.")]
    GitHubDevice,
    #[error("Invalid GitHub credential encoding.")]
    GitHubEncoding(#[source] std::str::Utf8Error),
    #[error("GitHub code expired. Sign in again.")]
    GitHubExpired,
    #[error("GitHub denied access: check token permissions or SSO authorization.")]
    GitHubForbidden,
    #[error("Invalid GitHub authorization header.")]
    GitHubHeader(#[source] ureq::http::header::InvalidHeaderValue),
    #[error("Invalid GitHub JSON response.")]
    GitHubJson(#[source] GitHubJsonError),
    #[error("GitHub network request failed or timed out ({0}).")]
    GitHubNetwork(#[source] ureq::Error),
    #[error("GitHub query failed. Check token repository permissions and rate limits.")]
    GitHubQuery,
    /// The account's (or, signed out, this address's) API quota is used up.
    #[error("GitHub rate limit reached. {}.", crate::updater::retry_hint(*reset))]
    GitHubRateLimit {
        reset: Option<chrono::DateTime<chrono::Utc>>,
    },
    #[error("GitHub response could not be read within the size/time limit.")]
    GitHubRead(#[source] io::Error),
    /// GitHub's own refusal of a requested change, cleaned and bounded.
    #[error("GitHub refused the request: {0}")]
    GitHubRejected(String),
    /// GraphQL answers a repository the credential cannot reach as missing.
    #[error("GitHub cannot see {repository} with this sign-in. {}", crate::github::hidden_hint(*app_token))]
    GitHubRepositoryHidden { repository: String, app_token: bool },
    #[error(
        "[github] {0} must not be in the config. Sign in from the app, or use GH_TOKEN / GITHUB_TOKEN."
    )]
    GitHubSecretInConfig(&'static str),
    #[error("GitHub response exceeded the size limit.")]
    GitHubSize,
    #[error("GitHub request failed (HTTP {0}). Check network and repository access.")]
    GitHubStatus(u16),
    #[error("Invalid GitHub token. Replace the configured credential.")]
    GitHubToken,
    #[error("Unsupported GitHub token type.")]
    GitHubTokenType,
    #[error("GitHub {0} worker stopped.")]
    GitHubWorker(&'static str),
    #[error("Oversized GIF, WebP, and animated PNG images cannot be resized safely.")]
    ImageAnimationResize,
    #[error("Could not decode the oversized image.")]
    ImageDecode(#[source] image::ImageError),
    #[error("The image exceeds the resize pixel or decoded-memory limit.")]
    ImageDecodeLimit,
    #[error("Could not encode the resized image.")]
    ImageEncode(#[source] image::ImageError),
    #[error("Could not {operation} the local image file.")]
    ImageFile {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("The local image must be a regular file.")]
    ImageFileType,
    #[error("SVG clipboard images are not supported. Use PNG, JPEG, GIF, WebP, BMP, or TIFF.")]
    ImageFormat,
    #[error("The image exceeds the {limit}-byte resize input limit.")]
    ImageInputTooLarge { limit: usize },
    #[error("Reading the local image timed out (3 seconds).")]
    ImageReadTimeout,
    #[error("The image must not be empty.")]
    ImageSize,
    #[error("The image exceeds Herdr's {limit}-byte upload limit.")]
    ImageTooLarge { limit: usize },
    #[error("The page reported an invalid annotation")]
    InvalidAnnotation,
    #[error(
        "Invalid Git branch name. Use a name such as config-reload, without spaces or special ref characters."
    )]
    InvalidBranchName,
    #[error("Invalid saved browser tabs")]
    InvalidBrowserTabs,
    #[error("Browser tabs open only http and https addresses with a host, of at most 8 KiB.")]
    InvalidBrowserUrl,
    #[error(
        "{0} must be 1..256 ASCII letters, digits, '.', '_' or '-' (public client ID, not a secret)"
    )]
    InvalidClientId(&'static str),
    #[error("A code tab must name an absolute file path without control characters")]
    InvalidCodeFile,
    #[error("Invalid saved dispatch history")]
    InvalidDispatchHistory,
    #[error("Not an address with a scheme other than http, https, and file, of at most 8 KiB.")]
    InvalidExternalUrl,
    #[error("Invalid saved editor groups")]
    InvalidGroupLayouts,
    #[error("keybindings.{command}: invalid keystroke {keystroke:?}")]
    InvalidKeystroke {
        command: &'static str,
        keystroke: String,
        #[source]
        source: gpui::InvalidKeystrokeError,
    },
    #[error(
        "Local pages must be a plain file in a folder that is not hidden and does not hold your home directory."
    )]
    InvalidLocalPage,
    #[error("pane_keys: invalid keystroke {keystroke:?}")]
    InvalidPaneKey {
        keystroke: String,
        #[source]
        source: gpui::InvalidKeystrokeError,
    },
    #[error("A saved review tab names a checkout that is not a local absolute path.")]
    InvalidReviewCheckout,
    #[error("invalid sidebar width")]
    InvalidSidebarWidth,
    #[error("sidebar_width_px must be finite and positive, or null")]
    InvalidStoredWidth,
    #[error("Invalid saved window geometry or too many saved windows")]
    InvalidWindowState,
    #[error("Invalid saved worktree notes")]
    InvalidWorktreeNotes,
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "keybindings.{command}: {keystroke:?} needs a cmd, ctrl, alt, or fn modifier so typing still reaches the terminal"
    )]
    KeystrokeWithoutModifier {
        command: &'static str,
        keystroke: String,
    },
    #[cfg(windows)]
    #[error(transparent)]
    LaunchAtLogin(#[from] crate::launch_at_login::Error),
    #[error("File not found: {}", path.display())]
    LinkedFileMissing { path: PathBuf },
    #[error("This link names a file on another computer, {host}. It is not opened from here.")]
    LinkOnAnotherHost { host: String },
    #[error(transparent)]
    ListeningPorts(#[from] crate::listening_ports::Error),
    #[error("Could not list this machine's network addresses: {0}")]
    LocalAddresses(#[source] io::Error),
    #[error("Git could not run")]
    LocalGit(#[source] io::Error),
    /// The search waited for the permission and macOS still refused it.
    /// Turning it on applies to the next search; the app need not restart.
    #[error(
        "macOS has not allowed Herdr on your local network. Turn on Herdr in System Settings › Privacy & Security › Local Network, then search again"
    )]
    LocalNetworkDenied,
    #[error("Worktree notes stay in this app and are never sent to the daemon")]
    LocalWorktreeNote,
    #[error("Could not read the log {}: {source}", path.display())]
    LogRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("This link's address is not valid, so it opens nothing.")]
    MalformedLink,
    #[error("Reopen the deletion dialog.")]
    MissingDeletion,
    #[error("HOME is not set")]
    MissingHome,
    #[error("neither XDG_STATE_HOME nor HOME is set")]
    MissingStateRoot,
    #[error("No current daemon snapshot.")]
    NoSnapshot,
    #[error("No tab selected.")]
    NoTab,
    #[error("{} is not in a Git repository", path.display())]
    NotAGitRepository { path: PathBuf },
    #[error("Not connected to a daemon.")]
    NotConnected,
    #[error("The current directory is not in a Git checkout.")]
    NotInCheckout,
    #[error("Could not read the orchestrator's database: {0}")]
    OrchestratorItems(#[source] crate::orchestrator::Error),
    #[error(transparent)]
    Overview(#[from] crate::overview::Error),
    #[cfg(target_os = "macos")]
    #[error("The window has no view to place a page beneath")]
    PageStack,
    #[error(transparent)]
    Palette(#[from] crate::palette::Error),
    #[error(transparent)]
    PaneCarry(#[from] crate::pane_menu::carry::Refusal),
    #[error("Could not decode an image a pane placed.")]
    PaneImageDecode(#[source] image::ImageError),
    #[error("An image a pane placed exceeds the decoded size limit.")]
    PaneImageLimit,
    #[error("pane_keys: {keystroke:?} is also bound to keybindings.{command}")]
    PaneKeyBound {
        keystroke: String,
        command: &'static str,
    },
    #[error("pane_keys.{keystroke:?}: text may be at most {limit} bytes")]
    PaneKeyTextTooLong { keystroke: String, limit: usize },
    #[error(
        "pane_keys: {0:?} needs a cmd, ctrl, alt, or fn modifier so typing still reaches the terminal"
    )]
    PaneKeyWithoutModifier(String),
    #[error(transparent)]
    PaneMove(#[from] crate::pane_move::Error),
    #[error("{}: {source}", path.display())]
    Path {
        path: PathBuf,
        #[source]
        source: Box<Error>,
    },
    #[cfg(target_os = "macos")]
    #[error("The page that opened a popup is no longer in a window")]
    PopupOpener,
    #[error(transparent)]
    PortForward(#[from] crate::port_forward::Error),
    #[error("Daemon did not provide an absolute checkout and repository key.")]
    PrAbsolutePath,
    #[error("A pull request action is already running.")]
    PrActionBusy,
    #[error("Could not start the pull request worker.")]
    PrActionProcess {
        #[source]
        source: io::Error,
    },
    #[error(
        "This pull request cannot be acted on here: it is not open, or its details are incomplete. Refresh and try again."
    )]
    PrActionTarget,
    #[error("Pull request worker stopped. Check the pull request on GitHub before retrying.")]
    PrActionWorker,
    #[error("Multiple PRs match this branch; no PR selected.")]
    PrAmbiguous,
    #[error("Multiple local worktrees match the daemon branch.")]
    PrAmbiguousWorktree,
    #[error("No supported branch available.")]
    PrBranch,
    #[error("Checkout branch changed. Waiting for daemon metadata.")]
    PrBranchChanged,
    #[error("PR lookup cancelled.")]
    PrCancelled,
    #[error("Local checkout unavailable or not a trusted Git repository.")]
    PrCheckout,
    #[error("Enter a comment of at most 4096 characters.")]
    PrCommentBody,
    #[error("preferences path has no parent")]
    PreferencesPath,
    #[error("Invalid process text.")]
    PrEncoding(#[source] std::str::Utf8Error),
    #[error("PR identity does not match the repository and branch.")]
    PrIdentity,
    #[error("The branch changed since this dialog opened. Review the pull request again.")]
    PrMergeChanged,
    #[error("The repository does not allow this merge method.")]
    PrMergeMethod,
    #[error("No repository metadata.")]
    PrMetadata,
    #[error("No local worktree matches the daemon branch.")]
    PrMissingWorktree,
    #[error(transparent)]
    Processes(#[from] crate::processes::Error),
    #[error("PR lookup supports GitHub.com origins only.")]
    PrOrigin,
    #[error("Could not {operation} for Git PR lookup.")]
    PrProcess {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("GitHub repository unavailable. Check repository access and token permissions.")]
    PrRepository,
    #[error("Local repository does not match daemon metadata.")]
    PrRepositoryMismatch,
    #[error("PR response exceeded the size limit.")]
    PrSize,
    #[error("PR lookup timed out (15 seconds).")]
    PrTimeout,
    #[error(
        "PR lookup requires your owned local session socket or a saved SSH device. Other socket locations are unsupported."
    )]
    PrUntrustedEndpoint,
    #[error("Workspace directory is not in a Git repository.")]
    PrWorkspaceRepository,
    #[error("Local repository unavailable for worktree lookup.")]
    PrWorktreeLookup,
    #[error(transparent)]
    Px0(#[from] crate::px0::Error),
    #[error("XDG_CONFIG_HOME must be an absolute path")]
    RelativeConfigRoot,
    /// A cloud machine, provider, sandbox image, or teleport failure from
    /// `herdr-remote`; see `error/remote.rs` for why it is not `#[from]`.
    #[error(transparent)]
    Remote(herdr_remote::Error),
    #[error("Could not install the agent tools on the SSH host: {0}")]
    RemoteAgentTools(#[source] herdr_client::Error),
    #[error("{method}: {source}")]
    Request {
        method: herdr_client::Method,
        #[source]
        source: herdr_client::Error,
    },
    #[error(
        "No base branch to compare with: neither the pull request's base nor origin/HEAD, main, or master exists locally. Pick one with the base button above."
    )]
    ReviewNoBase,
    #[error("These changes are too large to review here, even leaving out the largest files.")]
    ReviewTooLarge,
    #[error("The page's picture could not be turned into a PNG")]
    ScreenshotImage,
    #[error("Could not write the screenshot to {}: {source}", path.display())]
    ScreenshotWrite {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("The selection reaches rows the pane no longer shows.")]
    SelectionOffscreen,
    #[error("Selection is too large to copy.")]
    SelectionSize,
    #[error("The selected pane is no longer on screen.")]
    SelectionStale,
    #[error("the host did not publish its keybindings")]
    ServerKeybindingsMissing,
    #[error("the host's keybindings have no [keys] table")]
    ServerKeybindingsNoKeys,
    #[error("the host's keybindings are not valid TOML: {0}")]
    ServerKeybindingsParse(#[source] toml::de::Error),
    #[error("the host's keybindings exceed {max} bytes")]
    ServerKeybindingsTooLarge { max: usize },
    #[error(transparent)]
    Settings(#[from] crate::config::Error),
    #[error("Could not finish saving Settings: {0}")]
    SettingsSave(#[source] std::sync::Arc<Error>),
    #[cfg(unix)]
    #[error(transparent)]
    SetupRelay(#[from] crate::setup_relay::Error),
    #[error("Could not install the agent skill at {}: {source}", path.display())]
    SkillInstall {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Sound(#[from] crate::sound::Error),
    #[error("Could not read {}: {source}", path.display())]
    SshConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("The original target changed or no longer exists. Cancel and try again.")]
    StaleCloseTarget,
    #[error("The selected connection changed or is not ready. Cancel and try again.")]
    StaleConnection,
    #[error("The original pane changed or no longer exists. Cancel and try again.")]
    StalePane,
    #[error("The original tab changed or no longer exists. Cancel and try again.")]
    StaleTab,
    #[error("Workspace is no longer available. Dismiss and reopen the menu.")]
    StaleWorkspace,
    #[error("{} exceeds {limit} bytes", path.display())]
    StateFileSize { path: PathBuf, limit: u64 },
    #[error(transparent)]
    SystemLoad(#[from] crate::system_load::Error),
    #[error(transparent)]
    TabLayout(#[from] crate::config::tab_layout::Error),
    #[error("Tailscale returned an unreadable status: {0}")]
    TailscaleJson(#[source] serde_json::Error),
    #[error("Tailscale did not answer in time")]
    TailscaleTimeout,
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    TomlEdit(#[from] toml_edit::TomlError),
    #[error("keybindings.{0} must list at most 8 keystrokes")]
    TooManyKeystrokes(&'static str),
    #[error("pane_keys must list at most {0} keystrokes")]
    TooManyPaneKeys(usize),
    #[error(transparent)]
    Tunnel(#[from] crate::listening_ports::tunnel::Error),
    #[error("keybindings.{0} is not a command; see the keybindings list in config-gpui.toml")]
    UnknownKeybinding(String),
    #[error("pane_keys.{from:?}: a pane cannot receive {to:?}")]
    UnsendablePaneKey { from: String, to: String },
    #[error("This command action is not supported by this client.")]
    UnsupportedCommand,
    #[error("{0}")]
    Update(#[from] UpdateError),
    /// A usage read failed, or the usage shell the port and load scans share.
    #[error(transparent)]
    Usage(#[from] herdr_usage::Error),
    /// A Git, checkpoint, or code index failure; see `error/vcs.rs` for
    /// what is handled elsewhere.
    #[error(transparent)]
    Vcs(herdr_vcs::Error),
    #[cfg(any(target_os = "macos", windows))]
    #[error("Could not create the page: {0}")]
    WebView(#[from] wry::Error),
    #[cfg(any(target_os = "macos", windows))]
    #[error("Could not reach the native window for the page: {0}")]
    WindowHandle(#[from] WindowHandleError),
    #[error("Checkout changed. Dismiss and reopen the menu.")]
    WorkspaceCheckoutChanged,
    #[error("Workspace group changed. Dismiss and review the group again.")]
    WorkspaceGroupChanged,
    #[error("Repository changed. Dismiss and reopen the menu.")]
    WorkspaceRepositoryChanged,
    #[error(transparent)]
    WorktreeIdentity(#[from] crate::worktree_identity::Error),
    #[error("Invalid or oversized worktree list. Dismiss and reopen the menu.")]
    WorktreeList,
    #[error("Malformed worktree list. Dismiss and reopen the menu.")]
    WorktreeListDecode(#[source] serde_json::Error),
    #[error(transparent)]
    WorktreeScripts(#[from] crate::worktree_scripts::Error),
    #[error("Select a checkout from the daemon's worktree list.")]
    WorktreeSelection,
    #[error("Another drop's paths are still being mapped in WSL. Try again in a moment.")]
    WslPasteBusy,
    #[error("The pane the files were dropped on changed before their paths were ready.")]
    WslPasteTargetGone,
    // keep-sorted end
}

mod clipboard;
mod config;
mod github;
mod path;
mod remote;
mod vcs;

#[cfg(test)]
mod tests;
