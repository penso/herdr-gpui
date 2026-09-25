//! Internal failures retain their categories and sources until presentation.
pub use crate::updater::UpdateError;
use std::{
    io,
    path::{Path, PathBuf},
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn daemon_error_message(error: &serde_json::Value) -> &str {
    error
        .get("message")
        .and_then(serde_json::Value::as_str)
        .or_else(|| error.as_str())
        .or_else(|| error.get("code").and_then(serde_json::Value::as_str))
        .filter(|message| !message.is_empty())
        .unwrap_or("Invalid daemon error")
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid saved window geometry or too many saved windows")]
    InvalidWindowState,
    #[error("ui.toast.delay_seconds must be between 0 and 3600")]
    SoundDelay,
    #[error("Sound configuration exceeds 1 MiB")]
    SoundConfigSize,
    #[error("Unknown sidebar layout {0:?}")]
    UnknownLayout(String),
    #[error("Could not open audio output: {0}")]
    SoundDevice(#[from] rodio::DeviceSinkError),
    #[error("Audio output failed: {0}")]
    SoundStream(#[from] rodio::cpal::StreamError),
    #[error("Could not decode MP3 sound: {0}")]
    SoundDecode(#[from] rodio::decoder::DecoderError),
    #[error("Could not read sound file {}: {source}", path.display())]
    SoundFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Sound must be a regular file of at most 16 MiB")]
    SoundFileSize,
    #[error("Audio playback timed out")]
    SoundTimeout,
    #[error("Audio playback cancelled")]
    SoundCancelled,
    #[error(
        "PR lookup requires your owned local session socket. Select Local using its standard socket; SSH and other socket locations are unsupported."
    )]
    PrUntrustedEndpoint,
    #[error("The selected pane is no longer on screen.")]
    SelectionStale,
    #[error("Selection is too large to copy.")]
    SelectionSize,
    #[error("File drop exceeds 256 paths or 64 KiB of quoted text.")]
    FileDropSize,
    #[error("Dropped paths must be UTF-8.")]
    FileDropEncoding,
    #[error("Dropped paths must not contain control characters.")]
    FileDropControl,
    #[error("Dropped paths must not be empty.")]
    FileDropEmptyPath,
    #[error("Could not {operation} the local image file.")]
    ImageFile {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("The local image must be a regular file.")]
    ImageFileType,
    #[error("The image must not be empty.")]
    ImageSize,
    #[error("The image exceeds Herdr's {limit}-byte upload limit.")]
    ImageTooLarge { limit: usize },
    #[error("The image exceeds the {limit}-byte resize input limit.")]
    ImageInputTooLarge { limit: usize },
    #[error("The image exceeds the resize pixel or decoded-memory limit.")]
    ImageDecodeLimit,
    #[error("Oversized GIF, WebP, and animated PNG images cannot be resized safely.")]
    ImageAnimationResize,
    #[error("Could not decode the oversized image.")]
    ImageDecode(#[source] image::ImageError),
    #[error("Could not encode the resized image.")]
    ImageEncode(#[source] image::ImageError),
    #[error("Reading the local image timed out (3 seconds).")]
    ImageReadTimeout,
    #[error("SVG clipboard images are not supported. Use PNG, JPEG, GIF, WebP, BMP, or TIFF.")]
    ImageFormat,
    #[error("Clipboard content exceeds the {limit}-byte limit.")]
    ClipboardSize { limit: usize },
    #[error("Clipboard text is not valid UTF-8.")]
    ClipboardEncoding(#[source] std::str::Utf8Error),
    #[error("The clipboard changed while reading. Try pasting again.")]
    ClipboardChanged,
    #[error("Clipboard acquisition timed out.")]
    ClipboardTimeout,
    #[error("Could not read the clipboard using wl-paste or xclip.")]
    ClipboardProcess(#[source] io::Error),
    #[error("Remote clipboard acquisition is not supported on this platform.")]
    ClipboardUnsupported,
    #[error("Checkout lookup failed. Dismiss and reopen the menu.")]
    DeletionLookup,
    #[error("Reopen the deletion dialog.")]
    MissingDeletion,
    #[error("Invalid or oversized worktree list. Dismiss and reopen the menu.")]
    WorktreeList,
    #[error("Malformed worktree list. Dismiss and reopen the menu.")]
    WorktreeListDecode(#[source] serde_json::Error),
    #[error("Select a checkout from the daemon's worktree list.")]
    WorktreeSelection,
    #[error("{method}: {source}")]
    Request {
        method: herdr_client::Method,
        #[source]
        source: herdr_client::Error,
    },
    #[error("Workspace is no longer available. Dismiss and reopen the menu.")]
    StaleWorkspace,
    #[error("Workspace label must not be empty.")]
    EmptyWorkspaceLabel,
    #[error(
        "Invalid Git branch name. Use a name such as config-reload, without spaces or special ref characters."
    )]
    InvalidBranchName,
    #[error("Workspace group changed. Dismiss and review the group again.")]
    WorkspaceGroupChanged,
    #[error("Repository changed. Dismiss and reopen the menu.")]
    WorkspaceRepositoryChanged,
    #[error("Checkout changed. Dismiss and reopen the menu.")]
    WorkspaceCheckoutChanged,
    #[error("Daemon did not provide an absolute checkout and repository key.")]
    PrAbsolutePath,
    #[error("No supported branch available.")]
    PrBranch,
    #[error("Local repository unavailable for worktree lookup.")]
    PrWorktreeLookup,
    #[error("Local checkout unavailable or not a trusted Git repository.")]
    PrCheckout,
    #[error("Local repository does not match daemon metadata.")]
    PrRepositoryMismatch,
    #[error("Checkout branch changed. Waiting for daemon metadata.")]
    PrBranchChanged,
    #[error("PR lookup supports GitHub.com origins only.")]
    PrOrigin,
    #[error("No local worktree matches the daemon branch.")]
    PrMissingWorktree,
    #[error("Multiple local worktrees match the daemon branch.")]
    PrAmbiguousWorktree,
    #[error("GitHub repository unavailable. Check repository access and token permissions.")]
    PrRepository,
    #[error("PR response exceeded the size limit.")]
    PrSize,
    #[error("Multiple PRs match this branch; no PR selected.")]
    PrAmbiguous,
    #[error("PR identity does not match the repository and branch.")]
    PrIdentity,
    #[error("Could not {operation} for Git PR lookup.")]
    PrProcess {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("Invalid process text.")]
    PrEncoding(#[source] std::str::Utf8Error),
    #[error("No repository metadata.")]
    PrMetadata,
    #[error("Could not {operation} the agent context note for the new checkout.")]
    AgentContext {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error(
        "Pull request #{number} comes from the fork {owner}. Fetch its branch yourself, then create the checkout from the branch field."
    )]
    ForkPullRequest { number: u64, owner: String },
    #[error("git {operation} failed: {details}")]
    GitFailed {
        operation: &'static str,
        details: String,
    },
    #[error("A Git operation is already running.")]
    GitBusy,
    #[error("No local checkout is focused.")]
    GitNoCheckout,
    #[error("Enter a commit message of at most 4096 characters.")]
    GitCommitMessage,
    #[error("Nothing to commit: the checkout has no changes.")]
    GitNothingToCommit,
    #[error("The branch has no commit to describe. Commit before opening a pull request.")]
    GitPullRequestTitle,
    #[error("This branch is the repository default branch; no pull request can be opened from it.")]
    GitPullRequestBase,
    #[error("Git worker stopped. Retry the operation.")]
    GitWorker,
    #[error("Could not {operation}.")]
    GitProcess {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error(
        "GitHub authentication required. Use menu > GitHub sign-in or set GH_TOKEN / GITHUB_TOKEN."
    )]
    GitHubAuthentication,
    #[error("GitHub denied access: check token permissions, SSO authorization, or rate limits.")]
    GitHubForbidden,
    #[error("GitHub rate limit reached. Retry later.")]
    GitHubRateLimit,
    #[error("GitHub request failed (HTTP {0}). Check network and repository access.")]
    GitHubStatus(u16),
    #[error("GitHub response could not be read within the size/time limit.")]
    GitHubRead(#[source] io::Error),
    #[error("GitHub response exceeded the size limit.")]
    GitHubSize,
    #[error("Invalid GitHub JSON response.")]
    GitHubJson(#[source] GitHubJsonError),
    #[error("Invalid GitHub token. Replace the configured credential.")]
    GitHubToken,
    #[error("Invalid GitHub credential encoding.")]
    GitHubEncoding(#[source] std::str::Utf8Error),
    #[cfg(target_os = "macos")]
    #[error("Cannot read GitHub Keychain entry. Unlock your login Keychain or set GH_TOKEN.")]
    KeychainRead(#[source] security_framework::base::Error),
    #[cfg(target_os = "macos")]
    #[error("GitHub Keychain update failed. Unlock your login Keychain and try again.")]
    KeychainWrite(#[source] security_framework::base::Error),
    #[error("Missing credential directory.")]
    CredentialDirectory,
    #[error(
        "Cannot access private GitHub credential file. Require an owned directory and regular 0600 file; symlinks are rejected."
    )]
    CredentialPermissions,
    #[error("Cannot access private GitHub credential file.")]
    CredentialIo(#[source] io::Error),
    #[error(
        "No secure credential store configured. Explicitly opt in with [github] allow_plaintext_credentials = true, or use GH_TOKEN / GITHUB_TOKEN."
    )]
    CredentialPolicy,
    #[error(
        "No GitHub credential store is available on this platform. Use GH_TOKEN / GITHUB_TOKEN."
    )]
    CredentialUnsupported,
    #[error("PR lookup cancelled.")]
    PrCancelled,
    #[error("PR lookup timed out (15 seconds).")]
    PrTimeout,
    #[error("GitHub network request failed or timed out.")]
    GitHubNetwork(#[source] ureq::Error),
    #[error("GitHub query failed. Check token repository permissions and rate limits.")]
    GitHubQuery,
    #[error("Invalid GitHub authorization header.")]
    GitHubHeader(#[source] ureq::http::header::InvalidHeaderValue),
    #[error("Invalid GitHub device authorization response.")]
    GitHubDevice,
    #[error("GitHub code expired. Sign in again.")]
    GitHubExpired,
    #[error("GitHub authorization denied.")]
    GitHubDenied,
    #[error("GitHub authorization failed. Check OAuth application settings.")]
    GitHubAuthorization,
    #[error("Unsupported GitHub token type.")]
    GitHubTokenType,
    #[error("GitHub {0} worker stopped.")]
    GitHubWorker(&'static str),
    #[error("{0}")]
    Config(#[source] config_loader::ConfigError),
    #[error("{source}")]
    ConfigFile {
        uri: Option<String>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("HERDR_GITHUB_OAUTH_CLIENT_ID must be UTF-8")]
    ClientIdEncoding,
    #[error(
        "{0} must be 1..256 ASCII letters, digits, '.', '_' or '-' (public client ID, not a secret)"
    )]
    InvalidClientId(&'static str),
    #[error("{0}")]
    Update(#[from] UpdateError),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    TomlEdit(#[from] toml_edit::TomlError),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Client(#[from] herdr_client::Error),
    #[error("neither XDG_STATE_HOME nor HOME is set")]
    MissingStateRoot,
    #[error("{0}")]
    DeviceSetupInput(&'static str),
    #[error("Could not open the setup terminal (exit status {0})")]
    DeviceSetupTerminal(std::process::ExitStatus),
    #[error("Opening the setup terminal timed out")]
    DeviceSetupTimeout,
    #[error("preferences must be an object")]
    PreferencesNotObject,
    #[error("sidebar_width_px must be finite and positive, or null")]
    InvalidStoredWidth,
    #[error("invalid sidebar width")]
    InvalidSidebarWidth,
    #[error("preferences path has no parent")]
    PreferencesPath,
    #[error("{}: {source}", path.display())]
    Path {
        path: PathBuf,
        #[source]
        source: Box<Error>,
    },
    #[error("{source}; removing {}: {cleanup}", path.display())]
    Cleanup {
        #[source]
        source: io::Error,
        path: PathBuf,
        cleanup: io::Error,
    },
    #[error("HOME is not set")]
    MissingHome,
    #[error("XDG_CONFIG_HOME must be an absolute path")]
    RelativeConfigRoot,
    #[error("Cannot migrate {}: {} already contains different settings. Merge your old settings into the local file, then remove the old config; it will be regenerated.", original.display(), local.display())]
    ConfigMigrationConflict { original: PathBuf, local: PathBuf },
    #[error("theme must not be empty")]
    EmptyTheme,
    #[error("{0}.family must not be empty")]
    EmptyFontFamily(&'static str),
    #[error("{0}.size must be finite and between 8 and 48 logical pixels")]
    InvalidFontSize(&'static str),
    #[error("{0}.fallback families must not be empty")]
    EmptyFontFallback(&'static str),
    #[error("{0}.fallback must list at most 8 families")]
    TooManyFontFallbacks(&'static str),
    #[error("layout.sidebar_gap must be finite and between 0 and 64 logical pixels")]
    InvalidSidebarGap,
    #[error("theme must be a name, absolute path, or ~/ path")]
    InvalidThemePath,
    #[error("keybindings.{0} is not a command; see the keybindings list in config-gpui.toml")]
    UnknownKeybinding(String),
    #[error("keybindings.{command}: invalid keystroke {keystroke:?}")]
    InvalidKeystroke {
        command: &'static str,
        keystroke: String,
        #[source]
        source: gpui_kit::InvalidKeystrokeError,
    },
    #[error(
        "keybindings.{command}: {keystroke:?} needs a cmd, ctrl, alt, or fn modifier so typing still reaches the terminal"
    )]
    KeystrokeWithoutModifier {
        command: &'static str,
        keystroke: String,
    },
    #[error("keybindings.{0} must list at most 8 keystrokes")]
    TooManyKeystrokes(&'static str),
    #[error("keybindings: {keystroke:?} is bound to both {first} and {second}")]
    DuplicateKeystroke {
        keystroke: String,
        first: &'static str,
        second: &'static str,
    },
    #[error("theme {name:?} not found in {directories:?}")]
    ThemeNotFound {
        name: String,
        directories: Vec<PathBuf>,
    },
    #[error("line {line}: {key}: {source}")]
    ThemeLine {
        line: usize,
        key: String,
        #[source]
        source: ThemeParseError,
    },
    #[error("The original target changed or no longer exists. Cancel and try again.")]
    StaleCloseTarget,
    #[error("The selected connection changed or is not ready. Cancel and try again.")]
    StaleConnection,
    #[error("Not connected to a daemon.")]
    NotConnected,
    #[error("The original tab changed or no longer exists. Cancel and try again.")]
    StaleTab,
    #[error("The original pane changed or no longer exists. Cancel and try again.")]
    StalePane,
    #[error("Enter a tab name.")]
    EmptyTabName,
    #[error("No tab selected.")]
    NoTab,
    #[error("The connection is not ready. Try again.")]
    ConnectionNotReady,
    #[error("Connection is busy. Try again.")]
    ConnectionBusy,
    #[error("The daemon session changed. Reopen the palette.")]
    PaletteSessionChanged,
    #[error("This workspace no longer exists. Reopen the palette.")]
    PaletteWorkspaceRemoved,
    #[error("This command action is not supported by this client.")]
    UnsupportedCommand,
    #[error("This command changed or was removed. Reopen the palette.")]
    PaletteCommandChanged,
    #[error("The original tab no longer exists in its workspace. Reopen the palette.")]
    PaletteTabRemoved,
    #[error("The original pane no longer exists in its tab. Reopen the palette.")]
    PalettePaneRemoved,
    #[error("This agent or terminal no longer exists. Reopen the palette.")]
    PaletteDestinationRemoved,
    #[error("This host is no longer connected. Reopen the palette.")]
    PaletteHostUnavailable,
    #[error("The selected connection is not ready.")]
    PaletteConnectionNotReady,
    #[error("No current daemon snapshot.")]
    NoSnapshot,
    #[error("No captured daemon session. Reopen the palette.")]
    NoPaletteSession,
    #[error("{}", daemon_error_message(.0))]
    DaemonResponse(serde_json::Value),
    #[error("Could not start herdr server: {source}. Use Terminal > Reconnect to retry.")]
    DaemonSpawn {
        #[source]
        source: io::Error,
    },
    #[error("daemon startup cancelled")]
    DaemonCancelled,
    #[error("Timed out waiting for herdr server at {}. Check the Herdr server log and use Terminal > Reconnect.", .0.display())]
    DaemonTimeout(PathBuf),
    #[error("herdr server exited before accepting connections: {0}. Check the Herdr server log.")]
    DaemonExited(std::process::ExitStatus),
}

impl Error {
    pub(crate) fn github_json(source: serde_json::Error) -> Self {
        Self::GitHubJson(GitHubJsonError(source))
    }

    pub(crate) fn at_path(self, path: &Path) -> Self {
        Self::Path {
            path: path.to_owned(),
            source: Box::new(self),
        }
    }
}

/// Parser diagnostics can quote credential-bearing fields; expose the cause only
/// to explicit source inspection, never ordinary Display or Debug formatting.
#[derive(thiserror::Error)]
#[error("Invalid GitHub JSON response.")]
pub struct GitHubJsonError(#[source] serde_json::Error);

impl std::fmt::Debug for GitHubJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitHubJsonError([REDACTED])")
    }
}

impl From<config_loader::ConfigError> for Error {
    fn from(error: config_loader::ConfigError) -> Self {
        // config preserves this cause but does not expose it through Error::source.
        match error {
            config_loader::ConfigError::FileParse { uri, cause } => {
                Self::ConfigFile { uri, source: cause }
            }
            error => Self::Config(error),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeParseError {
    #[error("expected a six-digit RGB hex color (optionally prefixed by #)")]
    InvalidColor,
    #[error("invalid hex color")]
    InvalidHex(#[source] std::num::ParseIntError),
    #[error("expected index=color")]
    MissingPaletteColor,
    #[error("palette index must be between 0 and 255")]
    InvalidPaletteIndex(#[source] std::num::ParseIntError),
    #[error("palette index must be between 0 and 255")]
    PaletteIndexOutOfRange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn updater_wrapper_preserves_source_chain() {
        let error = Error::from(UpdateError::Io(io::Error::from(io::ErrorKind::BrokenPipe)));
        assert!(matches!(error, Error::Update(_)));
        assert_eq!(
            error
                .source()
                .and_then(|source| source.source())
                .and_then(|source| source.downcast_ref::<io::Error>())
                .map(io::Error::kind),
            Some(io::ErrorKind::BrokenPipe)
        );
    }

    #[test]
    fn github_failures_keep_typed_sources_and_redact_parser_diagnostics() -> anyhow::Result<()> {
        let source = serde_json::from_str::<u64>("\"fixture-private-value\"")
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected JSON type error"))?;
        let error = Error::github_json(source);
        assert!(matches!(error, Error::GitHubJson(_)));
        assert!(!format!("{error} {error:?}").contains("fixture-private-value"));
        assert!(
            error
                .source()
                .and_then(|source| source.source())
                .is_some_and(|source| source.is::<serde_json::Error>())
        );

        let error = Error::CredentialIo(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<io::Error>())
                .is_some_and(|source| source.kind() == io::ErrorKind::PermissionDenied)
        );
        let error = Error::GitHubRead(io::Error::from(io::ErrorKind::TimedOut));
        assert!(
            matches!(&error, Error::GitHubRead(source) if source.kind() == io::ErrorKind::TimedOut)
        );
        Ok(())
    }

    #[test]
    fn io_context_retains_source_kind_and_cleanup_failure() {
        let error = Error::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            .at_path(Path::new("config.toml"));
        assert_eq!(error.to_string(), "config.toml: denied");
        assert!(
            error
                .source()
                .and_then(|source| source.source())
                .and_then(|source| source.downcast_ref::<io::Error>())
                .is_some_and(|source| source.kind() == io::ErrorKind::PermissionDenied)
        );

        let error = Error::Cleanup {
            source: io::Error::other("write failed"),
            path: "temporary".into(),
            cleanup: io::Error::new(io::ErrorKind::PermissionDenied, "cleanup denied"),
        };
        assert_eq!(
            error.to_string(),
            "write failed; removing temporary: cleanup denied"
        );
        assert!(
            error
                .source()
                .is_some_and(|source| source.is::<io::Error>())
        );
        assert!(
            matches!(error, Error::Cleanup { cleanup, .. } if cleanup.kind() == io::ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn client_schema_presentation_remains_redacted() -> anyhow::Result<()> {
        let source = serde_json::from_str::<Vec<String>>("{}")
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected schema error"))?;
        let error = Error::from(herdr_client::Error::CatalogSchema(source));
        assert_eq!(error.to_string(), "invalid endpoint catalog schema");
        assert!(
            error
                .source()
                .and_then(|source| source.source())
                .is_some_and(|source| source.is::<serde_json::Error>())
        );
        Ok(())
    }
}
