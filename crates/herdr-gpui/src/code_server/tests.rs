#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    io::Write,
    net::{SocketAddr, TcpListener},
    thread,
};

const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

/// A server that answers each request with the next of `responses`, then
/// closes; returns its address.
fn serve(responses: Vec<String>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    address
}

fn url(address: SocketAddr) -> WebUrl {
    WebUrl::try_from(format!("http://{address}/?tkn=secret").as_str()).unwrap()
}

fn version(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn a_version_body_is_a_commit() {
    let server = Server::from_version(&format!("{COMMIT}\n")).unwrap();
    assert_eq!(server.short_commit(), "2a59476");
    for invalid in [
        "",
        "1.105.0",
        &COMMIT[..39],
        &COMMIT.to_uppercase(),
        &format!("{COMMIT}0"),
        "<html>",
    ] {
        assert_eq!(Server::from_version(invalid), None, "{invalid}");
    }
}

#[test]
fn a_server_that_takes_the_token_is_vs_code() {
    let redirect =
        "HTTP/1.1 302 Found\r\nlocation: /\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    let address = serve(vec![version(COMMIT), redirect.into()]);
    assert_eq!(probe(&url(address)).unwrap().short_commit(), "2a59476");
}

#[test]
fn a_refused_token_is_told_apart_from_a_missing_server() {
    let forbidden =
        "HTTP/1.1 403 Forbidden\r\ncontent-length: 10\r\nconnection: close\r\n\r\nForbidden.";
    let address = serve(vec![version(COMMIT), forbidden.into()]);
    assert!(matches!(probe(&url(address)), Err(Error::CodeTokenRefused)));

    let missing = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    let address = serve(vec![missing.into()]);
    assert!(matches!(
        probe(&url(address)),
        Err(Error::CodeNotServer { status: 404 })
    ));

    let address = serve(vec![version("hello")]);
    assert!(matches!(
        probe(&url(address)),
        Err(Error::CodeNotServer { status: 200 })
    ));
}

#[test]
fn a_closed_port_is_unreachable() {
    // Nothing listens on port 1. A freed ephemeral port could be taken by a
    // parallel test's server meanwhile, so it is not used here.
    let error = probe(&WebUrl::try_from("http://127.0.0.1:1/?tkn=secret").unwrap()).unwrap_err();
    let Error::CodeUnreachable { address, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(address, "127.0.0.1:1");
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn an_unreachable_server_is_named_by_host_and_port_alone() {
    let refused = ureq::Error::Io(std::io::ErrorKind::ConnectionRefused.into());
    assert_eq!(NoAnswer::from(&refused), NoAnswer::Refused);
    assert_eq!(
        NoAnswer::from(&ureq::Error::HostNotFound),
        NoAnswer::UnknownHost
    );
    assert_eq!(
        NoAnswer::from(&ureq::Error::ConnectionFailed),
        NoAnswer::Refused
    );
    assert_eq!(NoAnswer::Timeout.to_string(), "No answer within 3 seconds");
    let url = WebUrl::try_from("http://127.0.0.1:8000/?tkn=secret").unwrap();
    let error = Error::CodeUnreachable {
        address: url.address(),
        reason: NoAnswer::from(&refused),
        source: refused,
    };
    assert_eq!(error.to_string(), "Connection refused at 127.0.0.1:8000.");
}
