use super::*;
use crate::code_server::{Error, NoAnswer};
use std::{io, path::PathBuf, process::ExitStatus, time::Duration};

#[cfg(unix)]
fn exit_status() -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(256)
}

#[cfg(windows)]
fn exit_status() -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(1)
}

fn io() -> io::Error {
    io::Error::other("disk on fire")
}

#[test]
fn every_code_server_variant_has_a_message() {
    let mut errors = vec![
        Error::NotServer { status: 404 },
        Error::TokenRefused,
        Error::Status(503),
        Error::NoDataDir,
        Error::TokenFile {
            path: PathBuf::from("/tmp/code/token"),
            source: io(),
        },
        Error::TokenRandom(getrandom::Error::UNSUPPORTED),
        Error::NoFreePort(io()),
        Error::PortTaken { port: 18080 },
        Error::PortCheck {
            port: 18080,
            source: io(),
        },
        Error::Spawn {
            program: PathBuf::from("/usr/local/bin/code"),
            source: io(),
        },
        Error::Exited(exit_status()),
        Error::Watch(io()),
        Error::Silent(Duration::from_secs(30)),
        Error::Worker(io()),
    ];
    errors.extend(
        [
            NoAnswer::Refused,
            NoAnswer::Timeout,
            NoAnswer::UnknownHost,
            NoAnswer::Other,
        ]
        .map(|reason| Error::Unreachable {
            address: "127.0.0.1:18080".into(),
            reason,
            source: ureq::Error::ConnectionFailed,
        }),
    );
    assert_localized(&errors);
}
