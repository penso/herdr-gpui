use super::*;
use crate::daemon::Error;
use serde_json::json;
use std::{path::PathBuf, process::ExitStatus};

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

#[test]
fn every_daemon_variant_has_a_message() {
    assert_localized(&[
        Error::Cancelled,
        Error::Exited(exit_status()),
        // The daemon said nothing of its own.
        Error::Response(json!({})),
        Error::Response(json!({ "message": "" })),
        Error::Response(json!(42)),
        Error::Spawn {
            source: std::io::Error::other("no such file"),
        },
        Error::Timeout(PathBuf::from("/tmp/herdr.sock")),
    ]);
}

#[test]
fn daemon_messages_show_as_they_are() {
    assert_verbatim(&[
        Error::Response(json!({ "message": "workspace not found", "code": "missing" })),
        Error::Response(json!("server busy")),
        Error::Response(json!({ "code": "invalid_params" })),
    ]);
}
