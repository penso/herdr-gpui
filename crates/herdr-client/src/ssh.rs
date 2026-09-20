//! POSIX discovery/stdio bridge adapted from upstream remote/attach.rs.
//! No installers, daemon restarts, SSH config edits, or trust-on-first-use.
use crate::{POLL, catalog::validate_target, invalid, session_socket};
use std::{
    io::{self, Read, Write},
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const READY: &[u8] = b"herdr-remote-output-ready:1\n";

pub(crate) struct SshChild(Child);
impl Drop for SshChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn bridge_command(session: &str) -> String {
    // PATH first, excluding mise shims, followed by upstream's known install roots.
    // Keep paths in shell variables: discovered executable names are never eval'd.
    let script = format!(
        r#"candidate=$(command -v herdr 2>/dev/null || :)
case "$candidate" in /*/mise/shims/herdr) candidate=;; /*) ;; *) candidate=;; esac
for path in "$candidate" "$HOME/.local/bin/herdr" /opt/homebrew/bin/herdr /usr/local/bin/herdr /home/linuxbrew/.linuxbrew/bin/herdr "$HOME/.nix-profile/bin/herdr" "/etc/profiles/per-user/$USER/bin/herdr" /nix/var/nix/profiles/default/bin/herdr /run/current-system/sw/bin/herdr; do
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

fn command(target: &str, session: &str) -> Command {
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
        "ForwardAgent=no",
        "-o",
        "ForwardX11=no",
        "-o",
        "ClearAllForwardings=yes",
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=none",
        "--",
        target,
    ]);
    command.arg(bridge_command(session));
    command
}

pub(crate) fn connect(
    target: &str,
    session: &str,
    stop: &AtomicBool,
) -> io::Result<(UnixStream, SshChild)> {
    validate_target(target)?;
    session_socket(Path::new(""), session)?;
    let (mut stream, child_stream) = UnixStream::pair()?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut command = command(target, session);
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

fn compatible_status(output: &[u8]) -> Option<bool> {
    output
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .find_map(|status| {
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
        })
}

fn await_ready(
    stream: &mut UnixStream,
    stop: &AtomicBool,
    started: Instant,
) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    let mut output = Vec::new();
    let mut total = 0;
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "SSH connection cancelled",
            ));
        }
        if started.elapsed() >= Duration::from_secs(15) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "SSH discovery timed out",
            ));
        }
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "SSH bridge closed; check host trust, authentication, and remote Herdr installation",
                ));
            }
            Ok(_) => {
                total += 1;
                if total > 16384 {
                    return Err(invalid("SSH startup output exceeds limit"));
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
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Test fixtures only.
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn discovery_and_bridge_stdio_work_with_quoted_install_paths() {
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
        let (mut stream, child_stream) = UnixStream::pair().unwrap();
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
    fn command_is_noninteractive_and_target_is_one_argument() {
        let command = command("user@host;not-a-command", "agents");
        let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
        for option in [
            "BatchMode=yes",
            "StrictHostKeyChecking=yes",
            "ForwardAgent=no",
            "ControlPath=none",
        ] {
            assert!(args.contains(&option));
        }
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
    fn marker_consumes_banners_not_protocol_bytes() {
        let (mut stream, mut remote) = UnixStream::pair().unwrap();
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
    fn startup_output_is_bounded() {
        let (mut stream, mut remote) = UnixStream::pair().unwrap();
        let writer = std::thread::spawn(move || remote.write_all(&vec![b'x'; 16385]).unwrap());
        assert!(
            await_ready(&mut stream, &AtomicBool::new(false), Instant::now())
                .unwrap_err()
                .to_string()
                .contains("limit")
        );
        writer.join().unwrap();
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
}
