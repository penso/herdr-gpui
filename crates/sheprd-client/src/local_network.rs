//! Whether macOS lets this process reach the local network. Local Network
//! privacy (macOS 15 and later) offers no API to read or request the
//! permission, and a refused connection reads as "no route to host", which an
//! offline device or a bad route also produces. A multicast send cannot fail
//! that way: it needs no neighbour, so a refused one is the permission. That
//! is the signal here, and an answer it cannot give is `Unknown`, never a
//! guess.
//!
//! The answer belongs to this process's responsible code: children such as
//! `ssh` that this app starts share it, while processes in a Herdr daemon's
//! panes are judged on their own.
use std::{
    io,
    net::{Ipv4Addr, UdpSocket},
    thread,
    time::{Duration, Instant},
};

/// What the system said about the local network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalNetworkAccess {
    Allowed,
    /// Refused. While the system still waits for the user to answer its
    /// alert, it refuses too, so this can turn into `Allowed` on its own.
    Denied,
    /// The check could not run, for example with no IPv4 network up.
    Unknown,
}

/// mDNS's group and port: an address every Mac already sends to.
const MDNS: (Ipv4Addr, u16) = (Ipv4Addr::new(224, 0, 0, 251), 5353);
/// A DNS header that asks nothing, which responders ignore.
const EMPTY_QUERY: [u8; 12] = [0; 12];
/// How often a wait checks again.
const RECHECK: Duration = Duration::from_millis(500);

/// Ask the system now. Doing so shows the Local Network alert when the user
/// has not answered it yet, and this long-lived process keeps it on screen,
/// which a short-lived `ssh` cannot (see Apple's TN3179). Other platforms
/// have no such permission.
pub fn local_network_access() -> LocalNetworkAccess {
    if !cfg!(target_os = "macos") {
        return LocalNetworkAccess::Allowed;
    }
    let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) else {
        return LocalNetworkAccess::Unknown;
    };
    access(
        socket
            .connect(MDNS)
            .and_then(|()| socket.send(&EMPTY_QUERY)),
    )
}

/// How a multicast send's result reads. Depending on the process, macOS
/// reports a refused send as "no route to host" or as a broken pipe (seen
/// from an ad hoc signed tool in a daemon's pane); with no neighbour or peer
/// involved, both are the policy. Anything else says nothing about the
/// permission.
pub(crate) fn access(sent: io::Result<usize>) -> LocalNetworkAccess {
    match sent {
        Ok(_) => LocalNetworkAccess::Allowed,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::HostUnreachable | io::ErrorKind::BrokenPipe
            ) =>
        {
            LocalNetworkAccess::Denied
        }
        Err(_) => LocalNetworkAccess::Unknown,
    }
}

/// Check until access is no longer refused, for at most `limit`, while
/// `keep_waiting` returns `true`; it is asked before each check, so the
/// caller can give up. Returns the last answer, `Denied` when the wait ran
/// out. Blocks: call it off the UI thread.
pub fn wait_for_local_network(
    limit: Duration,
    keep_waiting: impl FnMut() -> bool,
) -> LocalNetworkAccess {
    wait_with(limit, RECHECK, local_network_access, keep_waiting)
}

pub(crate) fn wait_with(
    limit: Duration,
    recheck: Duration,
    mut check: impl FnMut() -> LocalNetworkAccess,
    mut keep_waiting: impl FnMut() -> bool,
) -> LocalNetworkAccess {
    let deadline = Instant::now() + limit;
    loop {
        if !keep_waiting() {
            return LocalNetworkAccess::Denied;
        }
        let access = check();
        let now = Instant::now();
        if access != LocalNetworkAccess::Denied || now >= deadline {
            return access;
        }
        thread::sleep(recheck.min(deadline - now));
    }
}

#[cfg(test)]
mod tests;
