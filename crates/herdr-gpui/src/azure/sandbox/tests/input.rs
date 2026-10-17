#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn input_travels_in_length_frames_and_an_empty_one_ends_it() {
    assert_eq!(relay::input_frame(b"\x00accept\n"), b"8\n\x00accept\n");
    assert_eq!(relay::input_frame(&[]), b"0\n");
}

#[test]
fn the_setup_line_gives_the_command_an_input_that_ends() {
    let line = relay::setup_line(12, "/tmp/herdr-relay-x");
    // The reader stops at the empty frame, and the command reads its FIFO.
    assert!(line.contains("while IFS= read -r n && [ \"$n\" -gt 0 ]; do head -c \"$n\"; done;"));
    assert!(line.contains("eval \"$s\" <\"$f\" 2>\"$e\";"));
}

#[test]
fn the_interactive_shell_only_starts_a_quoted_non_interactive_one() {
    let line = relay::setup_line(12, "/tmp/herdr-relay-x");
    // An interactive shell prints job notices and expands history, so it
    // runs nothing but `stty` and one single-quoted script.
    let (interactive, script) = line.split_once("exec /bin/sh -c '").unwrap();
    assert_eq!(interactive, "stty raw -echo -iexten || exit 255; ");
    let script = script.strip_suffix("'\n").unwrap();
    assert!(!script.contains('\''));
}
