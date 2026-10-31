use super::*;
use herdr_client::{
    Error, SshFailure, clipboard, protocol::endpoint::ENDPOINT_PROTOCOL_GENERATION,
    scrollback::EndpointErrorCode, sessions, ssh, wsl,
};
use std::{io, path::PathBuf, process::ExitStatus};

fn io() -> io::Error {
    io::Error::other("disk on fire")
}

fn json() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

/// A process that exited with status 1, in the system's words.
fn status() -> ExitStatus {
    #[cfg(unix)]
    let status = std::os::unix::process::ExitStatusExt::from_raw(256);
    #[cfg(windows)]
    let status = std::os::windows::process::ExitStatusExt::from_raw(1);
    status
}

fn boxed(error: Error) -> Box<Error> {
    Box::new(error)
}

#[test]
fn every_client_variant_has_a_message() {
    let newer = ENDPOINT_PROTOCOL_GENERATION + 1;
    assert_localized(&[
        Error::BridgeIncompatible {
            generation: Some(newer),
            version: Some("0.9.4".into()),
        },
        Error::BridgeIncompatible {
            generation: None,
            version: Some("0.9.4".into()),
        },
        Error::BridgeIncompatible {
            generation: Some(newer),
            version: None,
        },
        Error::BridgeIncompatible {
            generation: Some(0),
            version: None,
        },
        Error::Cancelled,
        Error::CatalogLimit,
        Error::CatalogNotFile,
        Error::CatalogSchema(json()),
        Error::CatalogVersionOrCount,
        Error::Clipboard(clipboard::Error::Busy),
        Error::ClosedBeforeWelcome,
        Error::CommandBoot,
        Error::Disconnected,
        Error::EmptySurface,
        Error::EncodedSurfaceBeforeBaseline,
        Error::EndpointGeneration {
            generation: newer,
            server_version: "0.12.0".into(),
        },
        Error::EndpointGeneration {
            generation: 0,
            server_version: "0.8.0".into(),
        },
        Error::EventReceiverDropped,
        Error::ExpectedWelcome,
        Error::ForwardControl(io()),
        Error::ForwardExit(status()),
        Error::ForwardLocalPort(io()),
        Error::ForwardRefused(status()),
        Error::ForwardSpawn(io()),
        Error::ForwardTimeout,
        Error::FrameLength,
        Error::FramePrefix,
        Error::Full,
        Error::GeometryLimit,
        Error::HandshakeTimeout,
        Error::HealthTimeout,
        Error::IncompatibleCodecs,
        Error::InputWriteTimeout,
        Error::InvalidGitDir,
        Error::InvalidSession,
        Error::Io(io()),
        Error::Json(json()),
        Error::LegacyDaemon,
        Error::MissingBootId,
        Error::MissingHealthCheck {
            server_version: "0.9.1".into(),
        },
        Error::MissingSurfaceInterest {
            server_version: "0.9.1".into(),
        },
        Error::NoLocalSocket,
        Error::PartialFrameTimeout,
        Error::PatchBeforeBaseline,
        Error::ProfileId,
        Error::ProfileLabel,
        Error::Protocol(herdr_client::protocol::Error::FrameLimit),
        Error::RemoteCommand(status()),
        Error::RemoteOutput,
        Error::RequestTimeout,
        Error::ResponseBoot,
        Error::ResponseId,
        Error::ResponseLimit,
        Error::ResponseMissingResult,
        Error::ResponseSchema(json()),
        Error::ResponseType,
        Error::ScriptCancelled,
        Error::ScriptConnect(io()),
        Error::ScriptExit {
            status: status(),
            stderr: "permission denied".into(),
        },
        Error::ScriptInput(io()),
        Error::ScriptIo(io()),
        Error::ScriptOutput(io()),
        Error::ScriptOutputLimit,
        Error::ScriptSpawn(io()),
        Error::ScriptTimeout,
        Error::ScriptUnsupported,
        Error::ScriptWorker,
        Error::SelectionLimit,
        Error::SelectionNotFile,
        Error::SelectionSchema(json()),
        Error::SelectionUnavailable,
        Error::SelectionVersion,
        Error::Sessions(sessions::Error::Limit),
        Error::SnapshotIdentity,
        Error::SocketClosed,
        Error::Ssh(ssh::Error::Timeout),
        Error::Storage {
            operation: herdr_client::StorageOperation::Write,
            path: PathBuf::from("/tmp/catalog.json"),
            source: boxed(Error::StoragePath),
        },
        Error::StorageDestinationNotFile,
        Error::StoragePath,
        Error::SurfaceBeforeSnapshot,
        Error::SurfaceEncodingNotNegotiated,
        Error::SurfaceIdentity,
        Error::UnsolicitedResponse,
        Error::UnsupportedMethod,
        Error::UnsupportedSurfaceInterest,
        Error::UploadCancelled,
        Error::UploadCleanup {
            source: boxed(Error::UploadTimeout),
            cleanup: boxed(Error::UploadCancelled),
        },
        Error::UploadCleanupPath,
        Error::UploadExit { status: status() },
        Error::UploadIo(io()),
        Error::UploadName,
        Error::UploadNotFile,
        Error::UploadPathLimit,
        Error::UploadResponse,
        Error::UploadSizeOverflow,
        Error::UploadSourceChanged,
        Error::UploadTimeout,
        Error::UploadUnsupported,
        Error::WelcomeKind,
        Error::WelcomeRejected {
            code: "busy".into(),
            message: "too many clients".into(),
        },
        Error::Wsl(wsl::Error::Timeout),
    ]);
}

#[test]
fn daemon_text_is_shown_as_it_is() {
    assert_verbatim(&[
        Error::Endpoint {
            code: EndpointErrorCode::PaneNotFound,
            message: "pane not found".into(),
        },
        Error::ServerShutdown("server is shutting down".into()),
        Error::Storage {
            operation: herdr_client::StorageOperation::Open,
            path: PathBuf::from("/tmp/catalog.json"),
            source: boxed(Error::ServerShutdown("server is shutting down".into())),
        },
    ]);
}

#[test]
fn every_clipboard_variant_has_a_message() {
    assert_localized(&[
        clipboard::Error::Busy,
        clipboard::Error::Cancelled,
        clipboard::Error::InputTarget,
        clipboard::Error::PreparationTimeout,
        clipboard::Error::WriteTimeout,
    ]);
}

#[test]
fn every_sessions_variant_has_a_message() {
    assert_localized(&[
        sessions::Error::DeleteDefault,
        sessions::Error::DeleteFailed(status()),
        sessions::Error::DeleteTimeout,
        sessions::Error::Limit,
    ]);
}

#[test]
fn every_ssh_variant_has_a_message() {
    let failures = [
        SshFailure::Auth,
        SshFailure::HerdrMissing,
        SshFailure::HostKey,
        SshFailure::LocalNetworkDenied,
        SshFailure::NoRoute,
        SshFailure::Other,
        SshFailure::Unreachable,
    ];
    assert_localized(&failures);
    let mut errors = vec![
        ssh::Error::Cancelled,
        ssh::Error::Closed,
        ssh::Error::InvalidTarget,
        ssh::Error::KnownHostsUnavailable,
        ssh::Error::OutputLimit,
        ssh::Error::Timeout,
        ssh::Error::Unsupported,
    ];
    errors.extend(failures.into_iter().map(ssh::Error::Refused));
    assert_localized(&errors);
}

#[test]
fn every_wsl_variant_has_a_message() {
    let path = PathBuf::from(r"C:\Users\ada\notes.md");
    assert_localized(&[
        wsl::Error::Catalog,
        wsl::Error::CatalogSchema(json()),
        wsl::Error::Closed,
        wsl::Error::Command(status()),
        wsl::Error::HostExists,
        wsl::Error::HostMissing,
        wsl::Error::InvalidDistro,
        wsl::Error::PathInvalid(path.clone()),
        wsl::Error::PathLimit,
        wsl::Error::PathResponse,
        wsl::Error::PathUnmapped(path),
        wsl::Error::Timeout,
        wsl::Error::Unsupported,
    ]);
}
