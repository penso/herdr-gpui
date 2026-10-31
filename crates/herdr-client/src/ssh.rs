//! POSIX discovery/stdio bridge adapted from upstream remote/attach.rs.
//! No installers, daemon restarts, or SSH config edits. A host key is trusted
//! only after the user approves its fingerprint (see `host_key`).
//! The remote host is POSIX or Windows. On Unix the `ssh` child's standard
//! streams are one end of a socket pair; on Windows they are private pipes,
//! driven with the same read and write deadlines as the daemon's named pipe (see
//! `transport`). Windows OpenSSH has no connection multiplexing, so there a
//! configured `ControlPath` is never used and each connection authenticates.
//! The bridge and probe scripts, and the handshake that reads them, are shared
//! with WSL distributions, which run the same scripts through `wsl.exe`. A host
//! that cannot run `/bin/sh`, such as Windows OpenSSH, is retried through
//! PowerShell; see `powershell`.
use crate::{Result, catalog::validate_target, limits::POLL, session_socket, transport::Stream};
#[cfg(unix)]
use std::os::fd::OwnedFd;
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

mod error;
mod failure;
mod host_key;
pub(crate) mod mux;
mod unverified;
pub use error::Error;
pub use failure::SshFailure;
pub use host_key::{HostKeyFingerprint, HostKeyScan, NewHostKey, scan_host_key, trust_host_key};
pub use unverified::{HostKeys, MAX_UNVERIFIED_HOSTS, set_host_keys, set_unverified_hosts};
mod powershell;

const READY: &[u8] = b"herdr-remote-output-ready:1\n";

/// A bridge, probe, or script child (`ssh`, or `wsl.exe` on Windows), killed
/// and reaped on every exit path.
pub(crate) struct ChildGuard(pub(crate) Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) use crate::script::shell_quote as quote;

// PATH first, followed by known install roots and mise's installed `latest`
// links. Never invoke mise/shims: resolving project config can run hooks or
// install tools, and need not work in a noninteractive SSH shell.
// Keep paths in shell variables: discovered executable names are never eval'd.
pub(super) const CANDIDATES: &str = r#"candidate=$(command -v herdr 2>/dev/null || :)
case "$candidate" in /*) ;; *) candidate=;; esac
linuxbrew=
case "$(PATH=/usr/bin:/bin uname -s)" in Linux) linuxbrew=/home/linuxbrew/.linuxbrew/bin/herdr;; esac
mise_data=${MISE_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/mise}
case "$mise_data" in /*) ;; *) mise_data=;; esac
set -- "$candidate" "$HOME/.local/bin/herdr" /opt/homebrew/bin/herdr /usr/local/bin/herdr "$linuxbrew" "$HOME/.nix-profile/bin/herdr" "/etc/profiles/per-user/$USER/bin/herdr" /nix/var/nix/profiles/default/bin/herdr /run/current-system/sw/bin/herdr
if [ -n "$mise_data" ]; then
    for tool in herdr github-herdrdev-herdr github-ogulcancelik-herdr; do
        set -- "$@" "$mise_data/installs/$tool/latest/bin/herdr" "$mise_data/installs/$tool/latest/herdr"
    done
fi
for path do
    case "$path" in */mise/shims/herdr|"$mise_data/shims/herdr") continue;; esac"#;

const PROBE_CANDIDATE: &str = "herdr-probe:candidate";
const PROBE_DONE: &str = "herdr-probe:done";
pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// `ssh -G` only reads configuration, but a cold start on a busy Windows
/// machine (antivirus scanning `ssh.exe`) has taken over five seconds.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(15);
const PROBE_OUTPUT_LIMIT: u64 = 64 * 1024;

fn bridge_command(session: &str) -> String {
    format!("/bin/sh -c {}", quote(&bridge_script(session)))
}

/// The bridge as a script for `/bin/sh -c`: report each candidate's client
/// status, then run whichever one the handshake accepts.
pub(crate) fn bridge_script(session: &str) -> String {
    format!(
        r#"{CANDIDATES}
    if [ -n "$path" ] && [ -x "$path" ]; then
        status=$("$path" status client --json </dev/null) || continue
        printf '%s\n' "$status"
        printf '\n%s\n' 'herdr-remote-output-ready:1'
        IFS= read -r choice || exit 1
        case "$choice" in
            accept) exec "$path" --session {session} remote-client-bridge;;
            accept-idle) exec "$path" --session {session} remote-client-bridge --idle-timeout-v1;;
        esac
    fi
done
exit 127"#,
        session = quote(session)
    )
}

/// What a remote host offers for a saved SSH device, learned without
/// installing, upgrading, or starting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProbe {
    /// SSH could not run a command without prompting: unknown host key,
    /// password or passphrase authentication, or an unreachable host.
    SshFailed(SshFailure),
    /// No Herdr executable was found in the known install locations.
    Missing,
    /// Herdr is installed, but no copy speaks this client's endpoint protocol.
    Outdated,
    /// A compatible Herdr is installed and its server for the session is down.
    Stopped,
    /// A compatible Herdr server for the session is running.
    Running,
}

/// Every candidate reports its client and server status, one block each, so
/// the choice between them stays with the same rules the bridge applies.
fn probe_command(session: &str) -> String {
    format!("/bin/sh -c {}", quote(&probe_script(session)))
}

/// The probe as a script for `/bin/sh -c`.
pub(crate) fn probe_script(session: &str) -> String {
    format!(
        r#"{CANDIDATES}
    if [ -n "$path" ] && [ -x "$path" ]; then
        status=$("$path" status client --json </dev/null) || continue
        server=$("$path" --session {session} status server --json </dev/null) || server=
        printf '%s\n%s\n%s\n' '{PROBE_CANDIDATE}' "$status" "$server"
    fi
done
printf '%s\n' '{PROBE_DONE}'"#,
        session = quote(session)
    )
}

/// `None` when the script never finished, so partial output is not mistaken
/// for a host without Herdr.
pub(crate) fn classify_probe(output: &[u8]) -> Option<HostProbe> {
    classify_probe_with(output, compatible)
}

/// [`classify_probe`] with the host's rule for a compatible client status.
fn classify_probe_with(
    output: &[u8],
    compatible: impl Fn(&serde_json::Value) -> Option<bool>,
) -> Option<HostProbe> {
    // Each block holds one candidate's client status, then its server status.
    let mut blocks: Vec<Vec<serde_json::Value>> = Vec::new();
    for line in lines(output) {
        if line == PROBE_DONE.as_bytes() {
            let Some(statuses) = blocks
                .iter()
                .find(|statuses| statuses.iter().any(|s| compatible(s).is_some()))
            else {
                return Some(if blocks.is_empty() {
                    HostProbe::Missing
                } else {
                    HostProbe::Outdated
                });
            };
            let running = statuses
                .iter()
                .any(|s| s["running"].as_bool() == Some(true));
            return Some(if running {
                HostProbe::Running
            } else {
                HostProbe::Stopped
            });
        }
        if line == PROBE_CANDIDATE.as_bytes() {
            blocks.push(Vec::new());
        } else if let (Some(block), Ok(status)) = (blocks.last_mut(), serde_json::from_slice(line))
        {
            block.push(status);
        }
    }
    None
}

/// `output`'s lines without their `\n` or `\r\n` endings, since a Windows
/// host's PowerShell ends its lines with both.
fn lines(output: &[u8]) -> impl Iterator<Item = &[u8]> {
    output
        .split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
}

/// Blocks for at most `PROBE_TIMEOUT`: call it from a background thread.
pub fn probe_host(target: &str, session: &str) -> Result<HostProbe> {
    probe_host_with(target, session, HostKeys::of(target))
}

/// [`probe_host`] checking host keys as `keys` says, for a host whose choice
/// is not saved yet.
pub fn probe_host_with(target: &str, session: &str, keys: HostKeys) -> Result<HostProbe> {
    validate_target(target)?;
    session_socket(Path::new(""), session)?;
    let mut command = command_with(target, &probe_command(session), keys);
    // Stderr is read only to classify an `ssh` failure; its text is dropped.
    command.stderr(Stdio::piped());
    tracing::info!(category = "probe", "probing host");
    let started = Instant::now();
    let (status, output, stderr) = match run_with_stderr(&mut command, PROBE_TIMEOUT, || false) {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(category = "probe", kind = ?error.kind(), elapsed_ms = started.elapsed().as_millis() as u64, "probe did not finish");
            return Err(error);
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    if status.code() == Some(255) {
        let failure = SshFailure::classify(status.code(), &stderr);
        tracing::warn!(category = "probe", %status, ?failure, stderr_bytes = stderr.len(), elapsed_ms, "ssh failed during probe");
        return Ok(HostProbe::SshFailed(failure));
    }
    let probe = classify_probe(&output);
    if probe.is_none() && powershell::rejected_posix(status.code()) {
        tracing::info!(category = "probe", %status, "host cannot run /bin/sh; probing PowerShell");
        return probe_powershell_host(target, session, keys);
    }
    tracing::info!(
        category = "probe",
                %status,
        stdout_bytes = output.len(),
        candidates = output.split(|b| *b == b'\n').filter(|line| *line == PROBE_CANDIDATE.as_bytes()).count(),
        finished = output.split(|b| *b == b'\n').any(|line| line == PROBE_DONE.as_bytes()),
        ?probe,
        elapsed_ms,
        "probe finished"
    );
    probe.ok_or(Error::Closed.into())
}

/// [`probe_host`] for a host whose shell is `cmd.exe` or PowerShell.
fn probe_powershell_host(target: &str, session: &str, keys: HostKeys) -> Result<HostProbe> {
    let mut command = command_with(target, &powershell::probe_command(session), keys);
    command.stderr(Stdio::piped());
    let (status, output, stderr) = run_with_stderr(&mut command, PROBE_TIMEOUT, || false)?;
    if status.code() == Some(255) {
        return Ok(HostProbe::SshFailed(SshFailure::classify(
            status.code(),
            &stderr,
        )));
    }
    let probe = classify_probe_with(&output, powershell::compatible);
    tracing::info!(category = "probe", %status, ?probe, "PowerShell probe finished");
    probe.ok_or(Error::Closed.into())
}

/// The `remote.origin.url` of a repository on a saved host, read without a
/// prompt. `git_dir` is the absolute Git directory the daemon reported for the
/// workspace. `None` when the repository has no origin remote. Blocks for at
/// most `timeout`: call it from a background thread.
pub fn remote_origin_url(
    target: &str,
    git_dir: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<Option<String>> {
    remote_config_value(target, git_dir, "remote.origin.url", timeout, cancelled)
}

/// Read one Git configuration value on a saved host using bounded,
/// noninteractive SSH. `dir` is an absolute Git directory or a directory in a
/// working tree, since a daemon reports a workspace's Git directory only for
/// some checkouts. Call only from a background thread.
pub fn remote_config_value(
    target: &str,
    dir: &str,
    key: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<Option<String>> {
    validate_target(target)?;
    if !dir.starts_with('/') || dir.chars().any(char::is_control) {
        return Err(crate::Error::InvalidGitDir);
    }
    let (status, output) = run_remote(target, &config_command(dir, key), timeout, cancelled)?;
    match status.code() {
        Some(0) => {}
        // `git config --get` exits 1 when the key is absent.
        Some(1) => return Ok(None),
        _ => return Err(crate::Error::RemoteCommand(status)),
    }
    let value = String::from_utf8(output).map_err(|_| crate::Error::RemoteOutput)?;
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(crate::Error::RemoteOutput);
    }
    Ok(Some(value.to_owned()))
}

fn config_command(dir: &str, key: &str) -> String {
    // `-C` rather than `--git-dir`: Git finds the repository from either a
    // Git directory or a working directory, and exits 1 outside one.
    let script = format!(
        "exec git -c core.fsmonitor=false -C {} config --get -- {}",
        quote(dir),
        quote(key)
    );
    format!("/bin/sh -c {}", quote(&script))
}

/// Where an SSH target connects, as the local SSH configuration resolves it.
/// Different spellings of one host (an alias, `user@address`, `ssh://`, or a
/// config-supplied user or port) resolve to the same value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Destination {
    pub user: String,
    pub host: String,
    pub port: u16,
}

/// Resolve `target` with `ssh -G`, which reads configuration only and never
/// connects. Blocks briefly: call it from a background thread.
pub fn resolve_destination(target: &str) -> Result<Destination> {
    validate_target(target)?;
    let mut command = crate::environment::command("ssh");
    command.args(["-G", "--", target]);
    let (status, output) = run(&mut command, RESOLVE_TIMEOUT, || false)?;
    if !status.success() {
        tracing::warn!(category = "resolve", %status, "ssh -G failed");
        return Err(crate::Error::RemoteCommand(status));
    }
    let destination = parse_destination(&output);
    tracing::info!(
        category = "resolve",
        resolved = destination.is_some(),
        "resolved SSH target"
    );
    destination.ok_or(crate::Error::RemoteOutput)
}

fn parse_destination(output: &[u8]) -> Option<Destination> {
    let text = std::str::from_utf8(output).ok()?;
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(' '))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    Some(Destination {
        user: value("user")?.to_owned(),
        // Host names are case-insensitive; addresses are unaffected.
        host: value("hostname")?.to_ascii_lowercase(),
        port: value("port")?.parse().ok()?,
    })
}

/// Runs one noninteractive SSH command, keeping at most `PROBE_OUTPUT_LIMIT`
/// bytes of stdout and never stderr, which can carry banners or secrets.
/// It shares the host's quick-lane master when it can; waiting for one
/// counts against `timeout`.
fn run_remote(
    target: &str,
    remote_command: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<(std::process::ExitStatus, Vec<u8>)> {
    let deadline = Instant::now() + timeout;
    let (mut command, _channel) = mux::command(
        target,
        remote_command,
        mux::Lane::Quick,
        deadline,
        &cancelled,
    );
    let timeout = deadline
        .checked_duration_since(Instant::now())
        .ok_or(Error::Timeout)?;
    run(&mut command, timeout, cancelled)
}

/// Runs `command` with a deadline, keeping at most `PROBE_OUTPUT_LIMIT` bytes
/// of stdout and discarding stderr.
pub(crate) fn run(
    command: &mut Command,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<(std::process::ExitStatus, Vec<u8>)> {
    command.stderr(Stdio::null());
    let (status, output, _) = run_with_stderr(command, timeout, cancelled)?;
    Ok((status, output))
}

/// [`run`], also returning a bounded tail of stderr when the caller piped it.
/// That tail is for classification only: callers never log or show it.
fn run_with_stderr(
    command: &mut Command,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut child = ChildGuard(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let stdout = child.0.stdout.take().ok_or(Error::Closed)?;
    let stderr = match child.0.stderr.take() {
        Some(stderr) => Some(failure::drain(stderr)?),
        None => None,
    };
    // The pipe reaches EOF once the child exits or the guard kills it.
    let reader = std::thread::Builder::new()
        .name("herdr-ssh-command".into())
        .spawn(move || {
            let mut output = Vec::new();
            stdout
                .take(PROBE_OUTPUT_LIMIT + 1)
                .read_to_end(&mut output)
                .map(|_| output)
        })?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if cancelled() {
            return Err(Error::Cancelled.into());
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout.into());
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let output = reader.join().map_err(|_| Error::Closed)??;
    if output.len() as u64 > PROBE_OUTPUT_LIMIT {
        return Err(Error::OutputLimit.into());
    }
    // The pipe closes once the child exits, so this waits only for the drain.
    let stderr = stderr
        .and_then(|stderr| stderr.recv_timeout(Duration::from_secs(2)).ok())
        .unwrap_or_default();
    Ok((status, output, stderr))
}

/// Agent forwarding and connection sharing follow the user's SSH config, as
/// upstream's client does. `ForwardAgent yes` lets the remote bridge register
/// the forwarded agent with the daemon, so remote panes keep a working
/// `SSH_AUTH_SOCK` across reconnects. A configured `ControlPath` lets a master
/// the user authenticated interactively (MFA, passwords) carry these
/// noninteractive connections. `ControlMaster=no` still forbids this child from
/// becoming a master: killing it must never end the user's other sessions, and
/// it must not leave a persistent background process behind.
///
/// Windows OpenSSH has no connection multiplexing: with any `ControlPath` set
/// it fails with "getsockname failed: Not a socket" and exits 127, which reads
/// as a host without Herdr. There the policy sets `ControlPath=none`, so each
/// connection authenticates on its own.
pub(super) fn command(target: &str, remote_command: &str) -> Command {
    command_with(target, remote_command, HostKeys::of(target))
}

/// [`command`] checking host keys as `keys` says rather than as the user's
/// saved choice for `target` does.
fn command_with(target: &str, remote_command: &str, keys: HostKeys) -> Command {
    let mut command = noninteractive(keys);
    command.args(["-o", "ClearAllForwardings=yes", "--", target]);
    command.arg(remote_command);
    command
}

/// `ssh` with the bridge's connection policy and nothing to connect yet.
fn noninteractive(keys: HostKeys) -> Command {
    let mut command = crate::environment::command("ssh");
    command.args(keys.options()).args(POLICY);
    command
}

/// The bridge's connection policy. OpenSSH keeps the first value it reads for
/// an option, so a caller that needs another value passes it before these.
pub(crate) const POLICY: &[&str] = &[
    "-T",
    "-C",
    "-o",
    "BatchMode=yes",
    "-o",
    "NumberOfPasswordPrompts=0",
    "-o",
    "StrictHostKeyChecking=yes",
    "-o",
    "ConnectTimeout=10",
    "-o",
    "ConnectionAttempts=1",
    "-o",
    "ServerAliveInterval=15",
    "-o",
    "ServerAliveCountMax=4",
    "-o",
    "ForwardX11=no",
    "-o",
    "ControlMaster=no",
    #[cfg(windows)]
    "-o",
    #[cfg(windows)]
    "ControlPath=none",
];

/// The line a [`forward_command`] child prints on stdout once its forward is
/// bound.
pub const FORWARD_READY: &str = "herdr-forward-ready";

/// A noninteractive `ssh` child that runs no remote command and forwards
/// `127.0.0.1:<local>` on this machine to `<address>:<remote>` as `target`
/// dials it, `address` being the remote's `localhost` when None, so a remote
/// server opens here through SSH whatever address it listens on. Forwarding is this child's whole purpose, so unlike the bridge it
/// keeps forwardings.
///
/// Readiness comes from SSH itself, never from connecting to the port, which
/// another local process could have bound first: `ExitOnForwardFailure` ends
/// the child when the bind fails, and OpenSSH runs `LocalCommand` only after
/// its forwards are set up, so [`FORWARD_READY`] on stdout means this child
/// holds the port. `ControlPath=none` keeps the forward in this child rather
/// than in a shared master, so killing the child closes it; a host that needs
/// an interactively authenticated master cannot be tunnelled. The caller owns
/// the child: its streams, readiness, and reaping.
pub fn forward_command(
    target: &str,
    local: u16,
    address: Option<std::net::IpAddr>,
    remote: u16,
) -> Result<Command> {
    validate_target(target)?;
    let address = match address {
        None => "localhost".to_owned(),
        Some(std::net::IpAddr::V4(address)) => address.to_string(),
        Some(std::net::IpAddr::V6(address)) => format!("[{address}]"),
    };
    let mut command = noninteractive(HostKeys::of(target));
    command.args([
        "-N",
        "-o",
        "ControlPath=none",
        "-o",
        "PermitLocalCommand=yes",
        "-o",
        &format!("LocalCommand=echo {FORWARD_READY}"),
        "-o",
        "ExitOnForwardFailure=yes",
        "-L",
        &format!("127.0.0.1:{local}:{address}:{remote}"),
        "--",
        target,
    ]);
    Ok(command)
}

/// A one-shot, noninteractive `ssh` child that runs `script` under the remote
/// `/bin/sh`, whatever the login shell, with the bridge's connection policy.
/// The caller owns the child: its streams, deadline, and reaping.
pub fn script_command(target: &str, script: &str) -> Result<Command> {
    validate_target(target)?;
    Ok(command(target, &format!("/bin/sh -c {}", quote(script))))
}

/// An open channel on a host's shared connection. Keep it until the child
/// it was handed out with is reaped; dropping it frees the channel.
#[derive(Debug)]
pub struct SshChannel(#[allow(dead_code)] mux::Channel);

/// [`script_command`] on the host's shared `lane` connection when one is
/// available (see `ssh::mux`), else connecting directly as
/// [`script_command`] does. Keep the channel until the child is reaped. May
/// wait briefly for the host's connection to come up: call it off the UI
/// thread.
pub fn shared_script_command(
    target: &str,
    script: &str,
    lane: mux::Lane,
) -> Result<(Command, Option<SshChannel>)> {
    validate_target(target)?;
    let (command, channel) = mux::command(
        target,
        &format!("/bin/sh -c {}", quote(script)),
        lane,
        Instant::now() + SHARED_WAIT,
        || false,
    );
    Ok((command, channel.map(SshChannel)))
}

/// How long [`shared_script_command`] waits for a host's shared connection
/// before connecting directly.
const SHARED_WAIT: Duration = Duration::from_secs(10);

pub(crate) fn connect(
    target: &str,
    session: &str,
    stop: &AtomicBool,
) -> Result<(Stream, ChildGuard)> {
    validate_target(target)?;
    session_socket(Path::new(""), session)?;
    mux::bridge_connecting(target, session);
    match start_bridge(command(target, &bridge_command(session)), stop, handshake)? {
        Ok(bridge) => Ok(bridge),
        Err(closed) if powershell::rejected_posix(closed.code) => {
            tracing::info!(code = ?closed.code, "host cannot run /bin/sh; trying PowerShell");
            connect_powershell(target, session, stop, closed.failure)
        }
        Err(closed) => Err(Error::Refused(closed.failure).into()),
    }
}

/// [`connect`] for a host whose shell is `cmd.exe` or PowerShell: discover a
/// compatible `herdr.exe` over one connection, then bridge it over another.
/// `posix` is why the POSIX bridge closed, kept for a host that turns out to
/// have no PowerShell either.
fn connect_powershell(
    target: &str,
    session: &str,
    stop: &AtomicBool,
    posix: SshFailure,
) -> Result<(Stream, ChildGuard)> {
    let mut probe = command(target, &powershell::probe_command(session));
    probe.stderr(Stdio::piped());
    let (status, output, stderr) =
        run_with_stderr(&mut probe, PROBE_TIMEOUT, || stop.load(Ordering::Acquire))?;
    if status.code() == Some(255) {
        return Err(Error::Refused(SshFailure::classify(status.code(), &stderr)).into());
    }
    if !lines(&output).any(|line| line == PROBE_DONE.as_bytes()) {
        return Err(Error::Refused(posix).into());
    }
    let candidates = powershell::candidates(&output);
    let Some((path, idle_timeout)) = candidates.iter().find_map(|candidate| {
        powershell::compatible(&candidate.status).map(|idle| (candidate.path.as_str(), idle))
    }) else {
        return Err(match candidates.first() {
            None => crate::Error::from(Error::Refused(SshFailure::HerdrMissing)),
            Some(candidate) => incompatible(&candidate.status),
        });
    };
    let bridge = command(
        target,
        &powershell::bridge_command(path, session, idle_timeout),
    );
    start_bridge(bridge, stop, |stream, stop| {
        await_ready(stream, stop, Instant::now()).map(drop)
    })?
    .map_err(|closed| Error::Refused(closed.failure).into())
}

/// The stdio bridge through an application-built remote command, such as
/// `coder ssh -- <workspace>`: the discovery script is appended as its final
/// argument, and the command's standard streams become the connection. The
/// caller owns the program, its arguments, and any credential it needs.
pub fn connect_command(mut command: Command, session: &str, stop: &AtomicBool) -> Result<Bridge> {
    session_socket(Path::new(""), session)?;
    command.arg(bridge_command(session));
    let (stream, child) = bridge(command, stop)?;
    Ok(Bridge { stream, child })
}

/// A spawned remote bridge that has answered discovery. Dropping it kills and
/// reaps the child.
pub struct Bridge {
    pub(crate) stream: Stream,
    pub(crate) child: ChildGuard,
}

fn bridge(command: Command, stop: &AtomicBool) -> Result<(Stream, ChildGuard)> {
    start_bridge(command, stop, handshake)?.map_err(|closed| Error::Refused(closed.failure).into())
}

/// A bridge whose child closed its stdout before answering, with the exit
/// code that tells a failed `ssh` from a remote shell that could not run.
struct Closed {
    code: Option<i32>,
    failure: SshFailure,
}

/// Spawn `command` as a bridge and let `answer` read its banner. The outer
/// error is any failure but an early close, which is the inner one.
fn start_bridge(
    command: Command,
    stop: &AtomicBool,
    answer: impl FnOnce(&mut Stream, &AtomicBool) -> Result<()>,
) -> Result<std::result::Result<(Stream, ChildGuard), Closed>> {
    let (mut stream, mut child) = spawn_bridge(command)?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let stderr = match child.0.stderr.take() {
        Some(stderr) => failure::drain(stderr)?,
        None => return Err(Error::Closed.into()),
    };
    match answer(&mut stream, stop) {
        // Every other way the bridge can close already says why; this one is
        // read from what `ssh` reported.
        Err(crate::Error::Ssh(Error::Closed)) => {
            let (code, failure) = failure::diagnose(&mut child.0, &stderr, stop);
            Ok(Err(Closed { code, failure }))
        }
        result => result.map(|()| Ok((stream, child))),
    }
}

/// Spawn `command` with one end of a socket pair as its stdin and stdout.
/// Stderr is piped: never inherit a GUI terminal. Its tail is read only to
/// classify a failed start, then dropped; see `failure`.
#[cfg(unix)]
fn spawn_bridge(mut command: Command) -> Result<(Stream, ChildGuard)> {
    let (stream, child_stream) = Stream::pair()?;
    command
        .stdin(Stdio::from(OwnedFd::from(child_stream.try_clone()?)))
        .stdout(Stdio::from(OwnedFd::from(child_stream)))
        .stderr(Stdio::piped());
    let child = ChildGuard(command.spawn()?);
    // The command still owns the child's end of the socket; while it lives,
    // a bridge that exits early never reads as closed, only as timed out.
    drop(command);
    Ok((stream, child))
}

/// Spawn `command` with a private pipe each way, which `Stream` drives with
/// the same read and write deadlines as the daemon's named pipe. Stderr is
/// piped as on Unix.
#[cfg(windows)]
fn spawn_bridge(mut command: Command) -> Result<(Stream, ChildGuard)> {
    let (stream, stdin, stdout) = Stream::child()?;
    command.stdin(stdin).stdout(stdout).stderr(Stdio::piped());
    let child = ChildGuard(command.spawn()?);
    // The command still owns the child's ends of the pipes; while it lives,
    // a bridge that exits early never reads as closed, only as timed out.
    drop(command);
    Ok((stream, child))
}

/// Answer the bridge script: skip each incompatible candidate it reports and
/// accept the first compatible one, which then becomes the client stream.
/// When every candidate was skipped, the script exits and the first refusal
/// says why, instead of the closed bridge every other failure also ends in.
pub(crate) fn handshake(stream: &mut (impl Read + Write), stop: &AtomicBool) -> Result<()> {
    let started = Instant::now();
    let mut refused = None;
    loop {
        let status = match await_ready(stream, stop, started) {
            Err(crate::Error::Ssh(Error::Closed)) => {
                return Err(refused.unwrap_or(Error::Closed.into()));
            }
            result => result?,
        };
        if let Some(idle_timeout) = compatible_status(&status) {
            stream.write_all(if idle_timeout {
                b"accept-idle\n"
            } else {
                b"accept\n"
            })?;
            return Ok(());
        }
        refused = refused.or_else(|| incompatible_status(&status));
        stream.write_all(b"skip\n")?;
    }
}

/// Why a candidate's `status client --json` was skipped, if it reported one.
fn incompatible_status(output: &[u8]) -> Option<crate::Error> {
    let status = output
        .split(|b| *b == b'\n')
        .find_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())?;
    Some(incompatible(&status))
}

/// The refusal for one `status client --json` object that cannot serve.
fn incompatible(status: &serde_json::Value) -> crate::Error {
    crate::Error::BridgeIncompatible {
        generation: status["endpoint_protocol_generation"]
            .as_u64()
            .and_then(|generation| u32::try_from(generation).ok()),
        version: status["version"].as_str().and_then(crate::compat::label),
    }
}

fn compatible_status(output: &[u8]) -> Option<bool> {
    output
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .find_map(|status| compatible(&status))
}

/// Whether one `status client --json` object can serve as this client's
/// bridge, and if so whether it supports the idle timeout.
fn compatible(status: &serde_json::Value) -> Option<bool> {
    if status["endpoint_protocol_generation"].as_u64() != Some(1) {
        return None;
    }
    let capabilities = status["endpoint_capabilities"].as_array()?;
    if ![
        "surface_interest",
        "presentation_effects_fence",
        "health_check",
    ]
    .iter()
    .all(|required| capabilities.iter().any(|c| c.as_str() == Some(required)))
    {
        return None;
    }
    Some(
        status["remote_bridge_idle_timeout"]
            .as_bool()
            .unwrap_or(false),
    )
}

/// Read the bridge's banner until the ready line, returning what preceded it.
/// Takes only `Read`: the SSH child's pipe is a `UnixStream`, but the limit,
/// cancellation and timeout rules here are stream-independent and tested so.
fn await_ready(
    stream: &mut (impl Read + ?Sized),
    stop: &AtomicBool,
    started: Instant,
) -> Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut output = Vec::new();
    let mut total = 0;
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(Error::Cancelled.into());
        }
        if started.elapsed() >= Duration::from_secs(15) {
            return Err(Error::Timeout.into());
        }
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {
                return Err(Error::Closed.into());
            }
            Ok(_) => {
                total += 1;
                if total > 16384 {
                    return Err(Error::OutputLimit.into());
                }
                line.push(byte[0]);
                if byte[0] == b'\n' {
                    if line == READY || is_crlf(&line, READY) {
                        return Ok(output);
                    }
                    output.extend_from_slice(&line);
                    line.clear();
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// Whether `line` is `expected` with a `\r` before its final `\n`.
fn is_crlf(line: &[u8], expected: &[u8]) -> bool {
    line.strip_suffix(b"\r\n")
        .zip(expected.strip_suffix(b"\n"))
        .is_some_and(|(line, expected)| line == expected)
}

// Fixtures that need `/bin/sh` or a socket pair are Unix-only; the rest, and
// the Windows pipe transport, run everywhere.
#[cfg(test)]
#[allow(clippy::unwrap_used)] // Test fixtures only.
mod tests;
