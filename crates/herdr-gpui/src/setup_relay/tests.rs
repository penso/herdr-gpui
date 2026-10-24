#![allow(clippy::unwrap_used)]
mod cleanup;

use super::{
    frame::{Frame, MAX_PAYLOAD, Size},
    *,
};
use crate::Error;
use std::{
    ffi::OsString,
    os::unix::{fs::PermissionsExt, net::UnixStream, process::CommandExt},
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn encoded(frame: &Frame) -> Vec<u8> {
    let mut bytes = Vec::new();
    frame.write(&mut bytes).unwrap();
    bytes
}

#[test]
fn frames_round_trip() {
    let size = Size {
        rows: 30,
        cols: 100,
    };
    for frame in [
        Frame::Hello {
            size,
            term: Some("xterm-256color".into()),
        },
        Frame::Hello { size, term: None },
        Frame::Input(b"\x03secret\r".to_vec()),
        Frame::Resize(Size { rows: 1, cols: 2 }),
        Frame::Output(vec![0xff; MAX_PAYLOAD]),
        Frame::Exit(-1),
    ] {
        let bytes = encoded(&frame);
        assert_eq!(Frame::read(&mut bytes.as_slice()).unwrap(), Some(frame));
    }
    // A peer that closed between frames is the end, not an error.
    assert_eq!(Frame::read(&mut [].as_slice()).unwrap(), None);
}

#[test]
fn malformed_frames_end_the_session() {
    let oversized = [4, 0, 0, 0x40, 1];
    assert!(matches!(
        Frame::read(&mut oversized.as_slice()),
        Err(Error::SetupRelay(crate::setup_relay::Error::Frame))
    ));
    let unknown = [9, 0, 0, 0, 0];
    assert!(matches!(
        Frame::read(&mut unknown.as_slice()),
        Err(Error::SetupRelay(crate::setup_relay::Error::Frame))
    ));
    let short_resize = [3, 0, 0, 0, 2, 0, 1];
    assert!(matches!(
        Frame::read(&mut short_resize.as_slice()),
        Err(Error::SetupRelay(crate::setup_relay::Error::Frame))
    ));
    let truncated = &encoded(&Frame::Output(b"hello".to_vec()))[..7];
    assert!(matches!(
        Frame::read(&mut &truncated[..]),
        Err(Error::Io(_))
    ));
}

/// `TERM` becomes the setup's environment, so only a terminfo name passes.
#[test]
fn only_a_plain_term_name_is_passed_on() {
    let hello = |term: &[u8]| {
        let mut bytes = vec![1, 0, 0, 0, (4 + term.len()) as u8, 0, 24, 0, 80];
        bytes.extend_from_slice(term);
        match Frame::read(&mut bytes.as_slice()).unwrap() {
            Some(Frame::Hello { term, .. }) => term,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(
        hello(b"screen.xterm-256color").as_deref(),
        Some("screen.xterm-256color")
    );
    assert_eq!(hello(b"x\x1b]0;evil\x07"), None);
    assert_eq!(hello(b"a b"), None);
    assert_eq!(hello(&[b'x'; 65]), None);
}

/// Reads frames until the setup's exit, returning its output and status.
fn finish(stream: &mut UnixStream) -> (String, i32) {
    let mut output = Vec::new();
    loop {
        match Frame::read(stream).unwrap() {
            Some(Frame::Output(bytes)) => output.extend(bytes),
            Some(Frame::Exit(code)) => return (String::from_utf8_lossy(&output).into(), code),
            other => panic!("{other:?}"),
        }
    }
}

/// The setup gets a real terminal of the pane's size, reads what the pane
/// types, and its exit status comes back.
#[test]
fn the_setup_runs_on_a_terminal_the_app_owns() {
    let (mut pane, app) = UnixStream::pair().unwrap();
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        "[ -t 0 ] && echo tty; stty size; printf \"term:%s\\n\" \"$TERM\"; read line; echo \"got:$line\"; exit 3",
    ]);
    let host = thread::spawn(move || host::run(app, command, Duration::from_secs(30)));
    Frame::Hello {
        size: Size {
            rows: 30,
            cols: 100,
        },
        term: Some("vt100".into()),
    }
    .write(&mut pane)
    .unwrap();
    Frame::Input(b"abc\r".to_vec()).write(&mut pane).unwrap();
    let (output, code) = finish(&mut pane);
    assert_eq!(code, 3);
    assert!(output.contains("tty"), "{output}");
    assert!(output.contains("30 100"), "{output}");
    assert!(output.contains("term:vt100"), "{output}");
    assert!(output.contains("got:abc"), "{output}");
    assert_eq!(host.join().unwrap().unwrap().code(), Some(3));
}

#[test]
fn a_resized_pane_resizes_the_setup_terminal() {
    let (mut pane, app) = UnixStream::pair().unwrap();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "read line; stty size"]);
    let host = thread::spawn(move || host::run(app, command, Duration::from_secs(30)));
    Frame::Hello {
        size: Size { rows: 24, cols: 80 },
        term: None,
    }
    .write(&mut pane)
    .unwrap();
    Frame::Resize(Size {
        rows: 50,
        cols: 132,
    })
    .write(&mut pane)
    .unwrap();
    Frame::Input(b"\r".to_vec()).write(&mut pane).unwrap();
    let (output, code) = finish(&mut pane);
    assert_eq!(code, 0);
    assert!(output.contains("50 132"), "{output}");
    host.join().unwrap().unwrap();
}

/// Closing the pane hangs the setup up, as closing a terminal window does,
/// rather than leaving it waiting for input nobody can give.
#[test]
fn a_closed_pane_hangs_the_setup_up() {
    let (mut pane, app) = UnixStream::pair().unwrap();
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "trap 'exit 7' HUP; echo ready; while :; do sleep 1; done",
        ])
        // The session helper does this for a real setup.
        .process_group(0);
    let started = Instant::now();
    let host = thread::spawn(move || host::run(app, command, Duration::from_secs(60)));
    Frame::Hello {
        size: Size { rows: 24, cols: 80 },
        term: None,
    }
    .write(&mut pane)
    .unwrap();
    // Running, so it is the hang-up that ends it.
    let mut output = Vec::new();
    while !String::from_utf8_lossy(&output).contains("ready") {
        match Frame::read(&mut pane).unwrap() {
            Some(Frame::Output(bytes)) => output.extend(bytes),
            other => panic!("{other:?}"),
        }
    }
    drop(pane);
    let status = host.join().unwrap().unwrap();
    assert_eq!(status.code(), Some(7));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_relay_that_never_says_hello_is_refused() {
    let (mut pane, app) = UnixStream::pair().unwrap();
    Frame::Input(b"x".to_vec()).write(&mut pane).unwrap();
    let result = host::run(app, Command::new("/usr/bin/false"), Duration::from_secs(5));
    assert!(matches!(
        result,
        Err(Error::SetupRelay(crate::setup_relay::Error::Frame))
    ));
}

/// The socket is private, used once, and gone when the dialog gives up on it.
#[test]
fn the_socket_is_private_and_single_use() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("gpui");
    let waiting =
        listen::<()>(&dir, Command::new("/usr/bin/true"), Duration::from_secs(5)).unwrap();
    let path = waiting.path().to_owned();
    assert_eq!(
        std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(waiting);
    let deadline = Instant::now() + Duration::from_secs(5);
    while path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!path.exists());

    let held = std::sync::Arc::new(());
    let mut kept = listen(&dir, Command::new("/usr/bin/true"), Duration::from_secs(5)).unwrap();
    kept.keep(held.clone());
    let path = kept.path().to_owned();
    drop(kept);
    thread::sleep(Duration::from_millis(200));
    let mut pane = UnixStream::connect(&path).unwrap();
    Frame::Hello {
        size: Size { rows: 24, cols: 80 },
        term: None,
    }
    .write(&mut pane)
    .unwrap();
    assert_eq!(finish(&mut pane).1, 0);
    assert!(!path.exists());
    // What `keep` handed over is released once the setup is over.
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::sync::Arc::strong_count(&held) > 1 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(std::sync::Arc::strong_count(&held), 1);
    assert!(UnixStream::connect(&path).is_err());
}

/// A setup that cannot start says why in the pane and ends like a shell.
#[test]
fn a_setup_that_cannot_start_says_so() {
    let (mut pane, app) = UnixStream::pair().unwrap();
    Frame::Hello {
        size: Size { rows: 24, cols: 80 },
        term: None,
    }
    .write(&mut pane)
    .unwrap();
    let result = host::run(
        app,
        Command::new("/nonexistent/herdr-gpui"),
        Duration::from_secs(5),
    );
    assert!(matches!(result, Err(Error::Io(_))));
    let (output, code) = finish(&mut pane);
    assert_eq!(code, 127);
    assert!(output.starts_with("Herdr could not start the setup"));
}

#[test]
fn only_the_relay_arguments_start_a_helper() {
    let args = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    assert!(run_helper(&args(&["browser", "open"])).is_none());
    assert!(run_helper(&[]).is_none());
    assert_eq!(run_helper(&args(&[ATTACH])), Some(ExitCode::from(2)));
    assert_eq!(
        run_helper(&args(&[ATTACH, "a", "b"])),
        Some(ExitCode::from(2))
    );
    assert_eq!(run_helper(&args(&[SESSION])), Some(ExitCode::from(2)));
}
