use super::*;
use std::os::unix::{
    fs::PermissionsExt as _,
    net::{UnixListener, UnixStream},
};

#[test]
fn only_the_user_can_reach_the_socket() {
    let address = address();
    let _server = Server::bind(&address.path).unwrap();
    let mode = std::fs::metadata(&address.path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn a_stale_socket_is_replaced() {
    let address = address();
    // A socket file nobody listens on, as a crashed app leaves behind.
    drop(UnixListener::bind(&address.path).unwrap());
    // A process a parallel test forks meanwhile holds a copy of the
    // listener until it execs; wait until nothing answers on it.
    let deadline = Instant::now() + Duration::from_secs(5);
    while UnixStream::connect(&address.path).is_ok() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    Server::bind(&address.path).unwrap();
}
