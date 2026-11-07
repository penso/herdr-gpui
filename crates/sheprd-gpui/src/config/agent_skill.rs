//! `[agent_skill]`: where the app may put its managed agent skill. It wins
//! over the answer saved in `agent-skill.json`, so it can travel with
//! dotfiles and a fresh machine does not ask again.
use serde::Deserialize;

/// Where the managed skill may go on its own.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillInstall {
    /// Offer it once; the answer covers this machine and saved SSH hosts.
    #[default]
    Ask,
    /// As `ask`, but saved SSH hosts are never touched.
    Local,
    /// Never offer, refresh, or remove it here or on SSH hosts. Installing
    /// from Settings or the palette still works, once.
    Off,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AgentSkillConfig {
    pub install: SkillInstall,
}
