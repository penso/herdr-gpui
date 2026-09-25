//! POSIX discovery/stdio bridge adapted from upstream remote/attach.rs.
//! No installers, daemon restarts, SSH config edits, or trust-on-first-use.
//! The remote host is always POSIX; the local half needs a socket pair it can
//! hand to the `ssh` child as its standard streams, which only Unix provides.
#[cfg(unix)]
use crate::limits::POLL;
use crate::{Error, Result, catalog::validate_target, session_socket, transport::Stream};
#[cfg(unix)]
use std::{
    io::{self, Read, Write},
    os::fd::OwnedFd,
    process::{Child, Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use std::{path::Path, sync::atomic::AtomicBool};

#[cfg(unix)]
const READY: &[u8] = b"herdr-remote-output-ready:1\n";

#[cfg(unix)]
pub(crate) struct SshChild(pub(super) Child);
#[cfg(unix)]
impl Drop for SshChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// No bridge child is ever spawned on Windows, so this type has no values.
#[cfg(windows)]
pub(crate) enum SshChild {}

#[cfg(unix)]
pub(super) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

// PATH first, excluding mise shims, followed by upstream's known install roots.
// Keep paths in shell variables: discovered executable names are never eval'd.
#[cfg(unix)]
pub(super) const CANDIDATES: &str = r#"candidate=$(command -v herdr 2>/dev/null || :)
case "$candidate" in /*/mise/shims/herdr) candidate=;; /*) ;; *) candidate=;; esac
for path in "$candidate" "$HOME/.local/bin/herdr" /opt/homebrew/bin/herdr /usr/local/bin/herdr /home/linuxbrew/.linuxbrew/bin/herdr "$HOME/.nix-profile/bin/herdr" "/etc/profiles/per-user/$USER/bin/herdr" /nix/var/nix/profiles/default/bin/herdr /run/current-system/sw/bin/herdr; do"#;

#[cfg(unix)]
const PROBE_CANDIDATE: &str = "herdr-probe:candidate";
#[cfg(unix)]
const PROBE_DONE: &str = "herdr-probe:done";
#[cfg(unix)]
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(unix)]
const PROBE_OUTPUT_LIMIT: u64 = 64 * 1024;

#[cfg(unix)]
fn bridge_command(session: &str) -> String {
    let script = format!(
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
    );
    format!("/bin/sh -c {}", quote(&script))
}

/// What a remote host offers for a saved SSH device, learned without
/// installing, upgrading, or starting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProbe {
    /// SSH could not run a command without prompting: unknown host key,
    /// password or passphrase authentication, or an unreachable host.
    SshFailed,
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
#[cfg(unix)]
fn probe_command(session: &str) -> String {
    let script = format!(
        r#"{CANDIDATES}
    if [ -n "$path" ] && [ -x "$path" ]; then
        status=$("$path" status client --json </dev/null) || continue
        server=$("$path" --session {session} status server --json </dev/null) || server=
        printf '%s\n%s\n%s\n' '{PROBE_CANDIDATE}' "$status" "$server"
    fi
done
printf '%s\n' '{PROBE_DONE}'"#,
        session = quote(session)
    );
    format!("/bin/sh -c {}", quote(&script))
}

/// `None` when the script never finished, so partial output is not mistaken
/// for a host without Herdr.
#[cfg(unix)]
fn classify_probe(output: &[u8]) -> Option<HostProbe> {
    // Each block holds one candidate's client status, then its server status.
    let mut blocks: Vec<Vec<serde_json::Value>> = Vec::new();
    for line in output.split(|b| *b == b'\n') {
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

/// Blocks for at most `PROBE_TIMEOUT`: call it from a background thread.
#[cfg(unix)]
pub fn probe_host(target: &str, session: &str) -> Result<HostProbe> {
    validate_target(target)?;
    session_socket(Path::new(""), session)?;
    let (status, output) = run_remote(target, &probe_command(session), PROBE_TIMEOUT, || false)?;
    if status.code() == Some(255) {
        return Ok(HostProbe::SshFailed);
    }
    classify_probe(&output).ok_or(Error::SshClosed)
}

/// The `remote.origin.url` of a repository on a saved host, read without a
/// prompt. `git_dir` is the absolute Git directory the daemon reported for the
/// workspace. `None` when the repository has no origin remote. Blocks for at
/// most `timeout`: call it from a background thread.
#[cfg(unix)]
pub fn remote_origin_url(
    target: &str,
    git_dir: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<Option<String>> {
    validate_target(target)?;
    if !git_dir.starts_with('/') || git_dir.chars().any(char::is_control) {
        return Err(Error::InvalidGitDir);
    }
    let (status, output) = run_remote(target, &origin_command(git_dir), timeout, cancelled)?;
    match status.code() {
        Some(0) => {}
        // `git config --get` exits 1 when the key is absent.
        Some(1) => return Ok(None),
        _ => return Err(Error::RemoteCommand(status)),
    }
    let url = String::from_utf8(output).map_err(|_| Error::RemoteOutput)?;
    let url = url.trim_end_matches(['\r', '\n']);
    if url.is_empty() || url.len() > 2048 || url.chars().any(char::is_control) {
        return Err(Error::RemoteOutput);
    }
    Ok(Some(url.to_owned()))
}

#[cfg(unix)]
fn origin_command(git_dir: &str) -> String {
    let script = format!(
        "exec git -c core.fsmonitor=false --git-dir {} config --get remote.origin.url",
        quote(git_dir)
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
#[cfg(unix)]
pub fn resolve_destination(target: &str) -> Result<Destination> {
    validate_target(target)?;
    let mut command = Command::new("ssh");
    command.args(["-G", "--", target]);
    let (status, output) = run(&mut command, Duration::from_secs(5), || false)?;
    if !status.success() {
        return Err(Error::RemoteCommand(status));
    }
    parse_destination(&output).ok_or(Error::RemoteOutput)
}

#[cfg(windows)]
pub fn resolve_destination(target: &str) -> Result<Destination> {
    validate_target(target)?;
    Err(Error::SshUnsupported)
}

#[cfg(unix)]
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
#[cfg(unix)]
fn run_remote(
    target: &str,
    remote_command: &str,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<(std::process::ExitStatus, Vec<u8>)> {
    run(&mut command(target, remote_command), timeout, cancelled)
}

/// Runs `command` with a deadline, keeping at most `PROBE_OUTPUT_LIMIT` bytes
/// of stdout and discarding stderr.
#[cfg(unix)]
fn run(
    command: &mut Command,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<(std::process::ExitStatus, Vec<u8>)> {
    let mut child = SshChild(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let stdout = child.0.stdout.take().ok_or(Error::SshClosed)?;
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
            return Err(Error::SshCancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::SshTimeout);
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let output = reader.join().map_err(|_| Error::SshClosed)??;
    if output.len() as u64 > PROBE_OUTPUT_LIMIT {
        return Err(Error::SshOutputLimit);
    }
    Ok((status, output))
}

#[cfg(windows)]
pub fn probe_host(target: &str, session: &str) -> Result<HostProbe> {
    validate_target(target)?;
    session_socket(Path::new(""), session)?;
    Err(Error::SshUnsupported)
}

#[cfg(windows)]
pub fn remote_origin_url(
    target: &str,
    _git_dir: &str,
    _timeout: std::time::Duration,
    _cancelled: impl Fn() -> bool,
) -> Result<Option<String>> {
    validate_target(target)?;
    Err(Error::SshUnsupported)
}

/// Agent forwarding and connection sharing follow the user's SSH config, as
/// upstream's client does. `ForwardAgent yes` lets the remote bridge register
/// the forwarded agent with the daemon, so remote panes keep a working
/// `SSH_AUTH_SOCK` across reconnects. A configured `ControlPath` lets a master
/// the user authenticated interactively (MFA, passwords) carry these
/// noninteractive connections. `ControlMaster=no` still forbids this child from
/// becoming a master: killing it must never end the user's other sessions, and
/// it must not leave a persistent background process behind.
#[cfg(unix)]
pub(super) fn command(target: &str, remote_command: &str) -> Command {
    let mut command = Command::new("ssh");
    command.args([
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
        "ClearAllForwardings=yes",
        "-o",
        "ControlMaster=no",
        "--",
        target,
    ]);
    command.arg(remote_command);
    command
}

/// A one-shot, noninteractive `ssh` child that runs `script` under the remote
/// `/bin/sh`, whatever the login shell, with the bridge's connection policy.
/// The caller owns the child: its streams, deadline, and reaping.
#[cfg(unix)]
pub fn script_command(target: &str, script: &str) -> Result<Command> {
    validate_target(target)?;
    Ok(command(target, &format!("/bin/sh -c {}", quote(script))))
}

#[cfg(unix)]
pub(crate) fn connect(
    target: &str,
    session: &str,
    stop: &AtomicBool,
) -> Result<(Stream, SshChild)> {
    validate_target(target)?;
    session_socket(Path::new(""), session)?;
    let (mut stream, child_stream) = Stream::pair()?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut command = command(target, &bridge_command(session));
    command
        .stdin(Stdio::from(OwnedFd::from(child_stream.try_clone()?)))
        .stdout(Stdio::from(OwnedFd::from(child_stream)))
        // Do not inherit a GUI terminal or collect unbounded/secret-bearing diagnostics.
        .stderr(Stdio::null());
    let child = SshChild(command.spawn()?);
    let started = Instant::now();
    loop {
        let status = await_ready(&mut stream, stop, started)?;
        if let Some(idle_timeout) = compatible_status(&status) {
            stream.write_all(if idle_timeout {
                b"accept-idle\n"
            } else {
                b"accept\n"
            })?;
            return Ok((stream, child));
        }
        stream.write_all(b"skip\n")?;
    }
}

/// Windows rejects SSH endpoints before spawning anything. Handing a socket to a
/// child as its standard streams needs `OwnedFd`, and the anonymous pipes that
/// replace it there cannot carry the read timeouts the session loop polls on.
/// Validation still runs first so a malformed target reports the same error
/// everywhere.
#[cfg(windows)]
pub(crate) fn connect(
    target: &str,
    session: &str,
    _stop: &AtomicBool,
) -> Result<(Stream, SshChild)> {
    validate_target(target)?;
    session_socket(Path::new(""), session)?;
    Err(Error::SshUnsupported)
}

#[cfg(unix)]
fn compatible_status(output: &[u8]) -> Option<bool> {
    output
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .find_map(|status| compatible(&status))
}

/// Whether one `status client --json` object can serve as this client's
/// bridge, and if so whether it supports the idle timeout.
#[cfg(unix)]
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
#[cfg(unix)]
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
            return Err(Error::SshCancelled);
        }
        if started.elapsed() >= Duration::from_secs(15) {
            return Err(Error::SshTimeout);
        }
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {
                return Err(Error::SshClosed);
            }
            Ok(_) => {
                total += 1;
                if total > 16384 {
                    return Err(Error::SshOutputLimit);
                }
                line.push(byte[0]);
                if byte[0] == b'\n' {
                    if line == READY {
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

// The bridge is POSIX-only; its fixtures spawn real shells over a socket pair.
#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)] // Test fixtures only.
mod tests {
    use super::*;

    #[test]
    fn discovery_and_bridge_stdio_work_with_quoted_install_paths() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("herdr-client-{}-quoted ' path", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let binary = root.join("herdr");
        std::fs::write(&binary, r#"#!/bin/sh
if [ "$1" = status ]; then
    printf '%s\n' '{"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"],"remote_bridge_idle_timeout":true}'
    exit 0
fi
[ "$1" = --session ] && [ "$2" = agents ] && [ "$3" = remote-client-bridge ] && [ "$4" = --idle-timeout-v1 ] || exit 1
IFS= read -r hello || exit 1
printf '%s\n' "$hello"
"#).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (mut stream, child_stream) = Stream::pair().unwrap();
        stream.set_read_timeout(Some(POLL)).unwrap();
        let child = SshChild(
            Command::new("/bin/sh")
                .args(["-c", &bridge_command("agents")])
                .env("PATH", &root)
                .env("HOME", &root)
                .stdin(Stdio::from(OwnedFd::from(
                    child_stream.try_clone().unwrap(),
                )))
                .stdout(Stdio::from(OwnedFd::from(child_stream)))
                .spawn()
                .unwrap(),
        );
        let status = await_ready(&mut stream, &AtomicBool::new(false), Instant::now()).unwrap();
        assert_eq!(compatible_status(&status), Some(true));
        stream.write_all(b"accept-idle\nhello\n").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut response = [0; 6];
        stream.read_exact(&mut response).unwrap();
        assert_eq!(&response, b"hello\n");
        drop(child);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn probe_classifies_only_finished_output() {
        let client = r#"{"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"]}"#;
        let old = r#"{"endpoint_protocol_generation":0,"endpoint_capabilities":[]}"#;
        let running = r#"{"running":true,"version":"1"}"#;
        let stopped = r#"{"running":false}"#;
        let probe = |blocks: &[(&str, &str)], done: bool| {
            let mut output = String::from("motd banner\n");
            for (client, server) in blocks {
                output += &format!("{PROBE_CANDIDATE}\n{client}\n{server}\n");
            }
            if done {
                output += PROBE_DONE;
                output += "\n";
            }
            classify_probe(output.as_bytes())
        };
        assert_eq!(probe(&[], true), Some(HostProbe::Missing));
        assert_eq!(probe(&[(old, running)], true), Some(HostProbe::Outdated));
        assert_eq!(probe(&[(client, stopped)], true), Some(HostProbe::Stopped));
        // A failed server status leaves an empty line, which is not running.
        assert_eq!(probe(&[(client, "")], true), Some(HostProbe::Stopped));
        // The first compatible copy decides, as it does for the bridge.
        assert_eq!(
            probe(&[(old, running), (client, running)], true),
            Some(HostProbe::Running)
        );
        assert_eq!(probe(&[(client, running)], false), None);
        // A running old server does not make a compatible copy look running.
        assert_eq!(
            probe(&[(old, running), (client, stopped)], true),
            Some(HostProbe::Stopped)
        );
    }

    #[test]
    fn probe_script_reports_the_session_server_without_starting_it() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("herdr-probe-{}-a ' b", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let binary = root.join("herdr");
        std::fs::write(&binary, r#"#!/bin/sh
case "$*" in
    "status client --json") printf '%s\n' '{"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"]}';;
    "--session work's status server --json") [ -e "$HOME/up" ] && printf '%s\n' '{"running":true}' || printf '%s\n' '{"running":false}';;
    *) exit 1;;
esac
"#).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let run = || {
            let output = Command::new("/bin/sh")
                .args(["-c", &probe_command("work's")])
                .env("PATH", &root)
                .env("HOME", &root)
                .output()
                .unwrap();
            classify_probe(&output.stdout)
        };
        assert_eq!(run(), Some(HostProbe::Stopped));
        std::fs::write(root.join("up"), "").unwrap();
        assert_eq!(run(), Some(HostProbe::Running));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn origin_command_reads_only_the_named_repository() {
        let root =
            std::env::temp_dir().join(format!("herdr-origin-{}-a ' $(b)", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(&root)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        };
        git(&["init", "-q"]);
        let git_dir = root.join(".git");
        let run = || {
            Command::new("/bin/sh")
                .args(["-c", &origin_command(git_dir.to_str().unwrap())])
                .output()
                .unwrap()
        };
        // No origin: `git config --get` exits 1, which callers read as none.
        assert_eq!(run().status.code(), Some(1));
        git(&["remote", "add", "origin", "git@github.com:owner/repo.git"]);
        let output = run();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"git@github.com:owner/repo.git\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn origin_lookup_rejects_relative_or_control_git_dirs_before_ssh() {
        for dir in ["relative/.git", "/repo\n/.git", ""] {
            assert!(matches!(
                remote_origin_url("host", dir, Duration::from_secs(1), || false),
                Err(Error::InvalidGitDir)
            ));
        }
        assert!(matches!(
            remote_origin_url(
                "-oProxyCommand=x",
                "/repo/.git",
                Duration::from_secs(1),
                || false
            ),
            Err(Error::InvalidSshTarget)
        ));
    }

    #[test]
    fn destinations_come_from_the_resolved_config_not_the_spelling() {
        let parsed =
            parse_destination(b"user penso\nhostname M5Max.Local\nport 2222\nhostkeyalias none\n")
                .unwrap();
        assert_eq!(
            parsed,
            Destination {
                user: "penso".into(),
                host: "m5max.local".into(),
                port: 2222
            }
        );
        // `hostkeyalias` must not satisfy the `hostname` key.
        assert_eq!(parse_destination(b"user a\nhostnamex b\nport 22\n"), None);
        assert_eq!(parse_destination(b"user a\nhostname b\nport x\n"), None);
        // Spellings of one host agree through the real `ssh -G`.
        let a = resolve_destination("penso@example.invalid").unwrap();
        let b = resolve_destination("ssh://penso@EXAMPLE.invalid:22").unwrap();
        assert_eq!(a, b);
        assert!(matches!(
            resolve_destination("-oProxyCommand=x"),
            Err(Error::InvalidSshTarget)
        ));
    }

    #[test]
    fn probe_rejects_bad_targets_before_spawning_ssh() {
        assert!(matches!(
            probe_host("-oProxyCommand=x", "default"),
            Err(Error::InvalidSshTarget)
        ));
        assert!(matches!(
            probe_host("host", "../escape"),
            Err(Error::InvalidSession)
        ));
    }

    #[test]
    fn command_is_noninteractive_and_target_is_one_argument() {
        let command = command("user@host;not-a-command", "agents");
        let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
        for option in [
            "BatchMode=yes",
            "StrictHostKeyChecking=yes",
            "ControlMaster=no",
        ] {
            assert!(args.contains(&option));
        }
        // The user's SSH config decides agent forwarding and which master to share.
        assert!(
            !args
                .iter()
                .any(|arg| arg.starts_with("ForwardAgent=") || arg.starts_with("ControlPath="))
        );
        assert_eq!(args[args.len() - 3], "--");
        assert_eq!(args[args.len() - 2], "user@host;not-a-command");
        assert_eq!(quote("a'b"), "'a'\\''b'");
        for bad in [
            "",
            "-oProxyCommand=bad",
            "host\ncommand",
            "user:secret@host",
        ] {
            assert!(validate_target(bad).is_err());
        }
    }
    #[test]
    fn script_runs_under_remote_sh_and_rejects_option_targets() {
        let command = script_command("user@host", "echo 'hi'").unwrap();
        let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args[args.len() - 3], "--");
        assert_eq!(args[args.len() - 2], "user@host");
        assert_eq!(args[args.len() - 1], "/bin/sh -c 'echo '\\''hi'\\'''");
        assert!(matches!(
            script_command("-oProxyCommand=bad", "true"),
            Err(Error::InvalidSshTarget)
        ));
    }
    #[test]
    fn marker_consumes_banners_not_protocol_bytes() {
        let (mut stream, mut remote) = Stream::pair().unwrap();
        remote
            .write_all(b"banner\n\nherdr-remote-output-ready:1\nWIRE")
            .unwrap();
        await_ready(&mut stream, &AtomicBool::new(false), Instant::now()).unwrap();
        let mut bytes = [0; 4];
        stream.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"WIRE");
        assert!(await_ready(&mut stream, &AtomicBool::new(true), Instant::now()).is_err());
    }
    #[test]
    fn child_guard_reaps_on_drop() {
        let child = Command::new("sleep").arg("60").spawn().unwrap();
        let id = child.id();
        drop(SshChild(child));
        assert!(
            !Command::new("kill")
                .args(["-0", &id.to_string()])
                .stderr(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }
    #[test]
    fn discovery_checks_binary_capabilities_before_starting_bridge() {
        let mut status = serde_json::json!({"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"],"remote_bridge_idle_timeout":true});
        assert_eq!(compatible_status(status.to_string().as_bytes()), Some(true));
        status["endpoint_capabilities"] = serde_json::json!(["surface_interest", "health_check"]);
        assert_eq!(compatible_status(status.to_string().as_bytes()), None);
        assert_eq!(compatible_status(b"banner\n{\"wrapper\":true}\n"), None);
    }

    /// Yields each scripted result once, so a retryable error and a short read
    /// are exercised without a socket or a timing guess.
    struct ScriptedReader(std::collections::VecDeque<io::Result<Vec<u8>>>);

    impl Read for ScriptedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.0.pop_front() {
                Some(Ok(bytes)) => {
                    let len = bytes.len().min(buf.len());
                    buf[..len].copy_from_slice(&bytes[..len]);
                    Ok(len)
                }
                Some(Err(error)) => Err(error),
                None => Ok(0),
            }
        }
    }

    fn scripted(chunks: Vec<io::Result<Vec<u8>>>) -> ScriptedReader {
        ScriptedReader(chunks.into())
    }

    fn bytes(text: &[u8]) -> Vec<io::Result<Vec<u8>>> {
        text.iter().map(|byte| Ok(vec![*byte])).collect()
    }

    #[test]
    fn ready_line_ends_the_banner_and_returns_what_preceded_it() {
        let mut stream = scripted(
            bytes(b"one\ntwo\n")
                .into_iter()
                .chain(bytes(READY))
                .collect(),
        );
        let output = await_ready(&mut stream, &AtomicBool::new(false), Instant::now()).unwrap();
        assert_eq!(output, b"one\ntwo\n");
    }

    #[test]
    fn banner_output_is_bounded_and_a_closed_stream_is_reported() {
        let mut flood = scripted(bytes(&b"x".repeat(16385)));
        assert!(matches!(
            await_ready(&mut flood, &AtomicBool::new(false), Instant::now()),
            Err(Error::SshOutputLimit)
        ));
        // An exhausted script reads zero bytes, as a closed pipe does.
        let mut closed = scripted(bytes(b"partial\n"));
        assert!(matches!(
            await_ready(&mut closed, &AtomicBool::new(false), Instant::now()),
            Err(Error::SshClosed)
        ));
    }

    #[test]
    fn cancellation_and_deadline_are_checked_before_reading() {
        let stop = AtomicBool::new(true);
        let mut never_read = scripted(vec![Err(io::Error::other("must not be read"))]);
        assert!(matches!(
            await_ready(&mut never_read, &stop, Instant::now()),
            Err(Error::SshCancelled)
        ));
        let expired = Instant::now() - Duration::from_secs(16);
        let mut also_never_read = scripted(vec![Err(io::Error::other("must not be read"))]);
        assert!(matches!(
            await_ready(&mut also_never_read, &AtomicBool::new(false), expired),
            Err(Error::SshTimeout)
        ));
    }

    #[test]
    fn retryable_read_errors_do_not_end_the_banner() {
        let mut stream = scripted(
            [
                io::ErrorKind::WouldBlock,
                io::ErrorKind::TimedOut,
                io::ErrorKind::Interrupted,
            ]
            .into_iter()
            .map(|kind| Err(io::Error::from(kind)))
            .chain(bytes(READY))
            .collect(),
        );
        assert_eq!(
            await_ready(&mut stream, &AtomicBool::new(false), Instant::now()).unwrap(),
            Vec::<u8>::new()
        );
        let mut fatal = scripted(vec![Err(io::Error::other("broken pipe"))]);
        assert!(matches!(
            await_ready(&mut fatal, &AtomicBool::new(false), Instant::now()),
            Err(Error::Io(_))
        ));
    }
}

// Windows never spawns the bridge, but a rejected SSH endpoint must still be
// rejected for the same reasons and in the same order as on POSIX.
#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn invalid_targets_and_sessions_are_rejected_before_the_platform_refusal() {
        let stop = AtomicBool::new(false);
        assert!(matches!(
            connect("-oProxyCommand=x", "default", &stop),
            Err(Error::InvalidSshTarget)
        ));
        assert!(matches!(
            connect("host", "../escape", &stop),
            Err(Error::InvalidSession)
        ));
        assert!(matches!(
            connect("host", "default", &stop),
            Err(Error::SshUnsupported)
        ));
    }
}
