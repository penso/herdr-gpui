use super::*;
use crate::cloud_setup::Agent;

#[test]
fn no_cloud_table_runs_nothing() -> anyhow::Result<()> {
    let config = Config::parse("")?;
    assert!(config.cloud.shared_agents().is_empty());
    assert_eq!(config.cloud.shared_setup(), None);
    Ok(())
}

#[test]
fn every_provider_table_parses_in_every_build() -> anyhow::Result<()> {
    let config = Config::parse(
        "[cloud]\nagents = [\"claude\"]\nsetup = \"echo hi\"\n\
         [cloud.azure]\nagents = [\"codex\"]\n[cloud.boat]\nsetup = \"\"\n[cloud.coder]\nagents = []\n\
         [cloud.daytona]\nagents = [\"opencode\"]\n[cloud.railway]\nsetup = \"echo rail\"\n",
    )?;
    assert_eq!(config.cloud.shared_agents(), [Agent::Claude]);
    assert_eq!(config.cloud.shared_setup(), Some("echo hi"));
    assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    Ok(())
}

#[cfg(feature = "azure")]
#[test]
fn azure_uses_its_own_table() -> anyhow::Result<()> {
    use herdr_client::CloudProvider;
    let config =
        Config::parse("[cloud]\nagents = [\"claude\"]\n[cloud.azure]\nagents = [\"codex\"]\n")?;
    assert_eq!(config.cloud.agents(CloudProvider::Azure), [Agent::Codex]);
    let config = Config::parse("[cloud]\nagents = [\"claude\"]\n[cloud.azure]\nagents = []\n")?;
    assert_eq!(config.cloud.script(CloudProvider::Azure)?, None);
    Ok(())
}

#[test]
fn an_unknown_agent_is_reported_and_skipped() -> anyhow::Result<()> {
    let config = Config::parse(
        "[cloud]\nagents = [\"claude\", \"someday\"]\n[cloud.coder]\nagents = [\"later\"]\n",
    )?;
    assert_eq!(config.cloud.shared_agents(), [Agent::Claude]);
    assert_eq!(
        config.unknown_keys,
        ["cloud.agents.someday", "cloud.coder.agents.later"]
    );
    Ok(())
}

#[test]
fn an_unknown_cloud_key_is_refused() {
    assert!(Config::parse("[cloud]\nscript = \"echo\"\n").is_err());
}

#[test]
fn an_oversized_script_fails_to_load() {
    let text = format!(
        "[cloud.railway]\nsetup = \"{}\"\n",
        "x".repeat(crate::cloud_setup::MAX_SCRIPT + 1)
    );
    let error = Config::parse(&text).err();
    assert!(
        matches!(
            error,
            Some(crate::Error::CloudSetup(
                crate::cloud_setup::Error::ScriptTooLong { .. }
            ))
        ),
        "{error:?}"
    );
}

#[cfg(feature = "cloud")]
#[test]
fn a_provider_table_replaces_each_shared_key_it_sets() -> anyhow::Result<()> {
    use herdr_client::CloudProvider;
    let config = Config::parse(
        "[cloud]\nagents = [\"claude\"]\nsetup = \"echo shared\"\n\
         [cloud.boat]\nsetup = \"echo boat\"\n[cloud.coder]\nagents = []\nsetup = \"\"\n",
    )?;
    for provider in CloudProvider::ALL.iter().copied() {
        let script = config.cloud.script(provider)?;
        match provider.key() {
            "boat" => {
                let script = script.unwrap_or_default();
                assert!(script.contains("Claude Code") && script.contains("echo boat"));
                assert!(!script.contains("echo shared"));
            }
            "coder" => assert_eq!(script, None),
            _ => {
                let script = script.unwrap_or_default();
                assert!(script.contains("Claude Code") && script.contains("echo shared"));
            }
        }
        let agents = config.cloud.agents(provider);
        assert_eq!(agents.is_empty(), provider.key() == "coder");
    }
    Ok(())
}

#[test]
fn saving_agents_writes_them_in_install_order_and_keeps_comments() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config-gpui.toml");
    let local = path.with_extension("local.toml");
    fs::write(&local, "# Mine\n[cloud]\nsetup = \"echo hi\" # kept\n")?;
    let value = cloud::agents_value(&[Agent::OpenCode, Agent::Claude]);
    Config::edit_key_at(
        &local,
        &temp.0.join("absent.toml"),
        "cloud.agents",
        Some(value),
    )?;
    let text = fs::read_to_string(&local)?;
    assert!(text.contains("setup = \"echo hi\" # kept"), "{text}");
    let config = Config::parse(&text)?;
    assert_eq!(
        config.cloud.shared_agents(),
        [Agent::Claude, Agent::OpenCode]
    );
    Ok(())
}
