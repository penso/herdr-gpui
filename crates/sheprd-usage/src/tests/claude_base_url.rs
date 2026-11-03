use super::*;
use crate::{
    probe::{HostPath, Shell},
    providers::claude,
};

/// A Linux host signed in to Claude Code through `.credentials.json`, with
/// `settings` as its `settings.json` and `base` as `ANTHROPIC_BASE_URL`. Its
/// `curl` only leaves a mark, so a test can tell whether a request was sent,
/// and its `uname` and `security` stand in for the real ones, so on a Mac the
/// read never reaches this machine's Keychain.
fn host(settings: Option<&str>, base: Option<&str>) -> (tempfile::TempDir, Shell) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bin = home.join(".local/bin");
    let claude = home.join(".claude");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"left-behind"}}"#,
    )
    .unwrap();
    if let Some(settings) = settings {
        std::fs::write(claude.join("settings.json"), settings).unwrap();
    }
    for (tool, script) in [
        ("curl", "touch \"$HOME/asked\"\nexit 7"),
        ("security", "exit 44"),
        ("uname", "echo Linux"),
    ] {
        crate::test_executable::write(bin.join(tool), format!("#!/bin/sh\n{script}\n"), 0o755)
            .unwrap();
    }
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-s")
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(base) = base {
        command.env("ANTHROPIC_BASE_URL", base);
    }
    (root, Shell::start(command).unwrap())
}

fn elsewhere(settings: Option<&str>, base: Option<&str>) -> bool {
    let mut jar = CookieJar::default();
    let (_root, shell) = host(settings, base);
    let mut exec = Exec::Remote(shell);
    let mut probe = Probe::new(
        &mut exec,
        provider("claude"),
        None,
        &mut jar,
        Consent::Quiet,
    );
    let settings = HostPath::env_or("CLAUDE_CONFIG_DIR", ".claude", "settings.json");
    claude::elsewhere(&mut probe, &settings)
}

/// Whether reading Claude's usage sent a request, and what it answered.
fn fetch(settings: Option<&str>, base: Option<&str>) -> (bool, bool) {
    let mut jar = CookieJar::default();
    let (root, shell) = host(settings, base);
    let mut exec = Exec::Remote(shell);
    let mut probe = Probe::new(
        &mut exec,
        provider("claude"),
        None,
        &mut jar,
        Consent::Quiet,
    );
    let answer = provider("claude").service().fetch(&mut probe);
    (root.path().join("home/asked").exists(), answer.is_some())
}

#[test]
fn claude_code_on_another_endpoint_or_a_key_helper_is_not_signed_in_through_oauth() {
    assert!(!elsewhere(None, None));
    assert!(!elsewhere(None, Some("https://api.anthropic.com")));
    assert!(!elsewhere(None, Some("https://api.anthropic.com/")));
    assert!(elsewhere(None, Some("https://relay.example.com")));
    assert!(elsewhere(
        None,
        Some("https://api.anthropic.com.example.com")
    ));
    assert!(elsewhere(
        Some(r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.example.com/v1"}}"#),
        None,
    ));
    assert!(!elsewhere(
        Some(r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.anthropic.com"}}"#),
        None,
    ));
    assert!(elsewhere(
        Some(r#"{"apiKeyHelper":"~/bin/anthropic-key"}"#),
        None
    ));
    assert!(!elsewhere(Some(r#"{"model":"opus"}"#), None));
}

#[test]
fn a_left_behind_token_is_not_sent_when_claude_code_uses_another_endpoint() {
    let relay = r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.example.com"}}"#;
    assert_eq!(fetch(Some(relay), None), (false, false));
    assert_eq!(
        fetch(None, Some("https://relay.example.com")),
        (false, false)
    );
    // The same sign-in on Anthropic's own endpoint is still asked.
    assert_eq!(fetch(None, None), (true, true));
}
