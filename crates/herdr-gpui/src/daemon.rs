use herdr_client::ConnectTarget;
use std::{
    env, io,
    os::unix::{net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug)]
struct MissingInstallation(io::Error);

impl std::fmt::Display for MissingInstallation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Could not start herdr server: {}. Install Herdr and use Terminal > Reconnect.",
            self.0
        )
    }
}

impl std::error::Error for MissingInstallation {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

pub(super) fn is_missing_installation(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<MissingInstallation>())
}

pub fn connect(
    target: &ConnectTarget,
    stop: &AtomicBool,
    on_start: impl FnOnce(),
) -> io::Result<UnixStream> {
    let socket = target.socket_path()?;
    connect_or_start(
        &socket,
        stop,
        Duration::from_secs(20),
        || {
            on_start();
            let mut command = Command::new(executable());
            if let ConnectTarget::Session { name, .. } = target {
                command.args(["--session", name]);
            }
            command.arg("server");
            command
        },
        matches!(
            target,
            ConnectTarget::Local
                | ConnectTarget::Session {
                    development: false,
                    ..
                }
        ),
    )
}

fn executable() -> PathBuf {
    // Finder launches have a minimal PATH, which often omits Homebrew and Cargo.
    let path = env::var_os("PATH").unwrap_or_default();
    let candidates = env::split_paths(&path)
        .map(|dir| dir.join("herdr"))
        .chain(env::var_os("HOME").into_iter().flat_map(|home| {
            let home = PathBuf::from(home);
            [home.join(".local/bin/herdr"), home.join(".cargo/bin/herdr")]
        }))
        .chain([
            PathBuf::from("/opt/homebrew/bin/herdr"),
            PathBuf::from("/usr/local/bin/herdr"),
        ]);
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| "herdr".into())
}

fn connect_or_start(
    socket: &Path,
    stop: &AtomicBool,
    timeout: Duration,
    command: impl FnOnce() -> Command,
    auto_start: bool,
) -> io::Result<UnixStream> {
    match UnixStream::connect(socket) {
        Ok(stream) => return Ok(stream),
        Err(error)
            if auto_start
                && matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) => {}
        Err(error) => return Err(error),
    }
    if stop.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "daemon startup cancelled",
        ));
    }
    let mut child = command()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(error.kind(), MissingInstallation(error))
            } else {
                io::Error::new(
                    error.kind(),
                    format!(
                        "Could not start herdr server: {error}. Use Terminal > Reconnect to retry."
                    ),
                )
            }
        })?;
    // The daemon outlives the window. Reap it if it exits while the GUI is alive.
    let (exit_tx, exit_rx) = std::sync::mpsc::channel();
    thread::Builder::new()
        .name("herdr-daemon-wait".into())
        .spawn(move || {
            let _ = exit_tx.send(child.wait());
        })?;
    let deadline = Instant::now() + timeout;
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "daemon startup cancelled",
            ));
        }
        match UnixStream::connect(socket) {
            Ok(stream) => return Ok(stream),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Timed out waiting for herdr server at {}. Check the Herdr server log and use Terminal > Reconnect.",
                    socket.display()
                ),
            ));
        }
        if let Ok(status) = exit_rx.try_recv() {
            return Err(io::Error::other(format!(
                "herdr server exited before accepting connections: {}. Check the Herdr server log.",
                status?
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::AtomicUsize;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn socket() -> PathBuf {
        env::temp_dir().join(format!(
            "gpui-{}-{}.sock",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn existing_daemon_does_not_launch() {
        let path = socket();
        let listener = UnixListener::bind(&path).unwrap();
        let result = connect_or_start(
            &path,
            &AtomicBool::new(false),
            Duration::ZERO,
            || panic!("must not launch"),
            true,
        );
        assert!(result.is_ok());
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn attach_only_and_cancelled_do_not_launch() {
        let path = socket();
        for (cancelled, auto_start, kind) in [
            (false, false, io::ErrorKind::NotFound),
            (true, true, io::ErrorKind::Interrupted),
        ] {
            let error = connect_or_start(
                &path,
                &AtomicBool::new(cancelled),
                Duration::ZERO,
                || panic!("must not launch"),
                auto_start,
            )
            .unwrap_err();
            assert_eq!(error.kind(), kind);
            assert!(!is_missing_installation(&error));
        }
    }

    #[test]
    fn explicit_and_remote_targets_never_start_local_daemon() {
        for target in [
            ConnectTarget::Socket(socket()),
            ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
        ] {
            let _ = connect(&target, &AtomicBool::new(false), || {
                panic!("attach-only targets must not launch a local daemon")
            });
        }
    }

    #[test]
    fn missing_executable_is_actionable() {
        let error = connect_or_start(
            &socket(),
            &AtomicBool::new(false),
            Duration::ZERO,
            || Command::new("/nonexistent/herdr-gpui-test"),
            true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(is_missing_installation(&error));
        assert!(error.to_string().contains("Could not start herdr server"));
    }

    #[test]
    fn starts_and_connects_to_socket() {
        let path = socket();
        let mut listener = None;
        let stream = connect_or_start(
            &path,
            &AtomicBool::new(false),
            Duration::from_secs(1),
            || {
                listener = Some(UnixListener::bind(&path).unwrap());
                Command::new("/usr/bin/true")
            },
            true,
        )
        .unwrap();
        drop(stream);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn startup_wait_is_bounded() {
        let error = connect_or_start(
            &socket(),
            &AtomicBool::new(false),
            Duration::ZERO,
            || Command::new("/usr/bin/true"),
            true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}
