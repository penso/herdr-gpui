//! The `[cloud]` table: what a new cloud device runs the first time it
//! connects (see `crate::cloud_setup`). `agents` and `setup` apply to every
//! provider; a provider's own table, such as `[cloud.coder]`, replaces either
//! key it sets, and an empty list or script there turns that part off. Every
//! build parses every provider's table, so one config file serves them all.

#[cfg(feature = "cloud")]
use super::Config;
use crate::{
    Result,
    cloud_setup::{self, Agent},
};
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CloudConfig {
    // keep-sorted start
    /// Agent keys installed first, such as `claude` or `opencode`.
    pub agents: Option<Vec<String>>,
    pub azure: CloudSetup,
    pub boat: CloudSetup,
    pub coder: CloudSetup,
    pub daytona: CloudSetup,
    pub railway: CloudSetup,
    /// Shell lines run after the agents, with `sh -e`.
    pub setup: Option<String>,
    // keep-sorted end
}

/// One provider's replacements for the shared keys.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CloudSetup {
    pub agents: Option<Vec<String>>,
    pub setup: Option<String>,
}

impl CloudConfig {
    /// The agents every provider installs unless its table says otherwise.
    #[cfg(any(test, feature = "cloud"))]
    pub(crate) fn shared_agents(&self) -> Vec<Agent> {
        known(self.agents.as_deref())
    }

    /// The shared script, for Settings to preview.
    #[cfg(any(test, feature = "cloud"))]
    pub(crate) fn shared_setup(&self) -> Option<&str> {
        self.setup.as_deref()
    }

    /// The script for a device from the provider with this table.
    fn script_with(&self, table: &CloudSetup) -> Result<Option<String>> {
        let agents = table.agents.as_deref().or(self.agents.as_deref());
        let setup = table.setup.as_deref().or(self.setup.as_deref());
        Ok(cloud_setup::compose(known(agents), setup)?)
    }

    fn tables(&self) -> [(&'static str, &CloudSetup); 5] {
        [
            ("azure", &self.azure),
            ("boat", &self.boat),
            ("coder", &self.coder),
            ("daytona", &self.daytona),
            ("railway", &self.railway),
        ]
    }

    /// Drops agent names this build does not know, so a name a newer build
    /// added still loads, and returns them as unknown keys to report.
    pub(super) fn retain_known(&mut self) -> Vec<String> {
        let mut unknown = Vec::new();
        let mut retain = |prefix: &str, agents: &mut Option<Vec<String>>| {
            if let Some(agents) = agents {
                agents.retain(|key| {
                    let known = Agent::from_key(key).is_some();
                    if !known {
                        unknown.push(format!("{prefix}agents.{key}"));
                    }
                    known
                });
            }
        };
        retain("cloud.", &mut self.agents);
        retain("cloud.azure.", &mut self.azure.agents);
        retain("cloud.boat.", &mut self.boat.agents);
        retain("cloud.coder.", &mut self.coder.agents);
        retain("cloud.daytona.", &mut self.daytona.agents);
        retain("cloud.railway.", &mut self.railway.agents);
        unknown
    }

    /// Composes every provider's script, so one too long, or with a NUL in
    /// it, is reported when the file loads rather than when a device is made.
    pub(super) fn check(&self) -> Result<()> {
        self.tables()
            .into_iter()
            .try_for_each(|(_, table)| self.script_with(table).map(drop))
    }
}

#[cfg(feature = "cloud")]
impl CloudConfig {
    /// The script a new device from `provider` runs, or `None`.
    pub(crate) fn script(&self, provider: herdr_client::CloudProvider) -> Result<Option<String>> {
        self.script_with(self.table(provider))
    }

    /// The agents `provider` installs, for its card's summary.
    pub(crate) fn agents(&self, provider: herdr_client::CloudProvider) -> Vec<Agent> {
        known(
            self.table(provider)
                .agents
                .as_deref()
                .or(self.agents.as_deref()),
        )
    }

    /// The user's own lines `provider` runs after its agents.
    pub(crate) fn custom(&self, provider: herdr_client::CloudProvider) -> Option<&str> {
        self.table(provider)
            .setup
            .as_deref()
            .or(self.setup.as_deref())
            .filter(|script| !script.trim().is_empty())
    }

    fn table(&self, provider: herdr_client::CloudProvider) -> &CloudSetup {
        const NONE: &CloudSetup = &CloudSetup {
            agents: None,
            setup: None,
        };
        self.tables()
            .into_iter()
            .find_map(|(key, table)| (key == provider.key()).then_some(table))
            .unwrap_or(NONE)
    }
}

/// The agents `keys` names, in the order they install.
fn known(keys: Option<&[String]>) -> Vec<Agent> {
    let keys = keys.unwrap_or_default();
    Agent::ALL
        .iter()
        .copied()
        .filter(|agent| keys.iter().any(|key| key == agent.key()))
        .collect()
}

#[cfg(feature = "cloud")]
impl Config {
    /// Write the shared agent list to the local override file.
    pub(crate) fn save_cloud_agents(agents: &[Agent]) -> Result<()> {
        Self::set_key("cloud.agents", agents_value(agents))
    }
}

/// `agents` as the TOML array `[cloud] agents` holds, in install order.
#[cfg(any(test, feature = "cloud"))]
pub(crate) fn agents_value(agents: &[Agent]) -> toml_edit::Value {
    let array: toml_edit::Array = Agent::ALL
        .iter()
        .filter(|agent| agents.contains(agent))
        .map(|agent| agent.key())
        .collect();
    array.into()
}
