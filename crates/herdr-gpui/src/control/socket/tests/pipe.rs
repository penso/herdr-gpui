use super::*;
use interprocess::os::windows::named_pipe::{PipeListenerOptions, pipe_mode};
use std::os::windows::io::AsHandle as _;
use transport::security::{owner_sid, user_sid};

#[test]
fn the_pipe_is_named_for_the_user_and_an_explicit_state_directory() {
    let plain = transport::name("S-1-5-21-1", None);
    assert_eq!(
        plain,
        PathBuf::from(r"\\.\pipe\herdr-gpui-control-S-1-5-21-1")
    );
    assert_ne!(transport::name("S-1-5-21-2", None), plain);
    let state = Path::new(r"C:\state\a");
    let isolated = transport::name("S-1-5-21-1", Some(state));
    assert_ne!(isolated, plain);
    // The same directory names the same pipe in every process.
    assert_eq!(transport::name("S-1-5-21-1", Some(state)), isolated);
    // However the shell spells it.
    for spelling in ["C:/state/a", r"c:\STATE\a\", "C:/state/a/"] {
        assert_eq!(
            transport::name("S-1-5-21-1", Some(Path::new(spelling))),
            isolated,
            "{spelling}"
        );
    }
    assert_ne!(
        transport::name("S-1-5-21-1", Some(Path::new(r"C:\state\b"))),
        isolated
    );
}

#[test]
fn the_pipe_belongs_to_the_user_who_started_the_server() {
    let address = address();
    let _server = Server::bind(&address.path).unwrap();
    let stream = transport::connect(&address.path, IO_TIMEOUT, IO_TIMEOUT).unwrap();
    let user = user_sid().unwrap();
    assert!(user.starts_with("S-1-"), "{user}");
    assert_eq!(owner_sid(stream.pipe.as_handle()).unwrap(), user);
}

#[test]
fn the_server_never_joins_a_pipe_someone_else_created() {
    let address = address();
    // Another program's pipe of the same name, with default security.
    let _other = PipeListenerOptions::new()
        .path(address.path.as_path())
        .create_duplex::<pipe_mode::Bytes>()
        .unwrap();
    // Default security makes the token's default owner the pipe's owner: the
    // user, so it reads as a server already running, or, in an elevated
    // process, Administrators, so it reads as someone else's. Either way the
    // server refuses rather than joining it.
    let refused = Server::bind(&address.path);
    assert!(
        matches!(
            refused,
            Err(
                crate::Error::Control(crate::control::Error::SocketInUse { .. })
                    | crate::Error::Control(crate::control::Error::PipeForeign { .. })
            )
        ),
        "{:?}",
        refused.err()
    );
}

#[test]
fn a_silent_server_times_the_read_out() {
    let (_address, server, _client) = pair();
    let started = Instant::now();
    let error = read_line(&server).unwrap_err();
    assert!(
        matches!(&error, crate::Error::Io(source) if source.kind() == std::io::ErrorKind::TimedOut),
        "{error:?}"
    );
    assert!(started.elapsed() >= IO_TIMEOUT);
    assert!(started.elapsed() < IO_TIMEOUT * 3);
}

#[test]
fn a_waiting_peer_reads_as_open_until_it_hangs_up() {
    let (_address, server, client) = pair();
    assert!(!server.peer_closed());
    drop(client);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !server.peer_closed() {
        assert!(Instant::now() < deadline, "the hang-up was not seen");
        thread::sleep(Duration::from_millis(10));
    }
}
