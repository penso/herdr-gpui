#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_finished_commands_stderr_is_read_in_a_session_of_its_own() {
    let (mut socket, service) = terminal(&[b"warn\n", b"\x01"], 0);
    let mut stderr = Vec::new();
    relay::fetch_errors(&mut socket, "/tmp/herdr-relay-x", &mut stderr).unwrap();
    drop(socket);

    assert_eq!(stderr, b"warn\n\x01");
    let served = service.join().unwrap();
    let command = String::from_utf8(served.command).unwrap();
    // It deletes what it read, and its own session's stderr with it.
    let rest = command
        .strip_prefix("cat /tmp/herdr-relay-x.err 2>/dev/null; rm -f /tmp/herdr-relay-x.err /tmp/herdr-relay-")
        .unwrap();
    assert!(rest.ends_with(".err"), "{command}");
    assert!(served.input.is_empty());
}
