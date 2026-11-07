//! The wrapper run by a local shell. Unix only: it is a POSIX shell script,
//! and the sandbox it runs in is Linux.

use crate::opensandbox::relay::{frame, wrapper};
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// What execd would pass the command: the frames the relay sends, as bytes.
fn framed(chunks: &[&[u8]], end: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for chunk in chunks {
        bytes.extend_from_slice(&frame(chunk)[1..]);
    }
    bytes.extend_from_slice(b"-\n");
    if end {
        bytes.extend_from_slice(b"0\n");
    }
    bytes
}

fn run_wrapper(remote: &str, input: &[u8]) -> (Option<i32>, Vec<u8>) {
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(wrapper(remote))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    (output.status.code(), output.stdout)
}

#[test]
fn the_wrapper_passes_framed_bytes_unchanged_and_ends_the_input() {
    let binary: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    let input = framed(&[b"line\n", &binary, b"\x00tail"], true);
    let (code, stdout) = run_wrapper("cat; echo; exit 4", &input);
    assert_eq!(code, Some(4));
    let mut expected = b"line\n".to_vec();
    expected.extend_from_slice(&binary);
    expected.extend_from_slice(b"\x00tail\n");
    assert_eq!(stdout, expected);
}

#[test]
fn the_wrapper_ends_the_input_when_its_stream_does() {
    // The relay vanished without an end frame: the input still ends.
    let (code, stdout) = run_wrapper("wc -c | tr -d ' '", &framed(&[b"abc"], false));
    assert_eq!(code, Some(0));
    assert_eq!(stdout, b"3\n");
}

#[test]
fn the_remote_command_is_quoted_once() {
    let (code, stdout) = run_wrapper("printf '%s' \"it's $((1+1))\"", &framed(&[], true));
    assert_eq!(code, Some(0));
    assert_eq!(stdout, b"it's 2");
}
