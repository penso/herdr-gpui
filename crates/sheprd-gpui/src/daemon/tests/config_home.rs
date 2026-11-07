use super::*;
use std::ffi::OsStr;

fn config_home(command: &Command) -> Option<Option<&OsStr>> {
    command
        .get_envs()
        .find(|(key, _)| *key == "XDG_CONFIG_HOME")
        .map(|(_, value)| value)
}

/// A started daemon takes the root its socket was resolved under, replacing
/// whatever the login environment set, so it listens where the app polls.
#[test]
fn a_started_daemon_uses_the_resolved_config_home() {
    let mut command = Command::new("herdr");
    command.env("XDG_CONFIG_HOME", "/shell/config");
    pin_config_home(&mut command, Some("/adopted".into()));
    assert_eq!(config_home(&command), Some(Some(OsStr::new("/adopted"))));
}

/// Without a resolved root the app polls the default one, so a login value
/// is removed rather than sending the daemon elsewhere.
#[test]
fn without_a_resolved_config_home_the_login_one_is_removed() {
    let mut command = Command::new("herdr");
    command.env("XDG_CONFIG_HOME", "/shell/config");
    pin_config_home(&mut command, None);
    assert_eq!(config_home(&command), Some(None));
}

/// Leaves a socket file nothing accepts on. A dropped `UnixListener` is not
/// enough on macOS: it marks a new socket close-on-exec only after creating
/// it, so a child another test thread spawns in between inherits the listener
/// and keeps accepting. A socket that never listens refuses even if leaked.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn stale_socket(path: &Path) {
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, UnixAddr, bind, socket};
    use std::os::fd::AsRawFd;
    let fd = socket(
        AddressFamily::Unix,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )
    .unwrap();
    bind(fd.as_raw_fd(), &UnixAddr::new(path).unwrap()).unwrap();
}

/// A stale default socket is refused rather than trusted, so the daemon is
/// started and the wait connects to it at the same path.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_stale_socket_is_replaced_by_a_daemon_at_the_polled_path() {
    let path = socket();
    stale_socket(&path);
    assert!(path.exists(), "a closed socket leaves its file");
    let mut listener = None;
    let stream = connect_or_start(
        &path,
        &AtomicBool::new(false),
        Duration::from_secs(1),
        || {
            // As `herdr server` does: replace the stale file and listen.
            std::fs::remove_file(&path).unwrap();
            listener = Some(UnixListener::bind(&path).unwrap());
            Command::new("/usr/bin/true")
        },
        true,
    )
    .unwrap();
    assert!(listener.is_some(), "the stale socket must start a daemon");
    drop(stream);
    drop(listener);
    std::fs::remove_file(path).unwrap();
}
