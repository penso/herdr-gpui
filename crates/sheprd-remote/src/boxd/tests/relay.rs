//! The relay against a scripted exec stream on loopback: a server that, like
//! boxd's, answers while the request body is still open.
#![allow(clippy::unwrap_used)]

use super::super::{
    relay::{open, relay},
    wire::{self, Field, Message, fields},
};
use crate::cloud::socket::input;
use secrecy::SecretString;
use std::{
    io::{BufRead, BufReader, Cursor, Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

/// What the server saw: the request head, the start message, and stdin.
struct Seen {
    head: String,
    start: Vec<u8>,
    stdin: Vec<u8>,
}

fn chunk(out: &mut impl Write, bytes: &[u8]) {
    write!(out, "{:x}\r\n", bytes.len()).unwrap();
    out.write_all(bytes).unwrap();
    out.write_all(b"\r\n").unwrap();
    out.flush().unwrap();
}

/// Echo stdin back as stdout chunk by chunk, write `warning` to stderr, and
/// end with `exit` and `status` once stdin ends.
fn serve(exit: u64, status: u32) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        writer
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: application/grpc-web+proto\r\ntransfer-encoding: chunked\r\n\r\n")
            .unwrap();
        let mut body = Vec::new();
        let mut start = None;
        let mut stdin = Vec::new();
        loop {
            let mut size = String::new();
            reader.read_line(&mut size).unwrap();
            let size = usize::from_str_radix(size.trim(), 16).unwrap();
            let mut data = vec![0; size + 2];
            reader.read_exact(&mut data).unwrap();
            if size == 0 {
                break;
            }
            body.extend_from_slice(&data[..size]);
            while let Some((_, payload, used)) = wire::take_frame(&body).unwrap() {
                let message = payload.to_vec();
                body.drain(..used);
                if start.is_none() {
                    start = Some(message);
                    let warning = Message::default().bytes(1, b"warning").bool(10, true);
                    chunk(&mut writer, &wire::frame(&warning.into_bytes()));
                    continue;
                }
                let data = fields(&message)
                    .unwrap()
                    .into_iter()
                    .find_map(|(n, f)| match f {
                        Field::Bytes(value) if n == 1 => Some(value.to_vec()),
                        _ => None,
                    })
                    .unwrap();
                stdin.extend_from_slice(&data);
                chunk(
                    &mut writer,
                    &wire::frame(&Message::default().bytes(1, &data).into_bytes()),
                );
            }
        }
        let mut code = Vec::new();
        if exit != 0 {
            code.extend_from_slice(&[0x30, exit as u8]);
        }
        chunk(&mut writer, &wire::frame(&code));
        let trailer = format!("grpc-status:{status}\r\ngrpc-message:boom\r\n");
        let mut frame = vec![wire::TRAILER];
        frame.extend_from_slice(&(trailer.len() as u32).to_be_bytes());
        frame.extend_from_slice(trailer.as_bytes());
        chunk(&mut writer, &frame);
        writer.write_all(b"0\r\n\r\n").unwrap();
        let _ = sent.send(Seen {
            head,
            start: start.unwrap(),
            stdin,
        });
    });
    (base, received)
}

fn run(base: &str, input_bytes: Vec<u8>) -> (super::super::Result<u8>, Vec<u8>, Vec<u8>) {
    let settings = super::settings(base);
    let key = SecretString::from("fixture-key");
    let mut connection = open(&settings, &key, "m-1", "/bin/sh -c 'cat'").unwrap();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    let code = relay(
        &mut connection,
        input(Cursor::new(input_bytes)),
        &mut stdout,
        &mut stderr,
    );
    (code, stdout, stderr)
}

#[test]
fn stdin_goes_up_in_chunks_and_output_comes_back_split_by_stream() {
    let (base, seen) = serve(7, 0);
    let bytes: Vec<u8> = (0..=255u8).cycle().take(200_000).collect();
    let (code, stdout, stderr) = run(&base, bytes.clone());
    assert_eq!(code.unwrap(), 7);
    assert_eq!(stdout, bytes, "binary input survives the trip both ways");
    assert_eq!(stderr, b"warning");
    let seen = seen.recv().unwrap();
    assert_eq!(seen.stdin, bytes);
    assert!(seen.head.contains("Authorization: Bearer fixture-key\r\n"));
    assert_eq!(seen.start, wire::exec_start("m-1", "/bin/sh -c 'cat'"));
}

#[test]
fn empty_stdin_still_ends_and_exit_zero_is_the_default() {
    let (base, seen) = serve(0, 0);
    let (code, stdout, _) = run(&base, Vec::new());
    assert_eq!(code.unwrap(), 0);
    assert!(stdout.is_empty());
    assert!(seen.recv().unwrap().stdin.is_empty());
}

#[test]
fn a_failed_call_is_an_error_not_an_exit_code() {
    let (base, _) = serve(0, 5);
    let (code, _, _) = run(&base, b"x".to_vec());
    let error = code.unwrap_err();
    assert_eq!(
        error.to_string(),
        "boxd request failed (gRPC status 5): boom"
    );
}

/// Answer with one stdout message and a body that ends without a status,
/// chunked or of known length, then keep the connection open until the
/// client closes it.
fn serve_untrailed(sized: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        let message = wire::frame(&Message::default().bytes(1, b"partial").into_bytes());
        if sized {
            write!(
                writer,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n",
                message.len()
            )
            .unwrap();
            writer.write_all(&message).unwrap();
        } else {
            writer
                .write_all(b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n")
                .unwrap();
            chunk(&mut writer, &message);
            writer.write_all(b"0\r\n\r\n").unwrap();
        }
        writer.flush().unwrap();
        // Held open: only the client's close ends this read.
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    });
    base
}

#[test]
fn a_reply_without_a_status_fails_rather_than_waiting() {
    for sized in [false, true] {
        let base = serve_untrailed(sized);
        let (done, finished) = mpsc::channel();
        thread::spawn(move || {
            // Stdin stays open, as the bridge's does while it runs.
            let (_keep, input) = mpsc::channel::<Option<Vec<u8>>>();
            let settings = super::settings(&base);
            let key = SecretString::from("fixture-key");
            let mut connection = open(&settings, &key, "m-1", "cat").unwrap();
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            let code = relay(&mut connection, input, &mut stdout, &mut stderr);
            let _ = done.send((code, stdout));
        });
        let (code, stdout) = finished
            .recv_timeout(std::time::Duration::from_secs(20))
            .unwrap_or_else(|_| panic!("the relay waited forever (sized: {sized})"));
        assert!(
            matches!(code, Err(super::super::Error::Wire)),
            "sized: {sized}"
        );
        assert_eq!(stdout, b"partial", "sized: {sized}");
    }
}
