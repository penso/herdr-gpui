use super::*;
use crate::SshCommand;
use std::path::PathBuf;

/// Spawns `command`, captures its stdout, and asserts it succeeded.
fn stdout(command: &mut Command) -> Vec<u8> {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("herdr-{name}-{}", std::process::id()))
}

/// A custom OpenSSH binary (a path with a space) is invoked with the bridge
/// policy, and the remote script it is handed actually runs.
#[test]
fn a_custom_openssh_binary_is_honored_and_runs_the_remote_script() {
    let root = scratch("custom-command");
    std::fs::create_dir(&root).unwrap();
    let custom = root.join("custom ssh");
    crate::test_executable::write(
        &custom,
        r#"#!/bin/sh
for option in "$@"; do
    [ "$option" = "BatchMode=yes" ] && seen=1
done
[ "$seen" = 1 ] || exit 7
for last in "$@"; do :; done
exec /bin/sh -c "$last"
"#,
        0o700,
    )
    .unwrap();
    let target = SshTarget {
        target: "smoke@fixture.invalid".into(),
        command: SshCommand::try_from(format!("'{}'", custom.display())).unwrap(),
    };
    let mut command = script_command(&target, "printf 'custom-output'").unwrap();
    assert_eq!(stdout(&mut command), b"custom-output");
    std::fs::remove_dir_all(root).unwrap();
}

/// `tailscale ssh` takes its destination first, so the configured command
/// puts it there with the remote script last; the script still runs.
#[test]
fn a_destination_first_tailscale_command_runs_the_remote_script() {
    let root = scratch("tailscale-command");
    std::fs::create_dir(&root).unwrap();
    let tailscale = root.join("tailscale");
    crate::test_executable::write(
        &tailscale,
        r#"#!/bin/sh
[ "$1" = "ssh" ] || exit 8
[ "$2" = "smoke@fixture.invalid" ] || exit 9
for last in "$@"; do :; done
exec /bin/sh -c "$last"
"#,
        0o700,
    )
    .unwrap();
    let target = SshTarget {
        target: "smoke@fixture.invalid".into(),
        command: SshCommand::try_from(format!("{} ssh", tailscale.display())).unwrap(),
    };
    let mut command = script_command(&target, "printf 'tailscale-output'").unwrap();
    assert_eq!(stdout(&mut command), b"tailscale-output");
    std::fs::remove_dir_all(root).unwrap();
}

/// Fixed command arguments are passed literally, never expanded: a
/// `$(touch ...)` argument reaches the child as text and no file appears.
#[test]
fn fixed_command_arguments_are_not_shell_expanded() {
    let root = scratch("injection");
    std::fs::create_dir(&root).unwrap();
    let ssh = root.join("ssh");
    crate::test_executable::write(
        &ssh,
        r#"#!/bin/sh
for last in "$@"; do :; done
exec /bin/sh -c "$last"
"#,
        0o700,
    )
    .unwrap();
    let injected = root.join("INJECTED");
    let target = SshTarget {
        target: "smoke@fixture.invalid".into(),
        command: SshCommand::try_from(format!(
            "{} '$(touch {})'",
            ssh.display(),
            injected.display()
        ))
        .unwrap(),
    };
    let mut command = script_command(&target, "printf 'clean'").unwrap();
    assert_eq!(stdout(&mut command), b"clean");
    assert!(!injected.exists());
    std::fs::remove_dir_all(root).unwrap();
}
