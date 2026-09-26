use std::{
    io,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Error>;

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
    #[error("a clipboard image upload is already reserved or in progress")]
    ClipboardImageBusy,
    #[error("clipboard image upload cancelled")]
    ClipboardImageCancelled,
    #[error("clipboard image write timed out; reconnect required")]
    ClipboardImageWriteTimeout,
    #[error("clipboard image uploads require a transport with bounded writes")]
    ClipboardImageUnsupported,
    #[error("semantic input fallback requires a pane or popup target")]
    ClipboardImageInputTarget,
    #[error("clipboard image preparation timed out; reservation skipped")]
    ClipboardImagePreparationTimeout,
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
    #[error("The default session cannot be deleted")]
    DefaultSession,
    #[error("Session deletion timed out; refresh the list before trying again")]
    SessionDeleteTimeout,
    #[error(
        "Herdr refused session deletion ({0}); the session may be running, inaccessible, or unsupported by the installed CLI"
    )]
    SessionDeleteFailed(std::process::ExitStatus),
    #[error("SSH has no local socket path")]
    NoLocalSocket,
    #[error("invalid SSH target (options, controls, and passwords are forbidden)")]
    InvalidSshTarget,
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
    #[error("expected endpoint.welcome.v1")]
    WelcomeKind,
    #[error("{code}: {message}")]
    WelcomeRejected { code: String, message: String },
    #[error("incompatible endpoint generation/codecs")]
    IncompatibleCodecs,
    #[error("endpoint lacks safe surface interest support")]
    MissingSurfaceInterest,
    #[error("SSH endpoint lacks health_check capability")]
    MissingHealthCheck,
    #[error("endpoint boot changed or snapshot revision regressed; reconnect required")]
    SnapshotIdentity,
    #[error("surface before snapshot")]
    SurfaceBeforeSnapshot,
    #[error("invalid surface identity/revision")]
    SurfaceIdentity,
    #[error("patch before baseline")]
    PatchBeforeBaseline,
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
    #[error("SSH endpoints are not supported on this platform")]
    SshUnsupported,
    #[error("SSH connection cancelled")]
    SshCancelled,
    #[error("SSH discovery timed out")]
    SshTimeout,
    #[error("SSH bridge closed; check host trust, authentication, and remote Herdr installation")]
    SshClosed,
    #[error("SSH startup output exceeds limit")]
    SshOutputLimit,
    #[error("remote Git directory must be an absolute path")]
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
    #[error("endpoint selection is not a regular file")]
    SelectionNotFile,
    #[error("endpoint selection exceeds storage limit")]
    SelectionLimit,
    #[error("unsupported endpoint selection version")]
    SelectionVersion,
    #[error("selected endpoint is absent or disabled")]
    SelectionUnavailable,
    #[error("invalid selection path")]
    SelectionPath,
    #[error("selection path is not a regular file")]
    SelectionDestinationNotFile,
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
    #[error("too many sessions to list")]
    SessionLimit,
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

    /// Preserve transport retry/cancellation categories, including the historical
    /// InvalidData category for session deadlines and validation failures.
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            Self::Storage { source, .. } => source.kind(),
            Self::Io(error) => error.kind(),
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
            Self::InvalidSession
            | Self::DefaultSession
            | Self::NoLocalSocket
            | Self::InvalidSshTarget => io::ErrorKind::InvalidInput,
            Self::SshUnsupported | Self::ClipboardImageUnsupported => io::ErrorKind::Unsupported,
            Self::ClipboardImageCancelled => io::ErrorKind::Interrupted,
            Self::ClipboardImageWriteTimeout | Self::ClipboardImagePreparationTimeout => {
                io::ErrorKind::TimedOut
            }
            Self::Cancelled | Self::SshCancelled => io::ErrorKind::Interrupted,
            Self::EventReceiverDropped | Self::Disconnected => io::ErrorKind::BrokenPipe,
            Self::SocketClosed | Self::SshClosed => io::ErrorKind::UnexpectedEof,
            Self::HealthTimeout | Self::SshTimeout | Self::SessionDeleteTimeout => {
                io::ErrorKind::TimedOut
            }
            Self::Full | Self::ClipboardImageBusy => io::ErrorKind::WouldBlock,
            _ => io::ErrorKind::InvalidData,
        }
    }
}
