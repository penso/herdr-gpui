//! Why `ssh` closed the bridge before its ready banner. The child's exit code
//! and a bounded stderr tail are read only to pick a class here; the text is
//! never retained, logged, or shown, since it can name users, keys, and paths.
use crate::Result;
use std::{
    io::{ErrorKind, Read},
    process::ChildStderr,
    sync::mpsc::{self, Receiver},
    thread,
};
#[cfg(unix)]
use std::{
    net::{IpAddr, ToSocketAddrs},
    process::Child,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// What stopped an SSH bridge before it was ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SshFailure {
    /// The host key is unknown or changed; strict checking refused it.
    #[error("the host key is not trusted; verify it by running ssh to the host in a terminal")]
    HostKey,
    /// No credential was accepted without a prompt.
    #[error(
        "authentication failed; load a key into the agent or open a ControlMaster connection in a terminal"
    )]
    Auth,
    /// The host could not be resolved or reached.
    #[error("the host is unreachable")]
    Unreachable,
    /// The system had no route to the address.
    #[error("there is no route to the host")]
    NoRoute,
    /// macOS refused a local network address because Herdr lacks Local
    /// Network permission. The system reports it as no route to the host,
    /// even when a terminal that has the permission reaches the same host.
    #[error(
        "macOS denied Herdr access to your local network. If it just asked, allow it and try again; otherwise turn on Herdr in System Settings › Privacy & Security › Local Network"
    )]
    LocalNetworkDenied,
    /// No Herdr was found in the known install locations. One that is
    /// installed but cannot serve this client is `Error::BridgeIncompatible`.
    #[error("no Herdr is installed on the host")]
    HerdrMissing,
    /// None of the above could be told from what `ssh` reported.
    #[error("check host trust, authentication, and remote Herdr installation")]
    Other,
}

impl SshFailure {
    /// Whether retrying soon is pointless: the user has to fix something
    /// outside the app first, such as a key, the known hosts, or an install.
    pub fn needs_user(self) -> bool {
        matches!(self, Self::HostKey | Self::Auth | Self::HerdrMissing)
    }

    /// `ssh` exits 255 for its own failures; the bridge script exits 127 when
    /// it found no candidate binary at all. Any other status is the remote's.
    #[cfg(unix)]
    pub(super) fn classify(code: Option<i32>, stderr: &[u8]) -> Self {
        Self::classify_with(code, stderr, resolve)
    }

    /// [`Self::classify`] with the name lookup a hostname in the error needs.
    #[cfg(unix)]
    fn classify_with(
        code: Option<i32>,
        stderr: &[u8],
        resolve: impl FnOnce(&str) -> Vec<IpAddr>,
    ) -> Self {
        match code {
            Some(127) => Self::HerdrMissing,
            Some(255) => {
                let text = String::from_utf8_lossy(stderr);
                let has = |markers: &[&str]| markers.iter().any(|m| text.contains(m));
                if has(&[
                    "Host key verification failed",
                    "REMOTE HOST IDENTIFICATION HAS CHANGED",
                ]) {
                    Self::HostKey
                } else if has(&["Permission denied", "Too many authentication failures"]) {
                    Self::Auth
                } else if has(&["No route to host"]) {
                    if cfg!(target_os = "macos")
                        && refused_host(&text).is_some_and(|host| match host.parse::<IpAddr>() {
                            Ok(address) => is_local(address),
                            Err(_) => resolve(host).into_iter().any(is_local),
                        })
                    {
                        Self::LocalNetworkDenied
                    } else {
                        Self::NoRoute
                    }
                } else if has(&[
                    "Could not resolve hostname",
                    "Connection refused",
                    "timed out",
                    "Network is unreachable",
                    "Connection closed by",
                    "Connection reset",
                ]) {
                    Self::Unreachable
                } else {
                    Self::Other
                }
            }
            _ => Self::Other,
        }
    }
}

/// The host in `ssh: connect to host <host> port <port>: ...`. `ssh` prints
/// the name it was given, so this is an address only when the target was one.
#[cfg(unix)]
fn refused_host(stderr: &str) -> Option<&str> {
    stderr.lines().rev().find_map(|line| {
        let (_, rest) = line.split_once("connect to host ")?;
        let (host, _) = rest.split_once(" port ")?;
        Some(host)
    })
}

/// Addresses macOS treats as the local network, which Local Network privacy
/// gates: private and link-local ranges, not loopback or the Internet.
/// Tailscale's ranges are a tunnel, not the LAN: its IPv4 range (100.64/10)
/// is not private, and its IPv6 one is carved out of unique-local below.
#[cfg(unix)]
fn is_local(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => (v6.is_unique_local() && !is_tailscale(v6)) || v6.is_unicast_link_local(),
    }
}

/// Tailscale's IPv6 prefix, `fd7a:115c:a1e0::/48`.
#[cfg(unix)]
fn is_tailscale(address: std::net::Ipv6Addr) -> bool {
    address.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0]
}

/// How long classification waits for a hostname's addresses. `ssh` has just
/// resolved the same name, so the system usually answers from its cache.
#[cfg(unix)]
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(1);

/// The addresses `host` resolves to, or none once `RESOLVE_TIMEOUT` passes.
/// The lookup runs on its own thread because the resolver cannot be
/// cancelled; a slow one is abandoned rather than waited on.
#[cfg(unix)]
fn resolve(host: &str) -> Vec<IpAddr> {
    let (tx, rx) = mpsc::sync_channel(1);
    let host = host.to_owned();
    let spawned = thread::Builder::new()
        .name("herdr-ssh-resolve".into())
        .spawn(move || {
            let addresses = (host.as_str(), 0)
                .to_socket_addrs()
                .map(|addresses| addresses.map(|address| address.ip()).collect())
                .unwrap_or_default();
            let _ = tx.send(addresses);
        });
    match spawned {
        Ok(_) => rx.recv_timeout(RESOLVE_TIMEOUT).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Bytes of stderr kept for classification. `ssh` reports its failure last.
const STDERR_TAIL: usize = 4096;
/// How long a closed bridge may take to exit and finish its stderr.
#[cfg(unix)]
const DIAGNOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Drain the child's stderr for its whole life, so a chatty remote can never
/// fill the pipe and stall `ssh`, and hand over the tail once it closes.
pub(super) fn drain(stderr: ChildStderr) -> Result<Receiver<Vec<u8>>> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("herdr-ssh-stderr".into())
        .spawn(move || {
            let _ = tx.send(tail(stderr));
        })?;
    Ok(rx)
}

fn tail(mut stderr: impl Read) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        let n = match stderr.read(&mut buffer) {
            Ok(0) => return kept,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return kept,
        };
        kept.extend_from_slice(&buffer[..n]);
        if kept.len() > STDERR_TAIL {
            kept.drain(..kept.len() - STDERR_TAIL);
        }
    }
}

/// Classify a bridge whose stdout closed before it was ready. Waits on the
/// connection worker, at most `DIAGNOSE_TIMEOUT`, for the exit and stderr.
#[cfg(unix)]
pub(super) fn diagnose(
    child: &mut Child,
    stderr: &Receiver<Vec<u8>>,
    stop: &AtomicBool,
) -> SshFailure {
    let deadline = Instant::now() + DIAGNOSE_TIMEOUT;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() < deadline && !stop.load(Ordering::Acquire) => {
                thread::sleep(Duration::from_millis(10));
            }
            _ => return SshFailure::Other,
        }
    };
    let stderr = stderr
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap_or_default();
    SshFailure::classify(code, &stderr)
}

#[cfg(all(test, unix))]
mod tests;
