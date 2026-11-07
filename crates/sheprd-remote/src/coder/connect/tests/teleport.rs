use super::*;
use sheprd_client::{ScriptLimits, run_script_command, shell_quote};
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

#[test]
fn teleport_runs_a_quoted_script_through_coder_ssh() {
    let dir = tempfile::tempdir().unwrap();
    let cli = dir.path().join("coder");
    fs::write(
        &cli,
        r#"#!/bin/sh
test "$CODER_URL" = https://coder.example.com || exit 90
test "$CODER_SESSION_TOKEN" = fixture-token || exit 91
test "$1" = ssh && test "$2" = --wait=no && test "$3" = -- || exit 92
test "$4" = renamed-box.main || exit 93
test "$#" = 5 || exit 94
exec /bin/sh -c "$5"
"#,
    )
    .unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
    let mut command = ssh_command(
        &cli,
        &settings(),
        &"fixture-token".into(),
        "renamed-box",
        "main",
    )
    .unwrap();
    let literal = "'quoted'; $(exit 99)\nnext line";
    command.arg(format!(
        "/bin/sh -c {}",
        shell_quote(&format!("printf %s {}; cat", shell_quote(literal)))
    ));
    let input = b"\0binary\xff";
    let mut output = Vec::new();
    run_script_command(
        command,
        &input[..],
        &mut output,
        ScriptLimits {
            output: 1024,
            idle: Duration::from_secs(5),
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(output, [literal.as_bytes(), input].concat());
}
