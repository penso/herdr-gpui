//! A second daemon on the same state leaves the first one's control socket
//! alone.

use crate::control::{Request, Response};

#[tokio::test]
async fn a_second_daemon_does_not_take_a_live_control_socket() {
    let dir = tempfile::tempdir().unwrap();
    let state = crate::web::tests::state(dir.path());
    let (stop, _) = tokio::sync::watch::channel(false);
    super::control::serve(dir.path(), state.clone(), stop.clone()).unwrap();
    let address = crate::control::address(dir.path());
    let status = || {
        tokio::task::spawn_blocking({
            let address = address.clone();
            move || crate::control::request(&address, &Request::Status)
        })
    };
    assert!(matches!(status().await.unwrap(), Ok(Response::Status(_))));

    let error = super::control::serve(dir.path(), state, stop).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    // The first daemon still answers.
    assert!(matches!(status().await.unwrap(), Ok(Response::Status(_))));
}
