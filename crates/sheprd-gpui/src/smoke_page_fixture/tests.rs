#![allow(clippy::unwrap_used)]

use super::*;
use core::prelude::v1::test;
use std::{
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::mpsc,
};

#[test]
fn an_idle_preconnection_does_not_block_the_page_request() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted, ready) = mpsc::sync_channel(1);
    let server = std::thread::spawn(move || {
        let (idle, _) = listener.accept().unwrap();
        accepted.send(()).unwrap();
        let error = respond(idle).unwrap_err();
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ));
        let (request, _) = listener.accept().unwrap();
        respond(request).unwrap();
    });

    let idle = TcpStream::connect(address).unwrap();
    ready.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut request = TcpStream::connect(address).unwrap();
    request
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    request
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    let result = request.read_to_string(&mut response);
    // Release the idle socket even on failure so the server can be joined.
    drop(idle);
    server.join().unwrap();
    result.unwrap();
    assert_eq!(
        response,
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
            PAGE.len()
        )
    );
}
