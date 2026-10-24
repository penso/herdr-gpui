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
