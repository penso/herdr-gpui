use super::*;
use crate::{probe::Shell, providers::claude};

/// A Mac whose keychain answers a lookup by service alone with `root`'s item,
/// which holds only MCP sign-ins, as one `sudo claude` leaves behind. Only
/// `alice` has a Claude sign-in.
fn mac(user: &str) -> (tempfile::TempDir, Shell) {
    mac_profile(user, None)
}

fn mac_profile(user: &str, profile: Option<&str>) -> (tempfile::TempDir, Shell) {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bin = home.join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    crate::test_executable::write(bin.join("uname"), "#!/bin/sh\necho Darwin\n", 0o755).unwrap();
    crate::test_executable::write(
        bin.join("security"),
        concat!(
            "#!/bin/sh\n",
            "case \"$*\" in\n",
            "  *'-a alice'*) printf '%s' '{\"claudeAiOauth\":{\"accessToken\":\"alice-token\",\"subscriptionType\":\"max\"}}' ;;\n",
            "  *' -a '*) exit 44 ;;\n",
            "  *) printf '%s' '{\"mcpOAuth\":{}}' ;;\n",
            "esac\n",
        ),
        0o755,
    )
    .unwrap();
    let mut command = std::process::Command::new("/bin/sh");
    command
        .arg("-s")
        .env_clear()
        .env("HOME", &home)
        .env("USER", user)
        .env("PATH", "/usr/bin:/bin")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(profile) = profile {
        command.env("CLAUDE_CONFIG_DIR", profile);
    }
    (root, Shell::start(command).unwrap())
}

#[test]
fn claude_reads_the_users_own_keychain_item() {
    let mut jar = CookieJar::default();
    let (_root, shell) = mac("alice");
    let mut exec = Exec::Remote(shell);
    let mut probe = Probe::new(
        &mut exec,
        provider("claude"),
        None,
        &mut jar,
        Consent::Quiet,
    );
    let credentials = claude::keychain(&mut probe).unwrap();
    assert_eq!(
        probe
            .text(&credentials, &["claudeAiOauth", "subscriptionType"])
            .as_deref(),
        Some("max")
    );

    let (_root, shell) = mac("carol");
    let mut exec = Exec::Remote(shell);
    let mut probe = Probe::new(
        &mut exec,
        provider("claude"),
        None,
        &mut jar,
        Consent::Quiet,
    );
    assert!(
        claude::keychain(&mut probe).is_none(),
        "an item without a Claude sign-in is still skipped"
    );
}

fn profile_plan(profile: &str) -> Option<String> {
    let (_root, shell) = mac_profile("alice", Some(profile));
    let bin = _root.path().join("home/.local/bin");
    crate::test_executable::write(
        bin.join("security"),
        "#!/bin/sh\ncase \"$*\" in\n  *'Claude Code-credentials-2ea9e238 '*|*'Claude Code-credentials-5636a904 '*) printf '%s' '{\"claudeAiOauth\":{\"accessToken\":\"profile-token\",\"subscriptionType\":\"profile\"}}' ;;\n  *'Claude Code-credentials-a10b5e0a '*) printf '%s' '{\"claudeAiOauth\":{\"accessToken\":\"trailing-token\",\"subscriptionType\":\"trailing\"}}' ;;\n  *'Claude Code-credentials '*) printf '%s' '{\"claudeAiOauth\":{\"accessToken\":\"wrong-token\",\"subscriptionType\":\"generic\"}}' ;;\n  *) exit 44 ;;\nesac\n",
        0o755,
    ).unwrap();
    let mut exec = Exec::Remote(shell);
    let mut jar = CookieJar::default();
    let mut probe = Probe::new(
        &mut exec,
        provider("claude"),
        None,
        &mut jar,
        Consent::Quiet,
    );
    let credentials = claude::keychain(&mut probe)?;
    probe.text(&credentials, &["claudeAiOauth", "subscriptionType"])
}

#[test]
fn claude_explicit_profile_never_reads_generic_credentials() {
    assert_eq!(profile_plan("/profiles/work").as_deref(), Some("profile"));
    assert_eq!(profile_plan("/profiles/missing"), None);
    assert_eq!(profile_plan("").as_deref(), Some("generic"));
}

#[test]
fn claude_profile_keychain_normalizes_unicode_to_nfc() {
    assert_eq!(profile_plan("/profiles/équipe").as_deref(), Some("profile"));
    assert_eq!(
        profile_plan("/profiles/e\u{301}quipe").as_deref(),
        Some("profile")
    );
}

#[test]
fn claude_profile_keychain_keeps_surrounding_whitespace() {
    assert_eq!(
        profile_plan("/profiles/work ").as_deref(),
        Some("trailing"),
        "a trailing space names a different directory"
    );
}
