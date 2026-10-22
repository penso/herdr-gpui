#![forbid(unsafe_code)]

use std::{ffi::OsString, process::Command};

/// Called only by worker threads, such as the daemon connection's and the
/// VS Code server's, never by a GPUI render/update.
pub(crate) fn apply(command: &mut Command) {
    #[cfg(unix)]
    if let Some(environment) = unix::resolve() {
        command.envs(environment.iter().map(|(key, value)| (key, value)));
    } else {
        command.env("PATH", crate::local_path::local_path());
    }
    #[cfg(windows)]
    let _ = command;
}

/// The `PATH` that [`apply`] gives a command, for finding a program on it.
/// Blocking the first time on Unix, like [`apply`]: run it off the UI thread.
pub(crate) fn path() -> Option<OsString> {
    #[cfg(unix)]
    return match unix::resolve() {
        Some(environment) => environment
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, value)| value.clone()),
        None => Some(crate::local_path::local_path()),
    };
    #[cfg(windows)]
    std::env::var_os("PATH")
}

/// The variable `name` as [`apply`] gives it to a command: the login
/// shell's value when it sets one, else this process's. Blocking the first
/// time on Unix, like [`apply`]: run it off the UI thread.
pub(crate) fn var(name: &str) -> Option<OsString> {
    #[cfg(unix)]
    if let Some((_, value)) = unix::resolve().and_then(|environment| {
        environment
            .iter()
            .find(|(key, _)| key.as_os_str() == std::ffi::OsStr::new(name))
    }) {
        return Some(value.clone());
    }
    std::env::var_os(name)
}

/// What the login shell at `shell` exports, probed as [`apply`] probes the
/// user's, for tests elsewhere that start from a desktop launch's bare
/// environment.
#[cfg(all(test, unix))]
pub(crate) fn probe_for_tests(shell: &std::path::Path) -> Option<Vec<(OsString, OsString)>> {
    unix::probe(
        shell.as_os_str(),
        false,
        false,
        std::time::Duration::from_secs(5),
    )
}

/// Adopts the login shell's `XDG_CONFIG_HOME` for a launch without one, such
/// as from Finder or a desktop launcher, so the daemon a shell started, and
/// the settings beside it, are found where the shell keeps them. Probing the
/// shell blocks, so this runs once before the app starts, and only when no
/// daemon answers at the default endpoint: a daemon there needs nothing
/// adopted, and without one the probe would run anyway to start it.
pub(crate) fn adopt_config_home() {
    #[cfg(unix)]
    {
        if std::env::var_os("XDG_CONFIG_HOME").is_some()
            || herdr_client::ConnectTarget::Local
                .socket_path()
                .is_ok_and(|path| unix::answers(&path))
        {
            return;
        }
        let Some(home) = unix::resolve().and_then(unix::config_home) else {
            return;
        };
        if herdr_client::set_login_config_home(home.into()) {
            tracing::info!("Using the login shell's XDG_CONFIG_HOME");
        }
    }
}

#[cfg(unix)]
mod unix {
    use std::{
        ffi::{OsStr, OsString},
        io::{self, IsTerminal, Read},
        os::{
            fd::OwnedFd,
            unix::{ffi::OsStringExt, net::UnixStream, process::CommandExt},
        },
        process::Stdio,
        sync::{
            OnceLock,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    type Environment = Vec<(OsString, OsString)>;
    static ENVIRONMENT: OnceLock<Option<Environment>> = OnceLock::new();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    const LIMIT: usize = 1024 * 1024;

    #[derive(Debug, thiserror::Error)]
    enum ProbeError {
        #[error(transparent)]
        Io(#[from] io::Error),
        #[error("login shell timed out")]
        Timeout,
        #[error("login shell failed: {0}")]
        Exit(std::process::ExitStatus),
        #[error("invalid or oversized login-shell environment")]
        Output,
    }

    pub(super) fn resolve() -> Option<&'static Environment> {
        ENVIRONMENT
            .get_or_init(|| {
                let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
                probe(
                    &shell,
                    io::stdin().is_terminal(),
                    std::env::var_os("TERM").is_some(),
                    Duration::from_secs(5),
                )
            })
            .as_ref()
    }

    /// Whether a daemon accepts connections at `socket`. A socket file an
    /// exited daemon left behind refuses them, so it does not count. A local
    /// connect is answered or refused at once, and dropping it unanswered is
    /// the same check upstream's session list makes; the daemon only logs a
    /// client that handshakes.
    pub(super) fn answers(socket: &std::path::Path) -> bool {
        herdr_client::Stream::connect(socket).is_ok()
    }

    pub(super) fn config_home(environment: &Environment) -> Option<OsString> {
        environment
            .iter()
            .find(|(key, value)| key == "XDG_CONFIG_HOME" && !value.is_empty())
            .map(|(_, value)| value.clone())
    }

    pub(super) fn probe(
        shell: &OsStr,
        tty: bool,
        term: bool,
        timeout: Duration,
    ) -> Option<Environment> {
        if tty || term {
            return None;
        }
        match capture(shell, timeout, LIMIT) {
            Ok(environment) => Some(environment),
            Err(error) => {
                tracing::warn!(%error, "Could not resolve login-shell environment; using fallback PATH");
                None
            }
        }
    }

    fn capture(shell: &OsStr, timeout: Duration, limit: usize) -> Result<Environment, ProbeError> {
        let id = format!(
            "herdr-login-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let start = format!("{id}-start");
        let end = format!("{id}-end");
        let script = format!("printf '{start}\\0'; /usr/bin/env -0; printf '{end}\\0'");
        let (mut reader, writer) = UnixStream::pair()?;
        reader.set_nonblocking(true)?;
        let mut child = herdr_client::new_command(shell)
            .args(["-l", "-i", "-c"])
            .arg(script)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::from(OwnedFd::from(writer)))
            .process_group(0)
            .spawn()?;
        let deadline = Instant::now() + timeout;
        let result = (|| {
            let mut output = Vec::new();
            loop {
                if Instant::now() >= deadline {
                    return Err(ProbeError::Timeout);
                }
                let mut buffer = [0; 8192];
                match reader.read(&mut buffer) {
                    Ok(len) => {
                        output.extend_from_slice(&buffer[..len]);
                        if output.len() > limit {
                            return Err(ProbeError::Output);
                        }
                        if len > 0 {
                            continue;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
                if let Some(status) = child.try_wait()? {
                    if !status.success() {
                        return Err(ProbeError::Exit(status));
                    }
                    // Drain bytes written between the last read and observing exit.
                    loop {
                        match reader.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(len) => {
                                output.extend_from_slice(&buffer[..len]);
                                if output.len() > limit {
                                    return Err(ProbeError::Output);
                                }
                            }
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                            Err(error) => return Err(error.into()),
                        }
                        if Instant::now() >= deadline {
                            return Err(ProbeError::Timeout);
                        }
                    }
                    return parse(&output, start.as_bytes(), end.as_bytes())
                        .ok_or(ProbeError::Output);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        if result.is_err() {
            // rc files can leave children holding stdout open; kill the whole group.
            if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = child.wait();
        }
        result
    }

    fn parse(output: &[u8], start: &[u8], end: &[u8]) -> Option<Environment> {
        let start = [start, &[0]].concat();
        let end = [end, &[0]].concat();
        let offset = output
            .windows(start.len())
            .position(|bytes| bytes == start)?
            + start.len();
        let output = &output[offset..];
        let end = output.windows(end.len()).position(|bytes| bytes == end)?;
        let data = output[..end].strip_suffix(&[0])?;
        data.split(|byte| *byte == 0)
            .map(|entry| {
                let equals = entry.iter().position(|byte| *byte == b'=')?;
                if equals == 0 {
                    return None;
                }
                Some((
                    OsString::from_vec(entry[..equals].to_vec()),
                    OsString::from_vec(entry[equals + 1..].to_vec()),
                ))
            })
            .collect::<Option<Environment>>()
            .map(|env| env.into_iter().filter(|(key, _)| allowed(key)).collect())
    }

    fn allowed(key: &OsStr) -> bool {
        let key = key.as_encoded_bytes();
        ![
            b"PWD".as_slice(),
            b"OLDPWD",
            b"SHLVL",
            b"_",
            b"TERM",
            b"TERM_SESSION_ID",
            b"COLORTERM",
            b"SECURITYSESSIONID",
        ]
        .contains(&key)
            && ![
                b"TERM_PROGRAM".as_slice(),
                b"ITERM_",
                b"DIRENV_",
                b"HERDR_",
                b"__CF",
                b"XPC_",
            ]
            .iter()
            .any(|prefix| key.starts_with(prefix))
    }

    #[cfg(test)]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    mod tests {
        use super::*;

        fn shell(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("shell");
            crate::test_executable::write(&path, format!("#!/bin/sh\n{contents}\n"), 0o700)
                .unwrap();
            (dir, path)
        }

        #[test]
        fn noisy_shell_preserves_login_environment() {
            let (_dir, path) = shell(
                "[ \"$1 $2 $3\" = '-l -i -c' ] || exit 9\nprintf '\\033]1337;noise\\007hello\\n'\nexport LOGIN_ENV_TEST='value=with spaces'\neval \"$4\"\nprintf 'logout noise'",
            );
            let env = probe(path.as_os_str(), false, false, Duration::from_secs(2)).unwrap();
            assert!(env.contains(&("LOGIN_ENV_TEST".into(), "value=with spaces".into())));
            assert!(env.iter().all(|(key, _)| allowed(key)));
        }

        #[test]
        fn config_home_is_the_login_shells_nonempty_value() {
            let environment = |value: &str| -> Environment {
                vec![
                    ("HOME".into(), "/home/me".into()),
                    ("XDG_CONFIG_HOME".into(), value.into()),
                ]
            };
            assert_eq!(
                config_home(&environment("/home/me/dotfiles/etc")),
                Some("/home/me/dotfiles/etc".into())
            );
            assert_eq!(config_home(&environment("")), None);
            assert_eq!(config_home(&vec![("HOME".into(), "/home/me".into())]), None);
        }

        /// A socket an exited daemon left behind is not a daemon, so a launch
        /// still adopts the login shell's root; a listening one, or a file
        /// that is not a socket at all, is told apart the same way.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        #[test]
        fn only_a_listening_socket_answers() {
            use nix::sys::socket::{AddressFamily, SockFlag, SockType, UnixAddr, bind, socket};
            use std::os::fd::AsRawFd;
            let dir = tempfile::tempdir().unwrap();
            let socket_path = dir.path().join("herdr-client.sock");
            assert!(!answers(&socket_path));
            let listener = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
            assert!(answers(&socket_path));
            drop(listener);
            // A dropped listener can keep answering on macOS: a child another
            // test thread spawns before it is marked close-on-exec inherits it.
            // A socket that is bound but never listens refuses even if leaked.
            let stale = dir.path().join("stale.sock");
            let fd = socket(
                AddressFamily::Unix,
                SockType::Stream,
                SockFlag::empty(),
                None,
            )
            .unwrap();
            bind(fd.as_raw_fd(), &UnixAddr::new(&stale).unwrap()).unwrap();
            drop(fd);
            assert!(stale.exists(), "a closed socket leaves its file");
            assert!(!answers(&stale));
            let plain = dir.path().join("plain.sock");
            std::fs::write(&plain, b"").unwrap();
            assert!(!answers(&plain));
        }

        #[test]
        fn markers_and_records_must_be_complete() {
            assert_eq!(
                parse(b"noiseSTART\0A=x=y\0B=\xff\0END\0noise", b"START", b"END"),
                Some(vec![
                    ("A".into(), "x=y".into()),
                    ("B".into(), OsString::from_vec(vec![255]))
                ])
            );
            for output in [
                b"START\0A=x\0".as_slice(),
                b"A=x\0END\0",
                b"START\0bad\0END\0",
                b"START\0=x\0END\0",
                b"START\0A=xEND\0",
            ] {
                assert!(parse(output, b"START", b"END").is_none());
            }
        }

        #[test]
        fn denylist_keeps_only_portable_environment() {
            for key in [
                "PWD",
                "OLDPWD",
                "SHLVL",
                "_",
                "TERM",
                "TERM_PROGRAM",
                "TERM_PROGRAM_VERSION",
                "TERM_SESSION_ID",
                "ITERM_PROFILE",
                "COLORTERM",
                "DIRENV_DIR",
                "HERDR_ENV",
                "__CFBundleIdentifier",
                "XPC_SERVICE_NAME",
                "SECURITYSESSIONID",
            ] {
                let output = format!("START\0{key}=discard\0PATH=/tools\0HOME=/home/me\0END\0");
                assert_eq!(
                    parse(output.as_bytes(), b"START", b"END"),
                    Some(vec![
                        ("PATH".into(), "/tools".into()),
                        ("HOME".into(), "/home/me".into())
                    ])
                );
            }
        }

        #[test]
        fn failed_missing_and_malformed_shells_return_none() {
            for script in ["eval \"$4\"; exit 7", "printf noise"] {
                let (_dir, path) = shell(script);
                assert!(probe(path.as_os_str(), false, false, Duration::from_secs(2)).is_none());
            }
            assert!(
                probe(
                    OsStr::new("/nonexistent/herdr-login-shell"),
                    false,
                    false,
                    Duration::from_secs(2)
                )
                .is_none()
            );
        }

        #[test]
        fn output_is_bounded_and_io_causes_are_preserved() {
            // macOS 15's head has no -c option; dd supports byte-sized fixtures there.
            let (_dir, path) = shell("/bin/dd if=/dev/zero bs=1024 count=2");
            // Exercise the size bound independently of CI's bulk-output throughput.
            let error = capture(path.as_os_str(), Duration::from_secs(2), 1024).unwrap_err();
            assert!(matches!(error, ProbeError::Output), "{error:?}");
            assert!(
                matches!(capture(OsStr::new("/nonexistent/herdr-login-shell"), Duration::from_secs(2), LIMIT), Err(ProbeError::Io(error)) if error.kind() == io::ErrorKind::NotFound)
            );
        }

        #[test]
        fn terminal_launch_does_not_execute_shell() {
            let (dir, path) = shell("touch \"$0.ran\"");
            for (tty, term) in [(true, false), (false, true), (true, true)] {
                assert!(probe(path.as_os_str(), tty, term, Duration::from_secs(2)).is_none());
            }
            assert!(!dir.path().join("shell.ran").exists());
        }

        #[test]
        fn timeout_kills_shell_and_descendant_group() {
            let (dir, path) =
                shell("/bin/sleep 60 &\nprintf '%s %s' \"$$\" \"$!\" > \"$0.pids\"\nwait");
            let start = Instant::now();
            // Room for a loaded machine to start the shell and record its pids
            // before the timeout fires; the sleep outlasts it either way.
            assert!(probe(path.as_os_str(), false, false, Duration::from_secs(3)).is_none());
            assert!(start.elapsed() < Duration::from_secs(10));
            let pids = std::fs::read_to_string(dir.path().join("shell.pids")).unwrap();
            for pid in pids.split_whitespace() {
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    let output = std::process::Command::new("/bin/ps")
                        .args(["-o", "stat=", "-p", pid])
                        .output()
                        .unwrap();
                    let state = String::from_utf8(output.stdout).unwrap();
                    // Orphaned children may remain zombies until the system reaps them.
                    if state.trim().is_empty() || state.trim().starts_with('Z') {
                        break;
                    }
                    assert!(Instant::now() < deadline, "process {pid} survived: {state}");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
}
