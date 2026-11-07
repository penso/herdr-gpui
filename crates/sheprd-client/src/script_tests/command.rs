use super::*;

#[test]
fn prepared_commands_stream_binary_input_and_keep_typed_failures() {
    let bytes: Vec<u8> = (0..CHUNK * 3 + 17).map(|n| n as u8).collect();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "cat"]);
    let mut output = Vec::new();
    assert_eq!(run_script_command(command, bytes.as_slice(), &mut output, LIMITS, &AtomicBool::new(false)).unwrap(), bytes.len() as u64);
    assert_eq!(output, bytes);

    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf 'failure\\033' >&2; exit 7"]);
    let error = run_script_command(command, io::empty(), io::sink(), LIMITS, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(error, Error::ScriptExit { status, stderr } if status.code() == Some(7) && stderr == "failure"));
}

#[test]
fn cancellation_before_start_never_spawns_a_prepared_command() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("spawned");
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "touch \"$1\"", "sh"]).arg(&marker);
    assert!(matches!(run_script_command(command, io::empty(), io::sink(), LIMITS, &AtomicBool::new(true)), Err(Error::ScriptCancelled)));
    assert!(!marker.exists());
}
