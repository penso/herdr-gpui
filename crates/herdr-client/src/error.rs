use std::{
    io,
    path::{Path, PathBuf},
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The storage step that failed. Replacement retains both source and destination paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageOperation {
    Open,
    Metadata,
    Read,
    Decode,
    Encode,
    Validate,
    CreateDirectory,
    Create,
    Write,
    Sync,
    Replace { destination: PathBuf },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Paths are available to diagnostic callers but omitted from presentation text.
    #[error("{source}")]
    Storage {
        operation: StorageOperation,
        path: PathBuf,
        #[source]
        source: Box<Error>,
    },
    #[error("client I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Protocol(#[from] herdr_protocol::Error),
    #[error("invalid endpoint JSON: {0}")]
    Json(#[from] serde_json::Error),
    // Schema details may contain secret-bearing unknown field names. Keep the
    // source for diagnostics, but never include it in presentation text.
    #[error("invalid endpoint catalog schema")]
    CatalogSchema(#[source] serde_json::Error),
    #[error("invalid endpoint selection schema")]
    SelectionSchema(#[source] serde_json::Error),
    #[error("client command queue is full")]
    Full,
    #[error("client is disconnected")]
    Disconnected,
    #[error(transparent)]
    Clipboard(#[from] crate::clipboard::Error),
    #[error("terminal input write timed out; reconnect required")]
    InputWriteTimeout,
    #[error("invalid client command: snapshot boot ID required")]
    MissingBootId,
    #[error("command does not match a ready snapshot boot")]
    CommandBoot,
    #[error("method not advertised by endpoint")]
    UnsupportedMethod,
    #[error("surface interest capabilities not advertised by endpoint")]
    UnsupportedSurfaceInterest,
    #[error("surface dimensions must be nonzero")]
    EmptySurface,
    #[error("surface geometry exceeds endpoint limits")]
    GeometryLimit,
    #[error("invalid session name")]
    InvalidSession,
    #[error("remote endpoints have no local socket path")]
    NoLocalSocket,
    #[error("client stopped")]
    Cancelled,
    #[error("event receiver dropped")]
    EventReceiverDropped,
    #[error("socket closed")]
    SocketClosed,
    #[error("partial frame timed out")]
    PartialFrameTimeout,
    #[error("invalid frame prefix")]
    FramePrefix,
    #[error("invalid frame length")]
    FrameLength,
    #[error("endpoint health check timed out")]
    HealthTimeout,
    #[error("handshake/snapshot timed out")]
    HandshakeTimeout,
    #[error("endpoint request timed out; not replayed")]
    RequestTimeout,
    #[error("expected stable endpoint welcome")]
    ExpectedWelcome,
    /// The daemon answered the endpoint hello with a pre-endpoint welcome.
    #[error("this Herdr server predates the endpoint protocol; run `herdr update` and reconnect")]
    LegacyDaemon,
    /// A local daemon closed after the hello without any welcome, as every
    /// release before the endpoint protocol does.
    #[error(
        "Herdr server closed the connection without answering; it is likely older than {}, which this app requires. Run `herdr update` and reconnect",
        crate::compat::MIN_HERDR_VERSION
    )]
    ClosedBeforeWelcome,
    #[error(
        "Herdr server {server_version} speaks endpoint generation {generation}, this app speaks {ours}; {advice}",
        ours = herdr_protocol::endpoint::ENDPOINT_PROTOCOL_GENERATION,
        advice = crate::compat::generation_advice(*.generation)
    )]
    EndpointGeneration {
        generation: u32,
        server_version: String,
    },
    #[error("expected endpoint.welcome.v1")]
    WelcomeKind,
    #[error("{code}: {message}")]
    WelcomeRejected { code: String, message: String },
    #[error("incompatible endpoint generation/codecs")]
    IncompatibleCodecs,
    #[error(
        "Herdr server {server_version} lacks safe surface interest support; run `herdr update` and reconnect"
    )]
    MissingSurfaceInterest { server_version: String },
    #[error(
        "Herdr server {server_version} on this SSH host lacks the health_check capability; run `herdr update` there and reconnect"
    )]
    MissingHealthCheck { server_version: String },
    #[error("endpoint boot changed or snapshot revision regressed; reconnect required")]
    SnapshotIdentity,
    #[error("surface before snapshot")]
    SurfaceBeforeSnapshot,
    #[error("invalid surface identity/revision")]
    SurfaceIdentity,
    #[error("patch before baseline")]
    PatchBeforeBaseline,
    #[error("encoded surface before baseline")]
    EncodedSurfaceBeforeBaseline,
    #[error("surface encoding was not advertised by endpoint")]
    SurfaceEncodingNotNegotiated,
    #[error("response boot mismatch")]
    ResponseBoot,
    #[error("response limit exceeded")]
    ResponseLimit,
    #[error("unsolicited response")]
    UnsolicitedResponse,
    #[error("response ID mismatch")]
    ResponseId,
    #[error("{0}")]
    ServerShutdown(String),
    #[error(transparent)]
    Sessions(#[from] crate::sessions::Error),
    #[error(transparent)]
    Ssh(#[from] crate::ssh::Error),
    #[error(transparent)]
    Wsl(#[from] crate::wsl::Error),
    /// Every Herdr installed on the SSH host or WSL distribution was skipped
    /// as unable to serve this client. Fields describe the first one;
    /// `version` is bounded text.
    #[error(
        "Herdr{} on this host cannot serve this app; {}",
        crate::compat::version_note(.version.as_deref()),
        crate::compat::generation_advice(.generation.unwrap_or(0))
    )]
    BridgeIncompatible {
        generation: Option<u32>,
        version: Option<String>,
    },
    #[error("remote repository directory must be an absolute path")]
    InvalidGitDir,
    #[error("remote command failed ({0})")]
    RemoteCommand(std::process::ExitStatus),
    #[error("remote command returned unexpected output")]
    RemoteOutput,
    #[error("SSH file transfer requires a Linux or macOS client")]
    UploadUnsupported,
    #[error("SSH file transfer accepts at most 256 paths")]
    UploadPathLimit,
    #[error("file transfer requires a UTF-8 basename without controls")]
    UploadName,
    #[error("file transfer source is not a regular file")]
    UploadNotFile,
    #[error("file transfer total size exceeds u64")]
    UploadSizeOverflow,
    #[error("file transfer source changed length")]
    UploadSourceChanged,
    #[error("SSH file transfer cancelled")]
    UploadCancelled,
    #[error("SSH file transfer made no progress for 30 seconds")]
    UploadTimeout,
    #[error("invalid or excessive SSH file transfer response")]
    UploadResponse,
    #[error("cleanup requires unchanged absolute paths returned by upload_files")]
    UploadCleanupPath,
    #[error("SSH file transfer failed; check host trust, authentication, and remote storage")]
    UploadExit { status: std::process::ExitStatus },
    #[error("SSH file transfer I/O failed")]
    UploadIo(#[source] io::Error),
    #[error("{source}; remote temporary-file cleanup also failed")]
    UploadCleanup {
        #[source]
        source: Box<Error>,
        cleanup: Box<Error>,
    },
    #[error("host scripts require a Linux or macOS client")]
    ScriptUnsupported,
    #[error("could not start host script")]
    ScriptSpawn(#[source] io::Error),
    #[error("could not reach host for script: {0}")]
    ScriptConnect(#[source] io::Error),
    #[error("host script I/O failed")]
    ScriptIo(#[source] io::Error),
    #[error("could not read host script input")]
    ScriptInput(#[source] io::Error),
    #[error("could not write host script output")]
    ScriptOutput(#[source] io::Error),
    #[error("host script output exceeds limit")]
    ScriptOutputLimit,
    #[error("host script cancelled")]
    ScriptCancelled,
    #[error("host script made no progress before its deadline")]
    ScriptTimeout,
    #[error("host script worker panicked")]
    ScriptWorker,
    /// `stderr` is a bounded, control-free tail kept for diagnostics.
    #[error("host script failed ({status}): {stderr}")]
    ScriptExit {
        status: std::process::ExitStatus,
        stderr: String,
    },
    #[error("could not start SSH for the port forward")]
    ForwardSpawn(#[source] io::Error),
    #[error("no local port is free to forward to")]
    ForwardLocalPort(#[source] io::Error),
    #[error("could not create the port forward's private control directory")]
    ForwardControl(#[source] io::Error),
    #[error("SSH did not connect in time; check host trust and authentication")]
    ForwardTimeout,
    #[error("SSH could not listen on a local port ({0})")]
    ForwardRefused(std::process::ExitStatus),
    /// Stderr is discarded, as for every SSH child: it can carry banners or secrets.
    #[error("SSH port forward ended ({0}); check host trust, authentication, and the network")]
    ForwardExit(std::process::ExitStatus),
    #[error("endpoint selection is not a regular file")]
    SelectionNotFile,
    #[error("endpoint selection exceeds storage limit")]
    SelectionLimit,
    #[error("unsupported endpoint selection version")]
    SelectionVersion,
    #[error("selected endpoint is absent or disabled")]
    SelectionUnavailable,
    #[error("invalid storage path")]
    StoragePath,
    #[error("storage destination is not a regular file")]
    StorageDestinationNotFile,
    #[error("endpoint catalog is not a regular file")]
    CatalogNotFile,
    #[error("endpoint catalog exceeds storage limit")]
    CatalogLimit,
    #[error("unsupported catalog version or too many profiles")]
    CatalogVersionOrCount,
    #[error("invalid or duplicate endpoint profile id")]
    ProfileId,
    #[error("invalid endpoint label")]
    ProfileLabel,
    /// An endpoint answered a request with an error. The message is the
    /// daemon's own display text; act on `code`, never on the message.
    #[error("{message}")]
    Endpoint {
        code: crate::scrollback::EndpointErrorCode,
        message: String,
    },
    #[error("invalid endpoint response")]
    ResponseSchema(#[source] serde_json::Error),
    #[error("endpoint response carried neither a result nor an error")]
    ResponseMissingResult,
    #[error("endpoint answered with a result of another method")]
    ResponseType,
}

impl Error {
    pub(crate) fn storage(
        operation: StorageOperation,
        path: &Path,
        source: impl Into<Error>,
    ) -> Self {
        Self::Storage {
            operation,
            path: path.to_owned(),
            source: Box::new(source.into()),
        }
    }

    /// Whether a host script's `ssh` was dropped before key exchange, as sshd
    /// drops connections past its `MaxStartups` while many are still
    /// authenticating. Nothing ran on the host, so the script may run again
    /// once the host has caught up. `ssh` exits 255 for its own failures, and
    /// OpenSSH before 8.0 printed `ssh_exchange_identification` instead.
    pub fn dropped_before_key_exchange(&self) -> bool {
        const MARKERS: [&str; 2] = [
            "kex_exchange_identification:",
            "ssh_exchange_identification:",
        ];
        matches!(
            self,
            Self::ScriptExit { status, stderr }
                if status.code() == Some(255)
                    && MARKERS.iter().any(|marker| stderr.contains(marker))
        )
    }

    /// Whether a host script failed for want of its host, not on it: the
    /// connection could not be made, went quiet past its deadline, or `ssh`
    /// itself failed, which it reports by exiting 255. What the host last
    /// answered may still hold, so a caller can keep it and ask again later.
    pub fn host_unreached(&self) -> bool {
        match self {
            Self::ScriptConnect(_) | Self::ScriptTimeout => true,
            Self::ScriptExit { status, .. } => status.code() == Some(255),
            _ => false,
        }
    }

    /// Preserve transport retry/cancellation categories, including the historical
    /// InvalidData category for session deadlines and validation failures.
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Storage { source, .. } => source.kind(),
            Self::Io(error) | Self::ScriptConnect(error) => error.kind(),
            Self::UploadIo(error) => error.kind(),
            Self::UploadCleanup { source, .. } => source.kind(),
            Self::UploadUnsupported => io::ErrorKind::Unsupported,
            Self::UploadCancelled => io::ErrorKind::Interrupted,
            Self::UploadTimeout => io::ErrorKind::TimedOut,
            Self::UploadPathLimit | Self::UploadName | Self::UploadCleanupPath => {
                io::ErrorKind::InvalidInput
            }
            Self::Protocol(error) => error.kind(),
            Self::Json(error) => error.io_error_kind().unwrap_or(if error.is_eof() {
                io::ErrorKind::UnexpectedEof
            } else {
                io::ErrorKind::InvalidData
            }),
            Self::InvalidSession | Self::NoLocalSocket => io::ErrorKind::InvalidInput,
            Self::Clipboard(error) => error.kind(),
            Self::Cancelled => io::ErrorKind::Interrupted,
            Self::EventReceiverDropped | Self::Disconnected => io::ErrorKind::BrokenPipe,
            Self::SocketClosed | Self::ClosedBeforeWelcome => io::ErrorKind::UnexpectedEof,
            Self::Sessions(error) => error.kind(),
            Self::Ssh(error) => error.kind(),
            Self::Wsl(error) => error.kind(),
            Self::ForwardSpawn(error)
            | Self::ForwardLocalPort(error)
            | Self::ForwardControl(error) => error.kind(),
            Self::HealthTimeout | Self::ForwardTimeout => io::ErrorKind::TimedOut,
            Self::Full => io::ErrorKind::WouldBlock,
            _ => io::ErrorKind::InvalidData,
        }
    }
}
