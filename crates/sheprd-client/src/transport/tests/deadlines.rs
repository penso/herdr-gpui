use super::*;
use std::{
    io,
    sync::mpsc::channel,
    thread,
    time::{Duration, Instant},
};

const CHUNK: usize = overlapped::CHUNK;

fn bytes(len: usize) -> Vec<u8> {
    (0..len).map(|index| (index % 251) as u8).collect()
}

fn timed_out(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::TimedOut
}

/// Write `data` until the peer stops taking it, as the image writer does: one
/// call per attempt, offset advanced only by what each call reports.
fn fill(stream: &mut Stream, data: &[u8]) -> usize {
    let mut offset = 0;
    while offset < data.len() {
        match stream.write(&data[offset..]) {
            Ok(written) => offset += written,
            Err(error) if timed_out(&error) => return offset,
            Err(error) => panic!("{error}"),
        }
    }
    offset
}

#[test]
fn a_write_to_a_peer_that_never_reads_ends_at_its_deadline() {
    let (mut client, _server) = Stream::pair().unwrap();
    client
        .set_write_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let data = bytes(16 * CHUNK);
    let start = Instant::now();
    let error = client.write_all(&data).unwrap_err();
    assert!(timed_out(&error), "{error}");
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(50), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    // A flush waits for the chunk in flight under the same deadline.
    let start = Instant::now();
    assert!(timed_out(&client.flush().unwrap_err()));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn timed_out_writes_neither_lose_nor_repeat_bytes() {
    let (mut client, mut server) = Stream::pair().unwrap();
    client
        .set_write_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let data = bytes(12 * CHUNK + 1234);
    // Fill the pipe until a write times out, then let the peer drain it, and
    // repeat: every resumed write starts exactly where the last one ended.
    let (drain, drained) = channel::<usize>();
    let reader = thread::spawn(move || {
        let mut received = Vec::new();
        while let Ok(len) = drained.recv() {
            let mut chunk = vec![0; len];
            server.read_exact(&mut chunk).unwrap();
            received.extend_from_slice(&chunk);
        }
        received
    });
    let mut offset = 0;
    let mut stalls = 0;
    while offset < data.len() {
        let accepted = fill(&mut client, &data[offset..]);
        if accepted < data.len() - offset {
            stalls += 1;
        }
        offset += accepted;
        // Release what was reported written; any of it may still be in flight.
        if accepted > 0 {
            drain.send(accepted).unwrap();
        }
        client
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        client.flush().unwrap();
        client
            .set_write_timeout(Some(Duration::from_millis(20)))
            .unwrap();
    }
    drop(drain);
    assert!(stalls > 0, "the pipe never filled");
    assert_eq!(reader.join().unwrap(), data);
}

#[test]
fn dropping_a_stream_mid_frame_cancels_without_waiting_for_the_peer() {
    let (mut client, mut server) = Stream::pair().unwrap();
    client
        .set_write_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let data = bytes(16 * CHUNK);
    let written = fill(&mut client, &data);
    assert!(written < data.len(), "the pipe never filled");
    let start = Instant::now();
    drop(client);
    assert!(start.elapsed() < Duration::from_secs(5));
    // The peer gets a prefix of the frame, then end of stream: never the rest
    // of it and never anything after it.
    server
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut received = Vec::new();
    server.read_to_end(&mut received).unwrap();
    assert!(received.len() <= written, "{} > {written}", received.len());
    assert_eq!(received, data[..received.len()]);
}

#[test]
fn a_read_deadline_keeps_the_read_and_its_later_bytes() {
    let (mut client, mut server) = Stream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    let start = Instant::now();
    assert!(timed_out(&client.read(&mut [0; 16]).unwrap_err()));
    assert!(start.elapsed() >= Duration::from_millis(30));
    // Bytes that arrive for the read left in flight are the next ones read.
    server.write_all(b"after the deadline").unwrap();
    server.flush().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut received = [0; 18];
    client.read_exact(&mut received).unwrap();
    assert_eq!(&received, b"after the deadline");
    drop(server);
    assert_eq!(client.read(&mut [0; 1]).unwrap(), 0);
    assert_eq!(client.read(&mut [0; 1]).unwrap(), 0, "end of stream stays");
}

#[test]
fn a_zero_timeout_is_refused_as_on_a_socket() {
    let (mut client, _server) = Stream::pair().unwrap();
    for result in [
        client.set_read_timeout(Some(Duration::ZERO)),
        client.set_write_timeout(Some(Duration::ZERO)),
    ] {
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }
}

#[test]
fn a_bridge_child_streams_both_ways_and_its_exit_reads_as_closed() {
    let (mut stream, stdin, stdout) = Stream::child().unwrap();
    // An unbuffered copy of standard input to standard output.
    let mut command = std::process::Command::new("powershell.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OpenStandardInput().CopyTo([Console]::OpenStandardOutput())",
        ])
        .stdin(stdin)
        .stdout(stdout)
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().unwrap();
    drop(command);
    // More than both pipes hold: alternate bounded writes and reads, as the
    // session loop does, or the child would block on its full output.
    let sent = bytes(3 * CHUNK);
    stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let mut offset = 0;
    let mut echoed = Vec::new();
    let mut buffer = [0; 8192];
    let start = Instant::now();
    while echoed.len() < sent.len() {
        assert!(start.elapsed() < Duration::from_secs(30), "echo stalled");
        if offset < sent.len() {
            match stream.write(&sent[offset..]) {
                Ok(written) => offset += written,
                Err(error) if timed_out(&error) => {}
                Err(error) => panic!("{error}"),
            }
        }
        match stream.read(&mut buffer) {
            Ok(0) => panic!("the child closed early"),
            Ok(read) => echoed.extend_from_slice(&buffer[..read]),
            Err(error) if timed_out(&error) => {}
            Err(error) => panic!("{error}"),
        }
    }
    assert_eq!(echoed, sent);
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    // The child's exit closes its output: end of stream, not a timeout.
    child.kill().unwrap();
    child.wait().unwrap();
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty());
}

#[test]
fn a_bridge_child_that_never_reads_cannot_hold_a_write() {
    let (mut stream, stdin, stdout) = Stream::child().unwrap();
    stream
        .set_write_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    // `ping` never reads its input.
    let mut command = std::process::Command::new("ping.exe");
    command
        .args(["-n", "30", "127.0.0.1"])
        .stdin(stdin)
        .stdout(stdout)
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().unwrap();
    drop(command);
    let start = Instant::now();
    assert!(timed_out(
        &stream.write_all(&bytes(16 * CHUNK)).unwrap_err()
    ));
    assert!(start.elapsed() < Duration::from_secs(5));
    child.kill().unwrap();
    child.wait().unwrap();
    let start = Instant::now();
    drop(stream);
    assert!(start.elapsed() < Duration::from_secs(5));
}
