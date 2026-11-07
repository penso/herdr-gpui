//! SSH servers that announce `_ssh._tcp` over multicast DNS: a Mac with Remote
//! Login on, or a Linux host whose Avahi publishes its SSH service. Hosts that
//! do not announce themselves are not found; nothing is scanned.

use super::{Candidate, Source, valid_host};
use crate::{Error, Result};
use mdns_sd::{RecvTimeoutError, ServiceDaemon};
use sheprd_client::LocalNetworkAccess;
use std::{
    collections::HashSet,
    net::IpAddr,
    time::{Duration, Instant},
};

const SERVICE: &str = "_ssh._tcp.local.";
/// How often the query is sent again while nothing has answered.
const REQUERY: Duration = Duration::from_secs(1);

/// Listen for `duration`, reporting each resolved announcement that is not
/// this machine until `found` returns `false`. Blocks: call it from the
/// background executor.
///
/// On macOS the browse first needs Local Network permission. When the
/// system refuses it, which it also does while its alert waits for the
/// user's answer, `waiting(true)` is reported and the permission is checked
/// again for up to `permission_wait`, until `waiting` returns `false`. Once
/// allowed, `waiting(false)` is reported and the full `duration` follows.
/// The first queries of a fresh permission can still go unanswered: the
/// library backs off to queries 1, 3 and 7 seconds in, so until something
/// answers, the browse restarts every second, which sends a new query.
pub(super) fn browse(
    duration: Duration,
    permission_wait: Duration,
    mut waiting: impl FnMut(bool) -> bool,
    mut found: impl FnMut(Candidate) -> bool,
) -> Result<()> {
    let local: HashSet<IpAddr> = if_addrs::get_if_addrs()
        .map_err(Error::LocalAddresses)?
        .into_iter()
        .map(|interface| interface.ip())
        .collect();
    if cfg!(target_os = "macos") && local.iter().any(|ip| ip.is_ipv4() && !ip.is_loopback()) {
        let access = permitted(
            sheprd_client::local_network_access,
            |keep_waiting| sheprd_client::wait_for_local_network(permission_wait, keep_waiting),
            &mut waiting,
        );
        match access {
            Permission::Granted => {}
            Permission::Cancelled => return Ok(()),
            Permission::Refused => return Err(Error::LocalNetworkDenied),
        }
    }
    let daemon = ServiceDaemon::new()?;
    let mut events = daemon.browse(SERVICE)?;
    let deadline = Instant::now() + duration;
    let mut answered = false;
    loop {
        let wake = if answered {
            deadline
        } else {
            deadline.min(Instant::now() + REQUERY)
        };
        let event = match events.recv_deadline(wake) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) if !answered && Instant::now() < deadline => {
                // Replaces the listener, so the old receiver gets nothing more.
                let Ok(again) = daemon.browse(SERVICE) else {
                    break;
                };
                events = again;
                continue;
            }
            // Timed out, or the daemon stopped on its own.
            Err(_) => break,
        };
        let mdns_sd::ServiceEvent::ServiceResolved(service) = event else {
            continue;
        };
        answered = true;
        let addresses: Vec<IpAddr> = service
            .get_addresses()
            .iter()
            .map(|address| address.to_ip_addr())
            .collect();
        let Some(candidate) = candidate(
            service.get_fullname(),
            service.get_hostname(),
            service.get_port(),
            &addresses,
            &local,
        ) else {
            continue;
        };
        if !found(candidate) {
            break;
        }
    }
    // What was found stands even if the daemon's thread fails to stop
    // cleanly; it exits with this process either way.
    let _ = daemon.shutdown();
    Ok(())
}

/// Where the Local Network permission left the browse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Permission {
    Granted,
    /// Still refused when the wait ran out.
    Refused,
    /// The form went away while waiting.
    Cancelled,
}

/// Check the permission, waiting for the user while it is refused. The mDNS
/// daemon only logs a refused send, so without this the browse would find
/// nothing and look like an empty network. An `Unknown` answer is left to
/// the browse.
pub(super) fn permitted(
    check: impl FnOnce() -> LocalNetworkAccess,
    wait: impl FnOnce(&mut dyn FnMut() -> bool) -> LocalNetworkAccess,
    waiting: &mut impl FnMut(bool) -> bool,
) -> Permission {
    if check() != LocalNetworkAccess::Denied {
        return Permission::Granted;
    }
    let mut open = waiting(true);
    let access = wait(&mut || {
        open = open && waiting(true);
        open
    });
    if !open {
        return Permission::Cancelled;
    }
    if access == LocalNetworkAccess::Denied {
        return Permission::Refused;
    }
    if waiting(false) {
        Permission::Granted
    } else {
        Permission::Cancelled
    }
}

/// The suggestion for one announcement, or `None` for this machine or a host
/// name unsafe to use as an SSH target.
pub(super) fn candidate(
    fullname: &str,
    hostname: &str,
    port: u16,
    addresses: &[IpAddr],
    local: &HashSet<IpAddr>,
) -> Option<Candidate> {
    if addresses.iter().any(|address| local.contains(address)) {
        return None;
    }
    let host = hostname.trim_end_matches('.');
    if !valid_host(host) || port == 0 {
        return None;
    }
    let instance = fullname
        .strip_suffix(SERVICE)
        .unwrap_or(fullname)
        .trim_end_matches('.');
    let name = if instance.is_empty() { host } else { instance };
    let target = if port == 22 {
        host.to_owned()
    } else {
        format!("ssh://{host}:{port}")
    };
    let addresses: Vec<String> = addresses.iter().map(IpAddr::to_string).collect();
    let hosts: Vec<&str> = std::iter::once(host)
        .chain(addresses.iter().map(String::as_str))
        .collect();
    Some(Candidate::new(Source::Bonjour, name, target, &hosts))
}
