//! Only help and usage-error paths: none may reach GPUI or require a desktop/daemon.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn cli(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_herdr-gpui"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("launch the Cargo-built GUI binary");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!("CLI {args:?} did not exit before timeout: {output:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn usage_error(args: &[&str], message: &str) {
    let output = cli(args);
    // An abort or native-window failure is not a successful parser rejection.
    assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
    assert!(output.stdout.is_empty(), "{args:?}: {output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(message), "{args:?}: {stderr}");
    assert!(stderr.contains("Usage: herdr-gpui"), "{stderr}");
}

#[test]
fn help_succeeds_and_documents_connection_options() {
    for flag in ["--help", "-h"] {
        let output = cli(&[flag]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let help = String::from_utf8(output.stdout).unwrap();
        for option in ["--socket CLIENT_SOCKET", "--session NAME", "--dev"] {
            assert!(help.contains(option), "missing {option}: {help}");
        }
        assert_eq!(
            help.contains("--integration-test"),
            cfg!(feature = "integration-test"),
            "{help}"
        );
    }
}

#[test]
fn missing_option_values_are_usage_errors() {
    usage_error(&["--socket"], "--socket requires a path");
    usage_error(&["--session"], "--session requires a name");
    usage_error(&["--dev", "--session"], "--session requires a name");
    usage_error(&["--socket", "--help"], "--socket requires a path");
    usage_error(&["--session", "--dev"], "--session requires a name");
}

#[test]
fn duplicate_options_are_usage_errors() {
    usage_error(
        &["--socket", "a", "--socket", "b"],
        "--socket may only be specified once",
    );
    usage_error(
        &["--session", "a", "--session", "b"],
        "--session may only be specified once",
    );
}

#[test]
fn unknown_flags_and_positional_arguments_are_usage_errors() {
    for arg in ["--unknown", "--socket=/unused.sock", "unexpected", "--"] {
        usage_error(&[arg], &format!("Unknown option: {arg}"));
    }
}

#[test]
fn explicit_socket_conflicts_with_session_and_development() {
    for args in [
        vec!["--socket", "/unused.sock", "--session", "test"],
        vec!["--session", "test", "--socket", "/unused.sock"],
        vec!["--socket", "/unused.sock", "--dev"],
        vec!["--dev", "--socket", "/unused.sock"],
    ] {
        usage_error(&args, "--socket cannot be combined with --session or --dev");
    }
}

#[cfg(not(feature = "integration-test"))]
#[test]
fn native_test_flag_is_rejected_in_normal_builds() {
    usage_error(&["--agent-test"], "Unknown option: --agent-test");
    usage_error(
        &["--integration-test"],
        "Unknown option: --integration-test",
    );
    usage_error(
        &["--socket", "/unused.sock", "--integration-test"],
        "Unknown option: --integration-test",
    );
}

#[cfg(feature = "integration-test")]
#[test]
fn agent_fixture_rejects_connection_options_and_other_modes() {
    for args in [
        vec!["--agent-test", "--socket", "/unused.sock"],
        vec!["--session", "test", "--agent-test"],
        vec!["--agent-test", "--dev"],
    ] {
        usage_error(
            &args,
            "fixture tests cannot be combined with connection options",
        );
    }
    for flag in [
        "--agent-test",
        "--sidebar-test",
        "--performance-test",
        "--integration-test",
    ] {
        usage_error(
            &["--agent-test", flag],
            "native test modes are mutually exclusive",
        );
        usage_error(
            &[flag, "--agent-test"],
            "native test modes are mutually exclusive",
        );
    }
}

#[cfg(feature = "integration-test")]
#[test]
fn native_test_flag_requires_explicit_socket_not_session_discovery() {
    for args in [
        vec!["--integration-test"],
        vec!["--integration-test", "--session", "test"],
        vec!["--dev", "--integration-test"],
        vec!["--session", "test", "--dev", "--integration-test"],
    ] {
        usage_error(&args, "--integration-test requires an explicit --socket");
    }
}
