//! `herdr_client::Error`: reaching a Herdr daemon locally, over SSH, or in
//! WSL, from `error-client.ftl`, and the client's own sub-errors through
//! their mappings beside this file.

mod clipboard;
mod sessions;
mod ssh;
mod terminal_attach;
mod wsl;

/// A missing home or config root, in the root error's words.
impl Localize for herdr_client::local_path::PathError {
    fn localize(&self) -> String {
        match self {
            Self::MissingHome => fl!("error-missing-home"),
            Self::RelativeConfigRoot => fl!("error-relative-config-root"),
        }
    }
}

use super::Localize;
use crate::i18n::fl;
use herdr_client::{Error, MIN_HERDR_VERSION, protocol::endpoint::ENDPOINT_PROTOCOL_GENERATION};

/// Who has to update when a daemon's endpoint generation is refused: this
/// app for a newer generation, the daemon otherwise. A Fluent selector key.
fn fix(generation: u32) -> &'static str {
    if generation > ENDPOINT_PROTOCOL_GENERATION {
        "app"
    } else {
        "daemon"
    }
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::BridgeIncompatible {
                generation,
                version: Some(version),
            } => fl!(
                "error-client-bridge-incompatible-version",
                version = version.as_str(),
                fix = fix(generation.unwrap_or(0))
            ),
            Self::BridgeIncompatible {
                generation,
                version: None,
            } => fl!(
                "error-client-bridge-incompatible",
                fix = fix(generation.unwrap_or(0))
            ),
            Self::Cancelled => fl!("error-client-cancelled"),
            Self::CatalogLimit => fl!("error-client-catalog-limit"),
            Self::CatalogNotFile => fl!("error-client-catalog-not-file"),
            Self::CatalogSchema(_) => fl!("error-client-catalog-schema"),
            Self::CatalogVersionOrCount => fl!("error-client-catalog-version-or-count"),
            Self::Clipboard(error) => error.localize(),
            Self::ClosedBeforeWelcome => {
                fl!(
                    "error-client-closed-before-welcome",
                    version = MIN_HERDR_VERSION
                )
            }
            Self::CommandBoot => fl!("error-client-command-boot"),
            Self::Disconnected => fl!("error-client-disconnected"),
            Self::EmptySurface => fl!("error-client-empty-surface"),
            Self::EncodedSurfaceBeforeBaseline => {
                fl!("error-client-encoded-surface-before-baseline")
            }
            // The daemon's own words for a refused request.
            Self::Endpoint { message, .. } => message.clone(),
            Self::EndpointGeneration {
                generation,
                server_version,
            } => fl!(
                "error-client-endpoint-generation",
                server_version = server_version.as_str(),
                generation = *generation,
                ours = ENDPOINT_PROTOCOL_GENERATION,
                fix = fix(*generation)
            ),
            Self::EventReceiverDropped => fl!("error-client-event-receiver-dropped"),
            Self::ExpectedWelcome => fl!("error-client-expected-welcome"),
            Self::ForwardControl(_) => fl!("error-client-forward-control"),
            Self::ForwardExit(status) => {
                fl!("error-client-forward-exit", status = status.to_string())
            }
            Self::ForwardLocalPort(_) => fl!("error-client-forward-local-port"),
            Self::ForwardRefused(status) => {
                fl!("error-client-forward-refused", status = status.to_string())
            }
            Self::ForwardSpawn(_) => fl!("error-client-forward-spawn"),
            Self::ForwardTimeout => fl!("error-client-forward-timeout"),
            Self::FrameLength => fl!("error-client-frame-length"),
            Self::FramePrefix => fl!("error-client-frame-prefix"),
            Self::Full => fl!("error-client-full"),
            Self::GeometryLimit => fl!("error-client-geometry-limit"),
            Self::HandshakeTimeout => fl!("error-client-handshake-timeout"),
            Self::HealthTimeout => fl!("error-client-health-timeout"),
            Self::IncompatibleCodecs => fl!("error-client-incompatible-codecs"),
            Self::InputWriteTimeout => fl!("error-client-input-write-timeout"),
            Self::InvalidGitDir => fl!("error-client-invalid-git-dir"),
            Self::InvalidSession => fl!("error-client-invalid-session"),
            Self::Io(error) => fl!("error-client-io", error = error.to_string()),
            Self::Json(error) => fl!("error-client-json", error = error.to_string()),
            Self::LegacyDaemon => fl!("error-client-legacy-daemon"),
            Self::MissingBootId => fl!("error-client-missing-boot-id"),
            Self::MissingHealthCheck { server_version } => fl!(
                "error-client-missing-health-check",
                server_version = server_version.as_str()
            ),
            Self::MissingSurfaceInterest { server_version } => fl!(
                "error-client-missing-surface-interest",
                server_version = server_version.as_str()
            ),
            Self::NoLocalSocket => fl!("error-client-no-local-socket"),
            Self::PartialFrameTimeout => fl!("error-client-partial-frame-timeout"),
            Self::PatchBeforeBaseline => fl!("error-client-patch-before-baseline"),
            Self::ProfileId => fl!("error-client-profile-id"),
            Self::ProfileLabel => fl!("error-client-profile-label"),
            Self::Protocol(error) => error.localize(),
            Self::RemoteCommand(status) => {
                fl!("error-client-remote-command", status = status.to_string())
            }
            Self::RemoteOutput => fl!("error-client-remote-output"),
            Self::RequestTimeout => fl!("error-client-request-timeout"),
            Self::ResponseBoot => fl!("error-client-response-boot"),
            Self::ResponseId => fl!("error-client-response-id"),
            Self::ResponseLimit => fl!("error-client-response-limit"),
            Self::ResponseMissingResult => fl!("error-client-response-missing-result"),
            Self::ResponseSchema(_) => fl!("error-client-response-schema"),
            Self::ResponseType => fl!("error-client-response-type"),
            Self::ScriptCancelled => fl!("error-client-script-cancelled"),
            Self::ScriptConnect(error) => {
                fl!("error-client-script-connect", error = error.to_string())
            }
            Self::ScriptExit { status, stderr } => fl!(
                "error-client-script-exit",
                status = status.to_string(),
                stderr = stderr.as_str()
            ),
            Self::ScriptInput(_) => fl!("error-client-script-input"),
            Self::ScriptIo(_) => fl!("error-client-script-io"),
            Self::ScriptOutput(_) => fl!("error-client-script-output"),
            Self::ScriptOutputLimit => fl!("error-client-script-output-limit"),
            Self::ScriptSpawn(_) => fl!("error-client-script-spawn"),
            Self::ScriptTimeout => fl!("error-client-script-timeout"),
            Self::ScriptUnsupported => fl!("error-client-script-unsupported"),
            Self::ScriptWorker => fl!("error-client-script-worker"),
            Self::SelectionLimit => fl!("error-client-selection-limit"),
            Self::SelectionNotFile => fl!("error-client-selection-not-file"),
            Self::SelectionSchema(_) => fl!("error-client-selection-schema"),
            Self::SelectionUnavailable => fl!("error-client-selection-unavailable"),
            Self::SelectionVersion => fl!("error-client-selection-version"),
            // The daemon's own words for why it is shutting down.
            Self::ServerShutdown(message) => message.clone(),
            Self::Sessions(error) => error.localize(),
            Self::SnapshotIdentity => fl!("error-client-snapshot-identity"),
            Self::SocketClosed => fl!("error-client-socket-closed"),
            Self::Ssh(error) => error.localize(),
            // Its path is for diagnostics; the message is the failure's own.
            Self::Storage { source, .. } => source.localize(),
            Self::StorageDestinationNotFile => fl!("error-client-storage-destination-not-file"),
            Self::StoragePath => fl!("error-client-storage-path"),
            Self::SurfaceBeforeSnapshot => fl!("error-client-surface-before-snapshot"),
            Self::SurfaceEncodingNotNegotiated => {
                fl!("error-client-surface-encoding-not-negotiated")
            }
            Self::SurfaceIdentity => fl!("error-client-surface-identity"),
            Self::TerminalAttach(error) => error.localize(),
            Self::UnsolicitedResponse => fl!("error-client-unsolicited-response"),
            Self::UnsupportedMethod => fl!("error-client-unsupported-method"),
            Self::UnsupportedSurfaceInterest => fl!("error-client-unsupported-surface-interest"),
            Self::UploadCancelled => fl!("error-client-upload-cancelled"),
            Self::UploadCleanup { source, .. } => {
                fl!("error-client-upload-cleanup", error = source.localize())
            }
            Self::UploadCleanupPath => fl!("error-client-upload-cleanup-path"),
            Self::UploadExit { .. } => fl!("error-client-upload-exit"),
            Self::UploadIo(_) => fl!("error-client-upload-io"),
            Self::UploadName => fl!("error-client-upload-name"),
            Self::UploadNotFile => fl!("error-client-upload-not-file"),
            Self::UploadPathLimit => fl!("error-client-upload-path-limit"),
            Self::UploadResponse => fl!("error-client-upload-response"),
            Self::UploadSizeOverflow => fl!("error-client-upload-size-overflow"),
            Self::UploadSourceChanged => fl!("error-client-upload-source-changed"),
            Self::UploadTimeout => fl!("error-client-upload-timeout"),
            Self::UploadUnsupported => fl!("error-client-upload-unsupported"),
            Self::WelcomeKind => fl!("error-client-welcome-kind"),
            Self::WelcomeRejected { code, message } => fl!(
                "error-client-welcome-rejected",
                code = code.as_str(),
                message = message.as_str()
            ),
            Self::Wsl(error) => error.localize(),
            // keep-sorted end
        }
    }
}
