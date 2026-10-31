//! Shared fixtures: a scripted core on the other end of a socket pair.

mod backend;
mod connection;
mod endpoint;
mod event;
mod frame;
mod hello;
mod live;

use crate::frame::{Decoder, Frame, Kind, encode};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

pub(crate) fn hello_reply(version: &str, methods: &[&str]) -> Value {
    json!({"jsonrpc": "2.0", "id": 0, "result": {
        "generation": 1, "version": version, "framed": true, "features": [],
        "core": {"id": "c0ffee", "name": "box"}, "methods": methods}})
}

pub(crate) fn every_method() -> Vec<&'static str> {
    crate::Method::ALL
        .iter()
        .map(|method| method.as_str())
        .collect()
}

pub(crate) fn terminal(address: &str) -> Value {
    json!({"address": address, "labels": {}, "argv": ["sh"], "cwd": "/tmp", "state": "live",
        "created_ms": 1, "screen": "", "cols": 80, "rows": 24})
}

#[cfg(unix)]
pub(crate) type Socket = std::os::unix::net::UnixStream;
#[cfg(windows)]
pub(crate) type Socket = herdr_client::Stream;

#[cfg(unix)]
pub(crate) fn pair() -> (Socket, Socket) {
    let (client, server) = Socket::pair().unwrap();
    for end in [&client, &server] {
        end.set_read_timeout(Some(crate::connection::POLL)).unwrap();
    }
    (client, server)
}

#[cfg(windows)]
pub(crate) fn pair() -> (Socket, Socket) {
    let (mut client, mut server) = Socket::pair().unwrap();
    for end in [&mut client, &mut server] {
        end.set_read_timeout(Some(crate::connection::POLL)).unwrap();
    }
    (client, server)
}

/// The core's side: answers the handshake, then reads and writes frames.
pub(crate) struct MockCore {
    decoder: Decoder,
    socket: Socket,
}

impl MockCore {
    /// Reads the client's hello line and answers with `reply`.
    pub(crate) fn accept(mut socket: Socket, reply: &Value) -> (Self, Value) {
        let mut line = Vec::new();
        let mut byte = [0];
        let deadline = Instant::now() + Duration::from_secs(5);
        while !line.ends_with(b"\n") {
            assert!(Instant::now() < deadline, "no hello");
            if let Ok(1) = socket.read(&mut byte) {
                line.push(byte[0]);
            }
        }
        let hello = serde_json::from_slice(&line).unwrap();
        let mut answer = serde_json::to_vec(reply).unwrap();
        answer.push(b'\n');
        socket.write_all(&answer).unwrap();
        (
            Self {
                decoder: Decoder::default(),
                socket,
            },
            hello,
        )
    }

    pub(crate) fn frame(&mut self) -> Frame {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut buffer = [0; 4096];
        loop {
            if let Some(frame) = self.decoder.next_frame().unwrap() {
                return frame;
            }
            assert!(Instant::now() < deadline, "no frame");
            if let Ok(read) = self.socket.read(&mut buffer) {
                assert!(read > 0, "client closed");
                self.decoder.push(&buffer[..read]);
            }
        }
    }

    /// The next call on channel 0, skipping credit and other stream frames.
    pub(crate) fn request(&mut self) -> Value {
        loop {
            let frame = self.frame();
            if frame.channel == 0 && frame.kind == Kind::Data {
                return serde_json::from_slice(&frame.payload).unwrap();
            }
        }
    }

    pub(crate) fn send(&mut self, channel: u32, kind: Kind, payload: &[u8]) {
        self.socket
            .write_all(&encode(channel, kind, payload).unwrap())
            .unwrap();
    }

    pub(crate) fn answer(&mut self, request: &Value, result: Value) {
        let message = json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
        self.send(0, Kind::Data, &serde_json::to_vec(&message).unwrap());
    }
}

/// A client connected to a scripted core that speaks every method.
pub(crate) fn connected() -> (crate::Core, MockCore) {
    let (client, server) = pair();
    let core = std::thread::spawn(move || {
        MockCore::accept(server, &hello_reply("0.1.13", &every_method()))
    });
    let client = crate::Core::over(client).unwrap();
    let (core, _) = core.join().unwrap();
    (client, core)
}
