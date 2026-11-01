use super::*;
use crate::listening_ports::{Error, tunnel};

#[test]
fn every_listening_ports_variant_has_a_message() {
    assert_localized(&[
        Error::Scan(Box::new(crate::Error::NotConnected)),
        Error::Tool,
        Error::Unsupported,
    ]);
}

#[test]
fn every_tunnel_variant_has_a_message() {
    let io = || std::io::Error::other("disk on fire");
    assert_localized(&[
        tunnel::Error::Exited(exit_status()),
        tunnel::Error::Port(io()),
        tunnel::Error::Start(io()),
        tunnel::Error::Timeout,
    ]);
}

/// A process that exited with status 1, as the system describes it.
pub(super) fn exit_status() -> std::process::ExitStatus {
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;
    #[cfg(unix)]
    let code = 1 << 8;
    #[cfg(windows)]
    let code = 1;
    std::process::ExitStatus::from_raw(code)
}
