use super::*;

fn failing(message: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", &format!("printf '%s\\n' '{message}' >&2; exit 255")]);
    command
}

#[test]
fn a_piped_stderr_tail_classifies_why_ssh_failed() {
    let mut command = failing("Host key verification failed.");
    command.stderr(Stdio::piped());
    let (status, output, stderr) =
        run_with_stderr(&mut command, Duration::from_secs(5), || false).unwrap();
    assert_eq!(status.code(), Some(255));
    assert!(output.is_empty());
    assert_eq!(
        SshFailure::classify(status.code(), &stderr),
        SshFailure::HostKey
    );
}

#[test]
fn plain_run_still_discards_stderr() {
    let (status, output) = run(
        &mut failing("Permission denied (publickey)."),
        Duration::from_secs(5),
        || false,
    )
    .unwrap();
    assert_eq!(status.code(), Some(255));
    assert!(output.is_empty());
}
