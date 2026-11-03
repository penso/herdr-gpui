#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::client::exit;

fn info(status: Option<i32>, truncated: bool) -> ExecInfo {
    ExecInfo {
        device: "coder:w1".into(),
        status,
        stdout: "main\n".into(),
        stderr: "warning\n".into(),
        truncated,
    }
}

#[test]
fn the_command_s_exit_decides_the_exit_and_stderr_says_why() {
    let done = output(&info(Some(0), false)).unwrap();
    assert_eq!(done.status, 0);
    assert_eq!(done.bytes.as_deref(), Some(&b"main\n"[..]));
    assert_eq!(done.text, "main");
    assert_eq!(done.stderr, "warning\n");
    assert_eq!(done.json["status"], 0);

    let failed = output(&info(Some(128), true)).unwrap();
    assert_eq!(failed.status, exit::REFUSED);
    assert_eq!(
        failed.stderr,
        "warning\n[sheprd: only the end of the output was kept]\n\
         [sheprd: the command exited 128]\n"
    );
    let signalled = output(&info(None, false)).unwrap();
    assert_eq!(signalled.status, exit::REFUSED);
    assert!(signalled.stderr.ends_with("a signal ended the command]\n"));
}
