use super::*;
use crate::teleport::AgentKind;

#[test]
fn no_table_starts_every_agent_with_no_arguments() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
        Config::parse("[orchestrator]")?,
    ] {
        for kind in AgentKind::ALL {
            assert!(config.orchestrator.args(kind, false).is_empty());
            assert!(config.orchestrator.args(kind, true).is_empty());
        }
    }
    Ok(())
}

#[test]
fn an_agents_table_sets_its_arguments_and_a_review_falls_back_to_them() -> anyhow::Result<()> {
    let config = Config::parse(
        r#"
[orchestrator.agents.opencode]
args = ["--standalone"]
review_args = ["--standalone", "--agent", "review"]

[orchestrator.agents.codex]
args = ["--sandbox", "workspace-write"]
"#,
    )?;
    let args = |kind, review| config.orchestrator.args(kind, review).to_vec();
    assert_eq!(args(AgentKind::Opencode, false), ["--standalone"]);
    assert_eq!(
        args(AgentKind::Opencode, true),
        ["--standalone", "--agent", "review"]
    );
    assert_eq!(
        args(AgentKind::Codex, true),
        ["--sandbox", "workspace-write"]
    );
    assert!(args(AgentKind::Claude, true).is_empty());
    // An empty list is a choice too: no arguments for reviews.
    let config =
        Config::parse("[orchestrator.agents.claude]\nargs = [\"--verbose\"]\nreview_args = []")?;
    assert!(config.orchestrator.args(AgentKind::Claude, true).is_empty());
    Ok(())
}

#[test]
fn an_agent_this_build_cannot_start_is_reported_and_ignored() -> anyhow::Result<()> {
    let config = Config::parse(
        r#"
[orchestrator.agents.not-an-agent]
args = ["--x"]

[orchestrator.agents.Claude]
args = ["--y"]

[orchestrator.agents.opencode]
args = ["--standalone"]
color = "blue"
"#,
    )?;
    for key in [
        "orchestrator.agents.not-an-agent",
        "orchestrator.agents.Claude",
        "orchestrator.agents.opencode.color",
    ] {
        assert!(
            config.unknown_keys.iter().any(|unknown| unknown == key),
            "{key} in {:?}",
            config.unknown_keys
        );
    }
    assert!(
        config
            .orchestrator
            .args(AgentKind::Claude, false)
            .is_empty()
    );
    assert_eq!(
        config.orchestrator.args(AgentKind::Opencode, false),
        ["--standalone"]
    );
    Ok(())
}

#[test]
fn arguments_herdr_would_refuse_fail_the_load_and_name_the_key() {
    let many = format!("[{}]", vec!["\"-v\""; 33].join(", "));
    for (table, key) in [
        ("args = [\"--agent\", \"a\\nb\"]", "args"),
        (&*format!("review_args = {many}"), "review_args"),
        (&*format!("args = [\"{}\"]", "x".repeat(1025)), "args"),
    ] {
        match Config::parse(&format!("[orchestrator.agents.opencode]\n{table}")) {
            Err(Error::OrchestratorConfig(error)) => {
                let message = error.to_string();
                assert!(
                    message.starts_with(&format!("orchestrator.agents.opencode.{key} ")),
                    "{message}"
                );
            }
            other => panic!("{table}: {other:?}"),
        }
    }
    // A list of strings, not one command line.
    assert!(Config::parse("[orchestrator.agents.opencode]\nargs = \"--agent review\"").is_err());
}
