//! Internal failures retain their categories and sources until presentation.
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
    #[error(transparent)]
    Clipboard(#[from] crate::window::clipboard::Error),
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
    Control(#[from] crate::control::Error),
    #[error(transparent)]
    Credentials(#[from] herdr_credentials::Error),
    #[error(transparent)]
    Daemon(#[from] crate::daemon::Error),
    #[error("Checkout lookup failed. Dismiss and reopen the menu.")]
    DeletionLookup,
    #[error("HEAD is detached. Check a branch out first.")]
    DetachedHead,
    #[error(transparent)]
    Devices(#[from] crate::menu::devices::Error),
    #[error(transparent)]
    Dispatch(#[from] crate::dispatch::Error),
    #[error(transparent)]
    Editor(#[from] crate::editor::Error),
    #[error("Enter a tab name.")]
    EmptyTabName,
    #[error("Workspace label must not be empty.")]
    EmptyWorkspaceLabel,
    #[error(transparent)]
    FileDrop(#[from] crate::window::file_drop::Error),
    #[error("Could not run git in {}: {source}", path.display())]
    GitCommonDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    GitHub(#[from] herdr_github::Error),
    #[error(
        "[github] {0} must not be in the config. Sign in from the app, or use GH_TOKEN / GITHUB_TOKEN."
    )]
    GitHubSecretInConfig(&'static str),
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
    #[error("A code tab must name an absolute file path without control characters")]
    InvalidCodeFile,
    #[error("Not an address with a scheme other than http, https, and file, of at most 8 KiB.")]
    InvalidExternalUrl,
    #[error("Invalid saved editor groups")]
    InvalidGroupLayouts,
    #[error(
        "Local pages must be a plain file in a folder that is not hidden and does not hold your home directory."
    )]
    InvalidLocalPage,
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
    #[error(transparent)]
    Keymap(#[from] crate::keymap::Error),
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
    #[error(transparent)]
    Mods(#[from] crate::mods::Error),
    #[error("No current daemon snapshot.")]
    NoSnapshot,
    #[error("No tab selected.")]
    NoTab,
    #[error("{} is not in a Git repository", path.display())]
    NotAGitRepository { path: PathBuf },
    #[error("Not connected to a daemon.")]
    NotConnected,
    #[error(transparent)]
    NotificationMutes(#[from] crate::notification_mutes::Error),
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
    #[error("preferences path has no parent")]
    PreferencesPath,
    #[error(transparent)]
    Processes(#[from] crate::processes::Error),
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
    #[error(transparent)]
    RemoteControl(#[from] crate::remote::Error),
    #[error("{method}: {source}")]
    Request {
        method: herdr_client::Method,
        #[source]
        source: herdr_client::Error,
    },
    #[error(transparent)]
    Review(#[from] crate::review::Error),
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
    #[error(transparent)]
    ServerKeybindings(#[from] crate::keymap::daemon::Error),
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
    #[error(transparent)]
    SshHosts(#[from] crate::config::ssh_hosts::Error),
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
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    TomlEdit(#[from] toml_edit::TomlError),
    #[error(transparent)]
    Tunnel(#[from] crate::listening_ports::tunnel::Error),
    #[error("This command action is not supported by this client.")]
    UnsupportedCommand,
    #[error("{0}")]
    Update(#[from] crate::updater::Error),
    /// A usage read failed, or the usage shell the port and load scans share.
    #[error(transparent)]
    Usage(#[from] herdr_usage::Error),
    /// A Git, checkpoint, or code index failure. Build it with `.into()`, never
    /// directly: `error/vcs.rs` sends a GitHub failure carried by `herdr-vcs`
    /// to `GitHub`, where callers match it.
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
    #[error(transparent)]
    WslDrop(#[from] crate::window::wsl_drop::Error),
    // keep-sorted end
}

mod config;
mod github;
mod path;
mod remote;
mod vcs;

#[cfg(test)]
mod tests;
