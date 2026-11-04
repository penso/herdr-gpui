//! Feature errors keep the retry/cancellation category and the display text
//! they had as flat root variants.
use super::*;

fn assert_kinds(cases: impl IntoIterator<Item = (Error, io::ErrorKind, &'static str)>) {
    for (error, kind, text) in cases {
        assert_eq!(error.kind(), kind, "{error:?}");
        assert_eq!(error.to_string(), text, "{error:?}");
    }
}

#[test]
fn ssh_errors_keep_kind_and_text() {
    use io::ErrorKind::*;
    assert_kinds([
        (
            ssh::Error::Cancelled.into(),
            Interrupted,
            "SSH connection cancelled",
        ),
        (
            ssh::Error::Closed.into(),
            UnexpectedEof,
            "SSH bridge closed; check host trust, authentication, and remote Herdr installation",
        ),
        (
            ssh::Error::InvalidTarget.into(),
            InvalidInput,
            "invalid SSH target (options, controls, and passwords are forbidden)",
        ),
        (
            ssh::Error::KnownHostsUnavailable.into(),
            InvalidData,
            "SSH has no user known hosts file to save the host key to",
        ),
        (
            ssh::Error::OutputLimit.into(),
            InvalidData,
            "SSH startup output exceeds limit",
        ),
        (
            ssh::Error::Timeout.into(),
            TimedOut,
            "SSH discovery timed out",
        ),
        (
            ssh::Error::Unsupported.into(),
            Unsupported,
            "SSH port forwarding is not supported on this platform",
        ),
    ]);
    for (failure, kind) in [
        (SshFailure::HostKey, PermissionDenied),
        (SshFailure::Auth, PermissionDenied),
        (SshFailure::Unreachable, NotConnected),
        (SshFailure::NoRoute, NotConnected),
        (SshFailure::LocalNetworkDenied, NotConnected),
        (SshFailure::HerdrMissing, NotFound),
        (SshFailure::Other, UnexpectedEof),
    ] {
        let error = Error::from(ssh::Error::Refused(failure));
        assert_eq!(error.kind(), kind, "{error:?}");
        assert_eq!(error.to_string(), format!("SSH bridge closed: {failure}"));
    }
}

#[test]
fn wsl_errors_keep_kind_and_text() {
    use io::ErrorKind::*;
    let path = || std::path::PathBuf::from("C:/x");
    assert_kinds([
        (
            wsl::Error::Catalog.into(),
            InvalidData,
            "unsupported WSL device list version, size, or entries",
        ),
        (
            wsl::Error::Closed.into(),
            UnexpectedEof,
            "WSL bridge closed; check that the distribution starts and Herdr is installed in it",
        ),
        (
            wsl::Error::HostExists.into(),
            InvalidData,
            "this WSL distribution is already a device",
        ),
        (
            wsl::Error::HostMissing.into(),
            InvalidData,
            "this WSL distribution is not a saved device",
        ),
        (
            wsl::Error::InvalidDistro.into(),
            InvalidInput,
            "invalid WSL distribution name",
        ),
        (
            wsl::Error::PathInvalid(path()).into(),
            InvalidInput,
            "C:/x is not an absolute Windows path a WSL distribution can map",
        ),
        (
            wsl::Error::PathLimit.into(),
            InvalidInput,
            "a WSL distribution takes at most 256 dropped paths at once",
        ),
        (
            wsl::Error::PathResponse.into(),
            InvalidData,
            "invalid answer from wslpath",
        ),
        (
            wsl::Error::PathUnmapped(path()).into(),
            NotFound,
            "C:/x is not reachable from this WSL distribution; only its own files and mounted drives are",
        ),
        (
            wsl::Error::Timeout.into(),
            TimedOut,
            "WSL did not answer in time; the distribution may be stopped or waiting for setup",
        ),
        (
            wsl::Error::Unsupported.into(),
            Unsupported,
            "WSL distributions are only available on Windows",
        ),
    ]);
    let schema = serde_json::from_str::<u8>("x").unwrap_err();
    let error = Error::from(wsl::Error::CatalogSchema(schema));
    assert_eq!(error.kind(), InvalidData);
    assert_eq!(error.to_string(), "invalid WSL device list schema");
}

#[test]
fn clipboard_errors_keep_kind_and_text() {
    use io::ErrorKind::*;
    assert_kinds([
        (
            clipboard::Error::Busy.into(),
            WouldBlock,
            "a clipboard image upload is already reserved or in progress",
        ),
        (
            clipboard::Error::Cancelled.into(),
            Interrupted,
            "clipboard image upload cancelled",
        ),
        (
            clipboard::Error::InputTarget.into(),
            InvalidData,
            "semantic input fallback requires a pane or popup target",
        ),
        (
            clipboard::Error::PreparationTimeout.into(),
            TimedOut,
            "clipboard image preparation timed out; reservation skipped",
        ),
        (
            clipboard::Error::WriteTimeout.into(),
            TimedOut,
            "clipboard image write timed out; reconnect required",
        ),
    ]);
}

#[test]
fn sessions_errors_keep_kind_and_text() {
    use io::ErrorKind::*;
    assert_kinds([
        (
            sessions::Error::DeleteDefault.into(),
            InvalidInput,
            "The default session cannot be deleted",
        ),
        (
            sessions::Error::DeleteTimeout.into(),
            TimedOut,
            "Session deletion timed out; refresh the list before trying again",
        ),
        (
            sessions::Error::Limit.into(),
            InvalidData,
            "too many sessions to list",
        ),
    ]);
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let status = std::process::ExitStatus::from_raw(256);
        let error = Error::from(sessions::Error::DeleteFailed(status));
        assert_eq!(error.kind(), InvalidData);
        assert_eq!(
            error.to_string(),
            format!(
                "Herdr refused session deletion ({status}); the session may be running, inaccessible, or unsupported by the installed CLI"
            )
        );
    }
}
