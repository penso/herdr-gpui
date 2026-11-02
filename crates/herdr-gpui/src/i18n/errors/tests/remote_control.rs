use super::*;
use herdr_remote_control::{
    app,
    control::{self, ErrorCode},
};
use std::{io, path::PathBuf, process::ExitStatus};

fn io() -> io::Error {
    io::Error::other("disk on fire")
}

#[cfg(unix)]
fn status() -> ExitStatus {
    std::os::unix::process::ExitStatusExt::from_raw(256)
}

#[cfg(windows)]
fn status() -> ExitStatus {
    std::os::windows::process::ExitStatusExt::from_raw(1)
}

fn qr() -> app::Error {
    app::qr::Qr::encode(&"x".repeat(5000)).err().unwrap()
}

#[test]
fn every_app_variant_has_a_message() {
    let path = PathBuf::from("/tmp/remote/control.plist");
    let mut errors = vec![
        app::Error::Control(control::Error::NoAnswer),
        app::Error::IdleDays,
        app::Error::NoStateDir,
        app::Error::PairingAddress,
        app::Error::PairingUrl(url::ParseError::EmptyHost),
        app::Error::PathText { path: path.clone() },
        app::Error::Port,
        app::Error::ProgramMissing,
        qr(),
        app::Error::Service {
            program: "launchctl",
            status: status(),
            stderr: "Bootstrap failed: 5: Input/output error".into(),
        },
        app::Error::Spawn {
            program: "systemctl",
            source: io(),
        },
        app::Error::Unexpected,
        app::Error::UnknownUser,
        app::Error::Unsupported,
    ];
    errors.extend(["cert", "key"].map(|which| app::Error::CertificatePath { which }));
    errors.extend(
        ["create", "write", "remove"].map(|operation| app::Error::Io {
            operation,
            path: path.clone(),
            source: io(),
        }),
    );
    assert_localized(&errors);
}

#[test]
fn every_control_variant_has_a_message() {
    assert_localized(&[
        control::Error::Connect(io()),
        control::Error::Daemon {
            code: ErrorCode::UnknownPairing,
            message: "no such pairing".into(),
        },
        control::Error::NoAnswer,
    ]);
}

/// Framing errors are the chat protocol's own, through its mapping.
#[test]
fn frame_errors_use_the_chat_protocols_messages() {
    assert_localized(&[
        control::Error::Frame(herdr_chat_protocol::Error::EmptyFrame),
        control::Error::Frame(herdr_chat_protocol::Error::Io(io())),
    ]);
}
