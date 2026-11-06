//! The whole path a Windows caller takes: a request over the named pipe,
//! answered by the app's own handler, back over the pipe.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::{constants::APP_VERSION, control::protocol::PROTOCOL_VERSION};

#[gpui::test]
fn status_round_trips_through_the_pipe(cx: &mut gpui::TestAppContext) {
    let address = socket::test_address();
    let server = socket::Server::bind(&address.path).unwrap();
    let path = address.path.clone();
    let caller =
        std::thread::spawn(move || socket::call(&path, &Request::Status(Caller::default())));
    let incoming = server.next().unwrap();
    let mut state = State::default();
    cx.update(|cx| serve(incoming, &mut state, cx));
    let Response::Status(status) = caller.join().unwrap().unwrap() else {
        panic!("expected a status answer");
    };
    assert_eq!(status.version, APP_VERSION);
    assert_eq!(status.protocol, PROTOCOL_VERSION);
    assert_eq!(status.pid, std::process::id());
    assert!(status.windows.is_empty());
}
