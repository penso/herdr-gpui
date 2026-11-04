//! Shared SSH connections for one-shot commands. Each saved host gets at most
//! one master per [`Lane`], started on first use and private to this process,
//! so a burst of reads opens channels on one login instead of a login each.
//! Hosts that rate-limit new SSH connections (a firewall's `limit` rule,
//! sshd's `MaxStartups`) then see one or two logins rather than a dozen.
//!
//! The terminal bridge and port forwards keep their own connections: typing
//! never queues behind side traffic, and a master's trouble never ends a
//! terminal. Every failure here falls back to a direct connection, as before
//! this existed, so sharing can only remove logins, never add a failure.
//!
//! Ownership. A master never outlives this process and never touches the
//! user's own masters:
//! - its control socket is in an owner-only directory this process created;
//! - `ControlPersist=no` keeps it in the foreground as this process's child;
//! - it runs under a `/bin/sh` watchdog that reads a pipe from this process:
//!   when the pipe closes, because the master is stopped or because this
//!   process exits or crashes, the watchdog ends the master.
//!
//! Windows OpenSSH cannot share connections, so there every command connects
//! directly.
#[cfg(unix)]
use std::time::Duration;
use std::time::Instant;

/// Which of a host's masters a command shares. Large transfers get their own
/// connection so their data never queues ahead of a small read in one TCP
/// stream, and a stalled transfer cannot hold up the reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// Small, latency-sensitive commands: Git reads, probes, short scripts.
    Quick,
    /// Commands that move real data, such as uploads and installs.
    Bulk,
}

#[cfg(unix)]
impl Lane {
    /// Channels open at once on one master. sshd's `MaxSessions` (default
    /// 10) refuses more per connection; a command beyond this connects
    /// directly instead of waiting.
    fn channels(self) -> usize {
        match self {
            Self::Quick => 8,
            Self::Bulk => 2,
        }
    }
}

/// How long the first command waits for a host's master before connecting
/// directly. SSH gives up connecting after `ConnectTimeout` (10 s).
#[cfg(unix)]
const READY_TIMEOUT: Duration = Duration::from_secs(20);
/// After a master fails to start, the host's commands connect directly for
/// this long before another master is tried. Each further failure in a row
/// doubles it up to `MAX_BACKOFF`, so a host that always refuses one (say,
/// it needs a password the master cannot type) sees a failed login only
/// rarely, while a brief refusal recovers within a minute.
#[cfg(unix)]
const BACKOFF: Duration = Duration::from_secs(60);
#[cfg(unix)]
const MAX_BACKOFF: Duration = Duration::from_secs(30 * 60);
/// A master with no channels open for this long is stopped.
#[cfg(unix)]
const IDLE: Duration = Duration::from_secs(300);

/// `remote_command` on `target` over the host's `lane` master, with the
/// channel that must stay alive until the child is reaped; or a direct
/// connection, with no channel, when sharing is unavailable. Waits for a
/// master being started until `deadline`, or until `cancelled`.
pub(crate) fn command(
    target: &str,
    remote_command: &str,
    lane: Lane,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
) -> (std::process::Command, Option<Channel>) {
    #[cfg(unix)]
    if let Some(channel) = shared::registry().open(target, lane, deadline, cancelled) {
        let command = shared::channel_command(target, remote_command, &channel.socket);
        return (command, Some(channel));
    }
    #[cfg(not(unix))]
    let _ = (lane, deadline, cancelled);
    (super::command(target, remote_command), None)
}

/// A bridge to `session` on `target` is connecting. When it connected before
/// in this process, this is a reconnect, and the network path may have
/// changed (the user's config can pick a LAN or a VPN address), so the host's
/// current masters are retired: commands already on them finish, and the
/// next one starts a new master. A first connect keeps them, so the master a
/// startup read just opened is not replaced at once.
pub(crate) fn bridge_connecting(target: &str, session: &str) {
    #[cfg(unix)]
    shared::registry().bridge_connecting(target, session);
    #[cfg(not(unix))]
    let _ = (target, session);
}

/// Whether `target`'s host key is checked has changed, so its masters, opened
/// under the old choice, take no new channels; the next command starts a
/// master that follows the new one.
pub(crate) fn host_keys_changed(target: &str) {
    #[cfg(unix)]
    shared::registry().retire(target);
    #[cfg(not(unix))]
    let _ = target;
}

#[cfg(unix)]
pub(crate) use shared::Channel;

/// No master exists on this platform, so no channel is ever handed out.
#[cfg(not(unix))]
#[derive(Debug)]
pub(crate) enum Channel {}

#[cfg(unix)]
mod shared;
