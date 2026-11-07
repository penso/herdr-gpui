use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::{Duration, Instant},
};

use super::*;

const WAIT: Duration = Duration::from_secs(10);

struct Address {
    path: PathBuf,
    #[cfg(unix)]
    _dir: tempfile::TempDir,
}

fn address() -> Address {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let unique = format!(
        "herdr-e-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    #[cfg(unix)]
    {
        let dir = tempfile::tempdir().unwrap();
        Address {
            path: dir.path().join("p").join(format!("{unique}.sock")),
            _dir: dir,
        }
    }
    #[cfg(windows)]
    Address {
        path: PathBuf::from(format!(r"\\.\pipe\{unique}")),
    }
}

fn pair(address: &Address) -> (Listener, Connection, Connection) {
    let listener = Listener::bind(&address.path).unwrap();
    let path = address.path.clone();
    let client = thread::spawn(move || Connection::connect(&path, WAIT).unwrap());
    let server = listener.accept().unwrap();
    (listener, server, client.join().unwrap())
}

#[test]
fn bytes_flow_both_ways_on_two_threads() {
    let address = address();
    let (_listener, server, client) = pair(&address);
    let (mut server_reader, mut server_writer, _) = server.split().unwrap();
    let (mut client_reader, mut client_writer, _) = client.split().unwrap();
    let echo = thread::spawn(move || {
        let mut buffer = [0; 5];
        server_reader.read_exact(&mut buffer).unwrap();
        server_writer.write_all(&buffer).unwrap();
        server_writer.flush().unwrap();
    });
    client_writer.write_all(b"hello").unwrap();
    client_writer.flush().unwrap();
    let mut buffer = [0; 5];
    client_reader.read_exact(&mut buffer).unwrap();
    assert_eq!(&buffer, b"hello");
    echo.join().unwrap();
}

#[test]
fn a_client_keeps_the_connection_a_waiting_server_accepted() {
    let address = address();
    let listener = Listener::bind(&address.path).unwrap();
    let server = thread::spawn(move || {
        let connection = listener.accept().unwrap();
        // The listener has made its next instance by now; a client that
        // reopened its handle would land there and leave this one closed.
        (listener, connection)
    });
    thread::sleep(Duration::from_millis(50));
    let client = Connection::connect(&address.path, WAIT).unwrap();
    let (_listener, server) = server.join().unwrap();
    let (mut reader, _writer, _closer) = server.split().unwrap();
    let (_client_reader, mut writer, _client_closer) = client.split().unwrap();
    writer.write_all(b"x").unwrap();
    reader.set_timeout(Some(WAIT)).unwrap();
    let mut buffer = [0; 1];
    assert_eq!(reader.read(&mut buffer).unwrap(), 1);
    assert_eq!(&buffer, b"x");
}

#[test]
fn closing_ends_a_blocked_read() {
    let address = address();
    let (_listener, server, _client) = pair(&address);
    let (mut reader, _writer, closer) = server.split().unwrap();
    let blocked = thread::spawn(move || {
        let mut buffer = [0; 1];
        reader.read(&mut buffer).unwrap()
    });
    thread::sleep(Duration::from_millis(50));
    closer.close();
    assert_eq!(blocked.join().unwrap(), 0);
}

#[test]
fn the_peer_going_away_reads_as_the_end() {
    let address = address();
    let (_listener, server, client) = pair(&address);
    let (mut reader, _writer, _closer) = server.split().unwrap();
    drop(client);
    let mut buffer = [0; 1];
    assert_eq!(reader.read(&mut buffer).unwrap(), 0);
}

#[test]
fn a_read_timeout_is_honoured() {
    let address = address();
    let (_listener, server, _client) = pair(&address);
    let (mut reader, _writer, _closer) = server.split().unwrap();
    reader.set_timeout(Some(Duration::from_millis(50))).unwrap();
    let started = Instant::now();
    let mut buffer = [0; 1];
    let error = reader.read(&mut buffer).unwrap_err();
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
    assert!(started.elapsed() < WAIT);
}

#[test]
fn a_second_listener_cannot_take_the_address() {
    let address = address();
    let _first = Listener::bind(&address.path).unwrap();
    assert!(Listener::bind(&address.path).is_err());
}

#[cfg(unix)]
#[test]
fn the_socket_and_its_directory_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let address = address();
    let _listener = Listener::bind(&address.path).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(address.path.parent().unwrap()), 0o700);
    assert_eq!(mode(&address.path), 0o600);
    release(&address.path);
    assert!(!address.path.exists());
}

#[test]
fn nothing_listening_fails_to_connect() {
    let address = address();
    assert!(Connection::connect(&address.path, Duration::from_millis(50)).is_err());
}
