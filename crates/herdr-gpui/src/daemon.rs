#![forbid(unsafe_code)]

use herdr_client::{ConnectTarget, Stream};
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod warning;
use std::{
    env, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use warning::LocalPeerWarning;

pub(crate) enum LocalPeer {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    Trusted,
    Unverified,
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    Rejected(LocalPeerWarning),
}

#[derive(Debug, thiserror::Error)]
#[error("Could not start herdr server: {0}. Install Herdr and use Terminal > Reconnect.")]
struct MissingInstallation(#[source] io::Error);

pub(super) fn is_missing_installation(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<MissingInstallation>())
}

pub fn connect(
    target: &ConnectTarget,
    stop: &AtomicBool,
    on_start: impl FnOnce(),
) -> io::Result<(Stream, LocalPeer)> {
    let socket = target
        .socket_path()
        .map_err(|error| io::Error::new(error.kind(), error))?;
    let stream = connect_or_start(
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
            crate::login_env::apply(&mut command);
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
    )?;
    let local = local_peer(&stream, target, &socket);
    Ok((stream, local))
}

/// Windows resolves a bare command name to `herdr.exe`; an explicit probe has to
/// name the extension itself.
const NAME: &str = if cfg!(windows) { "herdr.exe" } else { "herdr" };

pub(crate) fn executable() -> PathBuf {
    // Finder launches have a minimal PATH, which often omits Homebrew and Cargo.
    let path = env::var_os("PATH").unwrap_or_default();
    let candidates = env::split_paths(&path)
        .map(|dir| dir.join(NAME))
        .chain(
            env::var_os("HOME")
                .or_else(|| cfg!(windows).then(|| env::var_os("USERPROFILE")).flatten())
                .into_iter()
                .flat_map(|home| {
                    let home = PathBuf::from(home);
                    [
                        home.join(".local").join("bin").join(NAME),
                        home.join(".cargo").join("bin").join(NAME),
                    ]
                }),
        )
        .chain(
            [
                PathBuf::from("/opt/homebrew/bin/herdr"),
                PathBuf::from("/usr/local/bin/herdr"),
            ]
            .into_iter()
            .filter(|_| cfg!(unix)),
        );
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| NAME.into())
}

/// Trust the user's standard local endpoint, not an upgrade-sensitive executable.
/// A same-user proxy deliberately installed at that endpoint is within this trust
/// boundary; this is not remote-origin attestation.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn local_peer(stream: &Stream, target: &ConnectTarget, socket: &Path) -> LocalPeer {
    // Remote targets have no local endpoint; that is not worth reporting.
    let Ok(expected) = target.local_session_socket_path() else {
        return LocalPeer::Unverified;
    };
    match peer_matches_local_endpoint(stream, socket, &expected) {
        Ok(()) => LocalPeer::Trusted,
        Err(reason) => {
            // Local Git actions and reviews disappear without this; name the
            // failed check, not the user's paths.
            tracing::info!(?reason, "Daemon endpoint not trusted as local");
            match reason {
                // Explicit/forwarded sockets need not name a local session.
                // Their absence from the local trust boundary is not a fault
                // the user can fix with permissions.
                UntrustedEndpoint::NotSocket | UntrustedEndpoint::OtherSocket => {
                    LocalPeer::Unverified
                }
                _ => LocalPeer::Rejected(LocalPeerWarning::new(reason, expected)),
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn local_peer(_stream: &Stream, _target: &ConnectTarget, _socket: &Path) -> LocalPeer {
    LocalPeer::Unverified
}

/// Which local endpoint check refused the connection.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum UntrustedEndpoint {
    /// The peer is another user, or its credentials could not be read.
    #[error("The daemon's user could not be verified as your user.")]
    PeerUser,
    /// The standard session socket is missing or is not a socket (e.g. a symlink).
    #[error("The standard local endpoint is missing or is not a socket.")]
    NotSocket,
    /// The dialed socket is not the standard session socket.
    #[error("This is not the standard local session socket.")]
    OtherSocket,
    /// The socket is not owned by this user or is group/world-writable.
    #[error("The daemon socket has untrusted ownership or write permissions.")]
    SocketPermissions,
    /// The socket's directory is not owned by this user or is world-writable.
    #[error("The daemon socket directory must be owned by your user and not world-writable.")]
    DirectoryPermissions,
}

/// The user ID of the process at the other end of the socket.
#[cfg(target_os = "macos")]
fn peer_uid(stream: &Stream) -> nix::Result<nix::unistd::Uid> {
    nix::unistd::getpeereid(stream).map(|(uid, _)| uid)
}

/// The user ID of the process at the other end of the socket.
#[cfg(target_os = "linux")]
fn peer_uid(stream: &Stream) -> nix::Result<nix::unistd::Uid> {
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    getsockopt(stream, PeerCredentials)
        .map(|credentials| nix::unistd::Uid::from_raw(credentials.uid()))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn peer_matches_local_endpoint(
    stream: &Stream,
    socket: &Path,
    expected: &Path,
) -> Result<(), UntrustedEndpoint> {
    check_local_endpoint(socket, expected, || peer_uid(stream))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn check_local_endpoint(
    socket: &Path,
    expected: &Path,
    peer_uid: impl FnOnce() -> nix::Result<nix::unistd::Uid>,
) -> Result<(), UntrustedEndpoint> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    // Do not allow the standard socket itself to redirect to another location.
    if !std::fs::symlink_metadata(expected).is_ok_and(|metadata| metadata.file_type().is_socket()) {
        return Err(UntrustedEndpoint::NotSocket);
    }
    let expected = expected
        .canonicalize()
        .map_err(|_| UntrustedEndpoint::NotSocket)?;
    // Use the path actually dialed, not peer_addr(): BSD sockaddr lengths from
    // some listeners omit the NUL and std can truncate the reported pathname.
    if !socket.canonicalize().is_ok_and(|socket| socket == expected) {
        return Err(UntrustedEndpoint::OtherSocket);
    }
    // Only diagnose the local daemon after identifying its endpoint. A foreign
    // peer on an intentionally non-local socket cannot be fixed by restarting it.
    let uid = nix::unistd::geteuid();
    if !peer_uid().is_ok_and(|peer| peer == uid) {
        return Err(UntrustedEndpoint::PeerUser);
    }
    let parent = expected.parent().ok_or(UntrustedEndpoint::OtherSocket)?;
    let owned = |metadata: &std::fs::Metadata| {
        metadata.uid() == uid.as_raw() && metadata.mode() & 0o022 == 0
    };
    if !std::fs::symlink_metadata(&expected)
        .is_ok_and(|metadata| metadata.file_type().is_socket() && owned(&metadata))
    {
        return Err(UntrustedEndpoint::SocketPermissions);
    }
    // A normal 002 umask creates 0775 directories. Group write alone does not
    // attest a peer: its kernel UID and the socket's stricter permissions were
    // checked above. Still require this user's directory and reject world write.
    if !std::fs::metadata(parent).is_ok_and(|metadata| {
        metadata.is_dir() && metadata.uid() == uid.as_raw() && metadata.mode() & 0o002 == 0
    }) {
        return Err(UntrustedEndpoint::DirectoryPermissions);
    }
    Ok(())
}

/// The daemon outlives this window, so it must not share the GUI's signal or
/// console group: closing or quitting the GUI cannot take the server with it.
#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    // The server is a console program; launched from the GUI it must not flash one.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

fn connect_or_start(
    socket: &Path,
    stop: &AtomicBool,
    timeout: Duration,
    command: impl FnOnce() -> Command,
    auto_start: bool,
) -> io::Result<Stream> {
    match Stream::connect(socket) {
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
            crate::Error::DaemonCancelled,
        ));
    }
    let mut command = command();
    // Login-shell initialization can outlast a detach or reconnect.
    if stop.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            crate::Error::DaemonCancelled,
        ));
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            io::Error::new(error.kind(), MissingInstallation(error))
        } else {
            io::Error::new(error.kind(), crate::Error::DaemonSpawn { source: error })
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
                crate::Error::DaemonCancelled,
            ));
        }
        match Stream::connect(socket) {
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
                crate::Error::DaemonTimeout(socket.to_owned()),
            ));
        }
        if let Ok(status) = exit_rx.try_recv() {
            return Err(io::Error::other(crate::Error::DaemonExited(status?)));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

// The fixtures bind real sockets and launch POSIX helper executables.
#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
