//! Internal failures retain their categories and sources until presentation.
pub use crate::updater::UpdateError;
use daemon::daemon_error_message;
pub use github::{GitHubAccountsError, GitHubJsonError};
use std::{
    io,
    path::{Path, PathBuf},
};
pub use theme::ThemeParseError;

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
    Backdrop(#[from] crate::backdrop::Error),
    #[cfg(feature = "boat")]
    #[error(transparent)]
    Boat(#[from] crate::boat::Error),
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
    #[error("Could not keep the machine awake: {0}")]
    Caffeine(#[source] keepawake::Error),
    #[error("No agent is working, so there is nothing to stay awake for")]
    CaffeineNoWorkingAgents,
    #[error("Could not start the keep-awake thread")]
    CaffeineThread(#[source] io::Error),
    #[error("The keep-awake thread stopped; the display may sleep")]
    CaffeineWorkerStopped,
    #[error("That checkpoint was taken on another branch. Check that branch out to restore it.")]
    CheckpointBranch,
    #[error("A merge, rebase, cherry-pick, or revert is in progress. Finish or abort it first.")]
    CheckpointBusy,
    #[error("No checkout of this branch was found on its host.")]
    CheckpointCheckout,
    #[error("That checkpoint no longer exists.")]
    CheckpointMissing,
    #[error("Checkpoint output was not understood.")]
    CheckpointOutput,
    #[error("Checkpoint Git commands failed")]
    CheckpointScript(#[source] herdr_client::Error),
    #[error("Could not start the checkpoint worker")]
    CheckpointThread(#[source] io::Error),
    #[error("This checkout has no commit yet, so it has no checkpoints.")]
    CheckpointUnborn,
    #[error("The checkpoint worker stopped.")]
    CheckpointWorker,
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
    #[error("The clipboard changed while reading. Try pasting again.")]
    ClipboardChanged,
    #[error("Clipboard text is not valid UTF-8.")]
    ClipboardEncoding(#[source] std::str::Utf8Error),
    #[error("Could not read the clipboard; on Linux, install wl-paste or xclip.")]
    ClipboardProcess(#[source] io::Error),
    #[error("Clipboard content exceeds the {limit}-byte limit.")]
    ClipboardSize { limit: usize },
    #[error("Clipboard acquisition timed out.")]
    ClipboardTimeout,
    #[error("Remote clipboard acquisition is not supported on this platform.")]
    ClipboardUnsupported,
    #[cfg(feature = "cloud")]
    #[error(transparent)]
    Cloud(#[from] crate::cloud::Error),
    #[error(transparent)]
    CloudSetup(#[from] crate::cloud_setup::Error),
    #[error(transparent)]
    Code(#[from] crate::code_server::Error),
    #[error("This file is missing, larger than 1 MiB, or not UTF-8 text")]
    CodeFileUnreadable,
    #[error("Reading the checkout was cancelled")]
    CodeIndexCancelled,
    #[error("Only a local Git checkout can be searched for files and symbols")]
    CodeIndexRoot,
    #[cfg(feature = "coder")]
    #[error(transparent)]
    Coder(#[from] crate::coder::Error),
    #[error("{0}")]
    Config(#[source] config_loader::ConfigError),
    #[error("{source}")]
    ConfigFile {
        uri: Option<String>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("{key}: a part of this key holds a value, not a table; unset that part first")]
    ConfigKeyNotTable { key: String },
    #[error("Cannot migrate {}: {} already contains different settings. Merge your old settings into the local file, then remove the old config; it will be regenerated.", original.display(), local.display())]
    ConfigMigrationConflict { original: PathBuf, local: PathBuf },
    #[error("Connection is busy. Try again.")]
    ConnectionBusy,
    #[error("The connection is not ready. Try again.")]
    ConnectionNotReady,
    #[error("Control request or response exceeds {limit} bytes")]
    ControlMessageSize { limit: usize },
    #[error("Herdr UI closed the control connection without answering")]
    ControlNoResponse,
    #[error("{} is held by something other than this user's Herdr UI, so it is not used", path.display())]
    ControlPipeForeign { path: PathBuf },
    #[error("{message}")]
    ControlRejected {
        code: crate::control::ErrorCode,
        message: String,
    },
    #[error("Another Herdr UI already serves {}", path.display())]
    ControlSocketInUse { path: PathBuf },
    #[error("Herdr UI is not running, or its control socket {} is unreachable: {source}", path.display())]
    ControlUnavailable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Could not identify the Windows user: {0}")]
    ControlUser(#[source] io::Error),
    #[error("Missing credential directory.")]
    CredentialDirectory,
    #[error("Cannot access private credential file.")]
    CredentialIo(#[source] io::Error),
    #[cfg(windows)]
    #[error(transparent)]
    CredentialManager(#[from] crate::github::CredentialManagerError),
    #[error(
        "Cannot access private credential file. Require an owned directory and regular 0600 file; symlinks are rejected."
    )]
    CredentialPermissions,
    #[error(
        "No secure credential store configured. Explicitly opt in with allow_plaintext_credentials = true in the [github] or [coder] table, or use GH_TOKEN / GITHUB_TOKEN for GitHub."
    )]
    CredentialPolicy,
    #[error(
        "No GitHub credential store is available on this platform. Use GH_TOKEN / GITHUB_TOKEN."
    )]
    CredentialUnsupported,
    #[error("daemon startup cancelled")]
    DaemonCancelled,
    #[error("herdr server exited before accepting connections: {0}. Check the Herdr server log.")]
    DaemonExited(std::process::ExitStatus),
    #[error("{}", daemon_error_message(.0))]
    DaemonResponse(serde_json::Value),
    #[error("Could not start herdr server: {source}. Use Terminal > Reconnect to retry.")]
    DaemonSpawn {
        #[source]
        source: io::Error,
    },
    #[error("Timed out waiting for herdr server at {}. Check the Herdr server log and use Terminal > Reconnect.", .0.display())]
    DaemonTimeout(PathBuf),
    #[cfg(feature = "daytona")]
    #[error(transparent)]
    Daytona(#[from] crate::daytona::Error),
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
    /// A `[devices.<id>]` theme that does not parse, named by its device.
    #[error("devices.{device}.theme: {source}")]
    DeviceTheme {
        device: String,
        #[source]
        source: Box<Error>,
    },
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
    #[error("Another file is still opening in the editor")]
    EditorBusy,
    #[error(
        "editor_command must be at most 1024 bytes without quotes, backslashes, or control characters"
    )]
    EditorCommand,
    #[error("No local pane to open the editor beside")]
    EditorNoPane,
    #[error("This path cannot be typed into a shell safely, so it opens in the default app")]
    EditorPath,
    #[error("The Neovim in the editor pane did not answer")]
    EditorRemote,
    #[error("The Neovim in the editor pane is busy: answer it, then try again")]
    EditorRemoteBusy,
    #[error("The Neovim in the editor pane could not open the file")]
    EditorRemoteFailed,
    #[error("Could not run nvim to reach the editor pane")]
    EditorRemoteLaunch(#[source] io::Error),
    #[error(transparent)]
    EditorRequest(std::sync::Arc<Error>),
    #[error("Unexpected daemon response while opening the editor pane")]
    EditorResponse,
    #[error("Opening a terminal editor needs a Unix shell in the pane")]
    EditorUnsupported,
    #[error("{0}.fallback families must not be empty")]
    EmptyFontFallback(&'static str),
    #[error("{0}.family must not be empty")]
    EmptyFontFamily(&'static str),
    #[error("pane_keys.{0:?}: text: needs something to send")]
    EmptyPaneKeyText(String),
    #[error("Enter a tab name.")]
    EmptyTabName,
    #[error("theme must not be empty")]
    EmptyTheme,
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
    #[error("Port {0} is already forwarded from this host.")]
    ForwardDuplicate(u16),
    #[error("At most {0} ports can be forwarded at once.")]
    ForwardLimit(usize),
    #[error("Enter a port number from 1 to 65535.")]
    ForwardPort,
    #[error("A Git operation is already running.")]
    GitBusy,
    #[error("Enter a commit message of at most 4096 characters.")]
    GitCommitMessage,
    #[error("Could not run git in {}: {source}", path.display())]
    GitCommonDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("git {operation} failed: {details}")]
    GitFailed {
        operation: &'static str,
        details: String,
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
    #[error("No local checkout is focused.")]
    GitNoCheckout,
    #[error("Nothing to commit: the checkout has no changes.")]
    GitNothingToCommit,
    #[error("Could not {operation}.")]
    GitProcess {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("This branch is the repository default branch; no pull request can be opened from it.")]
    GitPullRequestBase,
    #[error("The branch has no commit to describe. Commit before opening a pull request.")]
    GitPullRequestTitle,
    #[error("Git worker stopped. Retry the operation.")]
    GitWorker,
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
    #[error("code must be a TOML table")]
    InvalidCodeTable,
    #[error("devices.{0:?} is not a saved device ID")]
    InvalidDeviceId(String),
    #[error("devices and each devices.<id> must be tables")]
    InvalidDevicesTable,
    #[error("Invalid saved dispatch history")]
    InvalidDispatchHistory,
    #[error("Not an address with a scheme other than http, https, and file, of at most 8 KiB.")]
    InvalidExternalUrl,
    #[error("{0}.size must be finite and between 8 and 48 logical pixels")]
    InvalidFontSize(&'static str),
    #[error("{0}.weight must be a weight name such as \"medium\" or a number from 100 to 900")]
    InvalidFontWeight(&'static str),
    #[error("Invalid saved editor groups")]
    InvalidGroupLayouts,
    #[error("sidebar.hosts.{host:?} must be a #rgb or #rrggbb colour, not {value:?}")]
    InvalidHostColor { host: String, value: String },
    #[error("keybindings.{command}: invalid keystroke {keystroke:?}")]
    InvalidKeystroke {
        command: &'static str,
        keystroke: String,
        #[source]
        source: gpui::InvalidKeystrokeError,
    },
    #[error("{0}.line_height must be a finite number between 1 and 2 times the font size")]
    InvalidLineHeight(&'static str),
    #[error(
        "Local pages must be a plain file in a folder that is not hidden and does not hold your home directory."
    )]
    InvalidLocalPage,
    #[error("Invalid saved palette history")]
    InvalidPaletteFrecency,
    #[error("pane_keys: invalid keystroke {keystroke:?}")]
    InvalidPaneKey {
        keystroke: String,
        #[source]
        source: gpui::InvalidKeystrokeError,
    },
    #[error("A saved review tab names a checkout that is not a local absolute path.")]
    InvalidReviewCheckout,
    #[error("layout.sidebar_gap must be finite and between 0 and 64 logical pixels")]
    InvalidSidebarGap,
    #[error("sidebar.{key} must be finite and between 0 and {max} logical pixels")]
    InvalidSidebarMetric { key: &'static str, max: f32 },
    #[error("invalid sidebar width")]
    InvalidSidebarWidth,
    #[error("sound.volume must be between 0 and 100")]
    InvalidSoundVolume,
    #[error("sidebar_width_px must be finite and positive, or null")]
    InvalidStoredWidth,
    #[error(
        "terminal.{0} must be one number or a pair of numbers, each finite and between 0 and 64 logical pixels"
    )]
    InvalidTerminalPadding(&'static str),
    #[error("a theme that follows the system must name both sides: light:NAME,dark:NAME")]
    InvalidThemePair,
    #[error("theme must be a name, absolute path, or ~/ path")]
    InvalidThemePath,
    #[error("usage must be a TOML table")]
    InvalidUsageTable,
    #[error("Invalid saved window geometry or too many saved windows")]
    InvalidWindowState,
    #[error("Invalid saved worktree notes")]
    InvalidWorktreeNotes,
    #[error("zen.font_size must be finite and between 8 and 48 logical pixels")]
    InvalidZenFontSize,
    #[error("zen.width must be 20 to 1000 columns, or a percentage from \"10%\" to \"100%\"")]
    InvalidZenWidth,
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[cfg(target_os = "macos")]
    #[error("Cannot read GitHub Keychain entry. Unlock your login Keychain or set GH_TOKEN.")]
    KeychainRead(#[source] security_framework::base::Error),
    #[cfg(target_os = "macos")]
    #[error("GitHub Keychain update failed. Unlock your login Keychain and try again.")]
    KeychainWrite(#[source] security_framework::base::Error),
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
    #[error("Could not read listening ports on this host.")]
    ListeningPorts(#[source] Box<Error>),
    #[error("Neither ss nor lsof is installed on this host, so listening ports cannot be read.")]
    ListeningPortsTool,
    #[error("Listening ports cannot be read on this platform.")]
    ListeningPortsUnsupported,
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
    #[error("No captured daemon session. Reopen the palette.")]
    NoPaletteSession,
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
    #[error("This command changed or was removed. Reopen the palette.")]
    PaletteCommandChanged,
    #[error("The selected connection is not ready.")]
    PaletteConnectionNotReady,
    #[error("This agent or terminal no longer exists. Reopen the palette.")]
    PaletteDestinationRemoved,
    #[error("This host is no longer connected. Reopen the palette.")]
    PaletteHostUnavailable,
    #[error("The local connection changed. Reopen the palette.")]
    PaletteLocalChanged,
    #[error("The original pane no longer exists in its tab. Reopen the palette.")]
    PalettePaneRemoved,
    #[error("git clone failed: {0}")]
    PaletteProjectClone(String),
    #[error("Could not create the project folder: {source}")]
    PaletteProjectCreate {
        #[source]
        source: io::Error,
    },
    #[error("A folder of that name already exists in the project root")]
    PaletteProjectExists,
    #[error("Project discovery reached its directory limit; narrow palette.project_roots")]
    PaletteProjectLimit,
    #[error("Project paths must be absolute UTF-8 paths without control characters")]
    PaletteProjectPath,
    #[error("Project directory is no longer available")]
    PaletteProjectRemoved,
    #[error("Unexpected workspace creation response. Review workspace state before retrying.")]
    PaletteProjectResponse,
    #[error("The first configured project root is not available")]
    PaletteProjectRootMissing,
    #[error("palette.project_roots must contain at most 16 nonempty paths of at most 8192 bytes")]
    PaletteProjectRoots,
    #[error("Workspace directories changed. Select the project again.")]
    PaletteProjectStateChanged,
    #[error("Project path references an unset environment variable: {0}")]
    PaletteProjectVariable(String),
    #[error("The daemon session changed. Reopen the palette.")]
    PaletteSessionChanged,
    #[error("The original tab no longer exists in its workspace. Reopen the palette.")]
    PaletteTabRemoved,
    #[error("This workspace no longer exists. Reopen the palette.")]
    PaletteWorkspaceRemoved,
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
    #[error("preferences must be an object")]
    PreferencesNotObject,
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
    #[error("Herdr's answer about the pane's process was not understood.")]
    ProcessesAnswer(#[source] serde_json::Error),
    #[error("Still ending the last processes. Try again in a moment.")]
    ProcessesBusy,
    #[error("The herdr executable's path is not valid UTF-8.")]
    ProcessesExecutable,
    #[error("Herdr did not name a process for this pane.")]
    ProcessesNoRoot,
    #[error("Could not ask Herdr which process the pane runs.")]
    ProcessesQuery(#[source] herdr_client::Error),
    #[error("The pane's process has exited.")]
    ProcessesRootExited,
    #[error("The process list stopped updating. Reopen it to try again.")]
    ProcessesStopped,
    #[error("Could not start watching the pane's processes.")]
    ProcessesWorker(#[source] io::Error),
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
    #[cfg(feature = "railway")]
    #[error(transparent)]
    Railway(#[from] crate::railway::Error),
    #[error("XDG_CONFIG_HOME must be an absolute path")]
    RelativeConfigRoot,
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
    #[cfg(target_os = "linux")]
    #[error(
        "Cannot read GitHub sign-in from the desktop keyring. Unlock your keyring or set GH_TOKEN."
    )]
    SecretServiceRead(#[source] Box<oo7::Error>),
    #[cfg(target_os = "linux")]
    #[error(
        "No desktop keyring (Secret Service) is running. Start GNOME Keyring, KWallet, or KeePassXC, or set [github] allow_plaintext_credentials = true, or use GH_TOKEN / GITHUB_TOKEN."
    )]
    SecretServiceUnavailable,
    #[cfg(target_os = "linux")]
    #[error("GitHub keyring update failed. Unlock your desktop keyring and try again.")]
    SecretServiceWrite(#[source] Box<oo7::Error>),
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
    #[error("Could not finish saving Settings: {0}")]
    SettingsSave(#[source] std::sync::Arc<Error>),
    #[cfg(unix)]
    #[error("Herdr closed the setup before it finished. Start it again from Add Device.")]
    SetupRelayClosed,
    #[cfg(unix)]
    #[error("This setup is no longer waiting. Start it again from Add Device.")]
    SetupRelayConnect(#[source] io::Error),
    #[cfg(unix)]
    #[error("{} is not a private folder of this user, so the setup cannot listen there.", .0.display())]
    SetupRelayDir(PathBuf),
    #[cfg(unix)]
    #[error("The setup terminal sent something unreadable.")]
    SetupRelayFrame,
    #[cfg(unix)]
    #[error("The device setup needs a terminal.")]
    SetupRelayTerminal,
    #[error("Could not install the agent skill at {}: {source}", path.display())]
    SkillInstall {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Audio playback cancelled")]
    SoundCancelled,
    #[error("Sound configuration exceeds 1 MiB")]
    SoundConfigSize,
    #[error("Could not decode MP3 sound: {0}")]
    SoundDecode(#[from] rodio::decoder::DecoderError),
    #[error("Could not open audio output: {0}")]
    SoundDevice(#[from] rodio::DeviceSinkError),
    #[error("Could not read sound file {}: {source}", path.display())]
    SoundFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Sound must be a regular file of at most 16 MiB")]
    SoundFileSize,
    #[error("Audio output failed: {0}")]
    SoundStream(#[from] rodio::cpal::StreamError),
    #[error("Audio playback timed out")]
    SoundTimeout,
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
    #[error("CPU and memory output was not understood.")]
    SystemLoadOutput,
    #[error("Could not read CPU and memory on this host.")]
    SystemLoadRemote(#[source] Box<Error>),
    /// The host's `uname -s`, bounded, so the message names what it is.
    #[error("CPU and memory cannot be read on {0:?} hosts.")]
    SystemLoadUnsupported(String),
    #[error("Tailscale returned an unreadable status: {0}")]
    TailscaleJson(#[source] serde_json::Error),
    #[error("Tailscale did not answer in time")]
    TailscaleTimeout,
    #[error(transparent)]
    Teleport(#[from] crate::teleport::Error),
    #[error("line {line}: {key}: {source}")]
    ThemeLine {
        line: usize,
        key: String,
        #[source]
        source: ThemeParseError,
    },
    #[error("theme {name:?} not found in {directories:?}")]
    ThemeNotFound {
        name: String,
        directories: Vec<PathBuf>,
    },
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    TomlEdit(#[from] toml_edit::TomlError),
    #[error("[devices] must list at most {0} devices")]
    TooManyDevices(usize),
    #[error("{0}.fallback must list at most 8 families")]
    TooManyFontFallbacks(&'static str),
    #[error("keybindings.{0} must list at most 8 keystrokes")]
    TooManyKeystrokes(&'static str),
    #[error("pane_keys must list at most {0} keystrokes")]
    TooManyPaneKeys(usize),
    #[error("SSH ended before the tunnel opened ({0}).")]
    TunnelExited(std::process::ExitStatus),
    #[error("No free local port for an SSH tunnel.")]
    TunnelPort(#[source] io::Error),
    #[error("Could not start ssh for a tunnel.")]
    TunnelStart(#[source] io::Error),
    #[error("The SSH tunnel did not open in time.")]
    TunnelTimeout,
    #[error("Herdr UI has no setting {}; config-gpui.toml lists every setting", .0.join(", "))]
    UnknownConfigKeys(Vec<String>),
    #[error("keybindings.{0} is not a command; see the keybindings list in config-gpui.toml")]
    UnknownKeybinding(String),
    #[error("Unknown sidebar layout {0:?}")]
    UnknownLayout(String),
    #[error("pane_keys.{from:?}: a pane cannot receive {to:?}")]
    UnsendablePaneKey { from: String, to: String },
    #[error("This command action is not supported by this client.")]
    UnsupportedCommand,
    #[error("{0}")]
    Update(#[from] UpdateError),
    #[error("Usage command failed: {0}.")]
    UsageCommand(&'static str),
    #[error("Could not reach the usage service from this host.")]
    UsageConnect,
    #[error("Saved sign-in cannot be sent as a header.")]
    UsageHeader(#[source] ureq::http::header::InvalidHeaderValue),
    /// The body may hold account details, so only the parser's category is kept.
    #[error("Usage response was not the expected JSON.")]
    UsageJson(serde_json::error::Category),
    #[error("Reading this sign-in needs Keychain access, which macOS asks for.")]
    UsageKeychainAccess,
    #[error("Keychain access was denied, so this sign-in cannot be read.")]
    UsageKeychainDenied,
    #[error("curl is not installed on this host, so usage cannot be read.")]
    UsageMissingCurl,
    #[error("Usage request mixes this machine's settings with the remote host's sign-in.")]
    UsageMixedSecrets,
    #[error("Usage request failed or timed out.")]
    UsageNetwork(#[source] ureq::Error),
    #[error("This account has no plan with usage limits to show.")]
    UsageNoPlan,
    #[error("No sign-in found on this host. Set it up under [usage.providers] in the config.")]
    UsageNotSignedIn,
    #[error("Could not {operation}.")]
    UsageProcess {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("Usage is rate limited. Retrying later.")]
    UsageRateLimited,
    #[error("Saved sign-in was rejected. Open the agent to sign in again.")]
    UsageRejected,
    #[error("Usage output exceeded the size limit.")]
    UsageSize,
    #[error("Usage service returned HTTP {0}.")]
    UsageStatus(u16),
    #[error("Usage check timed out.")]
    UsageTimeout,
    #[error("Could not reach this host over SSH to read usage.")]
    UsageUnreachable,
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
    #[error("Invalid or oversized worktree list. Dismiss and reopen the menu.")]
    WorktreeList,
    #[error("Malformed worktree list. Dismiss and reopen the menu.")]
    WorktreeListDecode(#[source] serde_json::Error),
    #[error("Another worktree script is still starting")]
    WorktreeScriptsBusy,
    #[error("The daemon did not identify this workspace's checkout")]
    WorktreeScriptsCheckout,
    #[error(".herdr/worktree.toml is not valid UTF-8")]
    WorktreeScriptsEncoding(#[source] std::str::Utf8Error),
    #[error("This workspace is not a Git checkout Herdr knows yet")]
    WorktreeScriptsNotGit,
    #[error(".herdr/worktree.toml scripts must not contain NUL bytes")]
    WorktreeScriptsNul,
    #[error(".herdr/worktree.toml: {0}")]
    WorktreeScriptsParse(#[source] toml::de::Error),
    #[error("Could not read .herdr/worktree.toml: {0}")]
    WorktreeScriptsRead(#[source] io::Error),
    #[error("Could not read .herdr/worktree.toml on the host: {0}")]
    WorktreeScriptsRemote(#[source] herdr_client::Error),
    #[error("{0}")]
    WorktreeScriptsRequest(#[source] std::sync::Arc<Error>),
    #[error("Unexpected daemon response while opening the script's tab")]
    WorktreeScriptsResponse,
    #[error(".herdr/worktree.toml exceeds {limit} bytes")]
    WorktreeScriptsSize { limit: u64 },
    #[error("Worktree scripts cannot be read from a WSL distribution yet")]
    WorktreeScriptsUnsupportedHost,
    #[error("Select a checkout from the daemon's worktree list.")]
    WorktreeSelection,
    /// A shell into a WSL distribution starts through `wsl.exe`, which only a
    /// Windows client has.
    #[error("WSL distributions can only be reached from Windows.")]
    WslHostUnsupported,
    #[error("Another drop's paths are still being mapped in WSL. Try again in a moment.")]
    WslPasteBusy,
    #[error("The pane the files were dropped on changed before their paths were ready.")]
    WslPasteTargetGone,
    // keep-sorted end
}

impl Error {
    pub(crate) fn at_path(self, path: &Path) -> Self {
        Self::Path {
            path: path.to_owned(),
            source: Box::new(self),
        }
    }
}

mod config;
mod daemon;
mod github;
mod theme;

#[cfg(test)]
mod tests;
