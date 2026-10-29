use super::*;

#[test]
fn nothing_picked_composes_no_script() {
    assert_eq!(compose([], None).unwrap(), None);
    assert_eq!(compose([], Some("  \n ")).unwrap(), None);
}

#[test]
fn agents_install_once_in_display_order_before_the_users_lines() {
    let script = compose(
        [Agent::OpenCode, Agent::Claude, Agent::OpenCode],
        Some("\ngit config --global pull.rebase true\n"),
    )
    .unwrap()
    .unwrap();
    let claude = script.find("Installing Claude Code").unwrap();
    let opencode = script.find("Installing OpenCode").unwrap();
    let custom = script.find("git config").unwrap();
    assert!(claude < opencode && opencode < custom);
    assert_eq!(script.matches("Installing OpenCode").count(), 1);
    assert!(script.ends_with("pull.rebase true\n"));
}

#[test]
fn a_custom_script_alone_runs_without_the_agents_note() {
    let script = compose([], Some("echo hi")).unwrap().unwrap();
    assert_eq!(script, "echo hi\n");
}

#[test]
fn a_nul_or_oversized_script_is_refused() {
    assert!(matches!(compose([], Some("a\0b")), Err(Error::ScriptNul)));
    let long = "x".repeat(MAX_SCRIPT);
    assert!(matches!(
        compose([Agent::Claude], Some(&long)),
        Err(Error::ScriptTooLong { limit: MAX_SCRIPT })
    ));
}

#[cfg(feature = "cloud")]
#[test]
fn a_summary_names_the_agents_and_counts_the_users_lines() {
    let provider = herdr_client::CloudProvider::ALL[0];
    let mut cloud = crate::config::CloudConfig::default();
    assert_eq!(summary(&cloud, provider), "Nothing to run.");
    cloud.setup = Some("echo one\n".into());
    assert_eq!(
        summary(&cloud, provider),
        "Runs \u{2068}1\u{2069} line of your own."
    );
    cloud.agents = Some(vec!["opencode".into(), "claude".into()]);
    cloud.setup = Some("a\nb\n".into());
    assert_eq!(
        summary(&cloud, provider),
        "Installs \u{2068}Claude Code, OpenCode\u{2069}, then runs \u{2068}2\u{2069} lines of your own."
    );
}
