use super::*;
use crate::config::SkillInstall;

#[test]
fn agent_skill_install_is_named_and_defaults_to_asking() -> anyhow::Result<()> {
    for config in [
        Config::default(),
        Config::parse("")?,
        Config::parse(DEFAULT_CONFIG)?,
    ] {
        assert_eq!(config.agent_skill.install, SkillInstall::Ask);
    }
    for (name, install) in [
        ("ask", SkillInstall::Ask),
        ("local", SkillInstall::Local),
        ("off", SkillInstall::Off),
    ] {
        let text = format!("[agent_skill]\ninstall = \"{name}\"");
        assert_eq!(Config::parse(&text)?.agent_skill.install, install);
    }
    for bad in ["\"Off\"", "\"never\"", "false"] {
        assert!(Config::parse(&format!("[agent_skill]\ninstall = {bad}")).is_err());
    }
    Ok(())
}
