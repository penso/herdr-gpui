use super::*;
use std::path::Path;

/// A state folder too long for a socket path moves the socket to a private
/// folder of this user's instead of failing to bind.
#[test]
fn a_long_state_folder_moves_the_socket() {
    let name = "setup-0123456789abcdef.sock";
    let short = Path::new("/Users/me/.local/state/herdr-gpui");
    assert_eq!(host::socket_dir(short, Path::new("/tmp"), name), short);

    let long = Path::new("/Users/me").join("x".repeat(80));
    let own = format!("herdr-gpui-{}", rustix::process::getuid().as_raw());
    let temp = Path::new("/var/folders/ab/cd/T");
    assert_eq!(host::socket_dir(&long, temp, name), temp.join(&own));
    assert_eq!(
        host::socket_dir(&long, &long, name),
        Path::new("/tmp").join(&own)
    );

    // And it binds there.
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("x".repeat(100));
    let waiting =
        listen::<()>(&dir, Command::new("/usr/bin/true"), Duration::from_secs(5)).unwrap();
    assert!(waiting.path().as_os_str().len() < 104);
    assert!(UnixStream::connect(waiting.path()).is_ok());
}

/// Anyone can create a name in `/tmp` first, so a link there is not used.
#[test]
fn a_linked_socket_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let elsewhere = root.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
    assert!(matches!(
        host::make_private(&link),
        Err(Error::SetupRelay(crate::setup_relay::Error::Dir(path))) if path == link
    ));
    host::make_private(&elsewhere).unwrap();
    assert_eq!(
        std::fs::metadata(&elsewhere).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

/// A pane that stops reading hangs the setup up once a write times out,
/// rather than keeping it and the host's claim until the setup's limit.
#[test]
fn a_pane_that_stops_reading_hangs_the_setup_up() {
    let (mut pane, app) = UnixStream::pair().unwrap();
    let mut command = Command::new("/usr/bin/yes");
    command.process_group(0);
    let started = Instant::now();
    let host = thread::spawn(move || {
        host::relay(
            app,
            command,
            Duration::from_secs(60),
            Duration::from_millis(200),
        )
    });
    Frame::Hello {
        size: Size { rows: 24, cols: 80 },
        term: None,
    }
    .write(&mut pane)
    .unwrap();
    // The pane never reads, and stays connected.
    let status = host.join().unwrap().unwrap();
    assert!(!status.success());
    assert!(started.elapsed() < Duration::from_secs(10));
    drop(pane);
}
