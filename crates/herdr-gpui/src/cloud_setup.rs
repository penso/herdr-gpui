//! The setup script a new cloud device runs the first time it connects: the
//! coding agents the user picked (see [`agents`]), then their own script, both
//! from `[cloud]` in this client's config or a provider's table under it.
//!
//! The script is the user's own, so it runs without the trust question a
//! repository's `.herdr/worktree.toml` gets, but like those scripts it runs in
//! a visible, focused tab (see `launch`), never in the background. A device
//! that finishes setting up queues its script app-wide (see `pending`), since
//! the window that connects to it may not be the one that created it.

mod agents;
#[cfg(feature = "cloud")]
mod launch;
#[cfg(feature = "cloud")]
mod pending;

pub(crate) use agents::Agent;
#[cfg(feature = "cloud")]
pub(crate) use launch::{PROVIDER_ENV, Running};
#[cfg(feature = "cloud")]
pub(crate) use pending::Pending;
#[cfg(all(test, feature = "cloud"))]
use pending::PendingSetups;

/// The longest script a device runs. It travels in `tab.create`'s
/// environment, so it stays well under the API's request bound.
pub(crate) const MAX_SCRIPT: usize = 32 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No setup script is configured for {0} devices; set [cloud] agents or setup")]
    NoScript(&'static str),
    #[error("No cloud device {0} in this window")]
    NoDevice(String),
    #[error("The cloud device setup script contains a NUL byte")]
    ScriptNul,
    #[error("The cloud device setup script is longer than {limit} bytes")]
    ScriptTooLong { limit: usize },
}

/// The script a device runs: each agent's install, in [`Agent::ALL`] order
/// and once, then the user's own lines. `None` when there is nothing to run.
pub(crate) fn compose(
    agents: impl IntoIterator<Item = Agent>,
    custom: Option<&str>,
) -> Result<Option<String>, Error> {
    let picked: Vec<Agent> = agents.into_iter().collect();
    let mut parts: Vec<&str> = Agent::ALL
        .iter()
        .filter(|agent| picked.contains(agent))
        .map(|agent| agent.script())
        .collect();
    if !parts.is_empty() {
        parts.push("echo '==> Agents installed; open a new tab to pick up PATH changes'");
    }
    parts.extend(custom.map(str::trim).filter(|custom| !custom.is_empty()));
    if parts.is_empty() {
        return Ok(None);
    }
    let script = parts.join("\n") + "\n";
    if script.contains('\0') {
        return Err(Error::ScriptNul);
    }
    if script.len() > MAX_SCRIPT {
        return Err(Error::ScriptTooLong { limit: MAX_SCRIPT });
    }
    Ok(Some(script))
}

/// What a provider's next device installs and runs, for the switch that
/// runs it.
#[cfg(feature = "cloud")]
pub(crate) fn summary(
    cloud: &crate::config::CloudConfig,
    provider: herdr_client::CloudProvider,
) -> String {
    let agents: Vec<&str> = cloud
        .agents(provider)
        .into_iter()
        .map(Agent::label)
        .collect();
    let lines = cloud
        .custom(provider)
        .map_or(0, |script| script.trim().lines().count());
    // Agent names are product names, listed as they are.
    let agents = agents.join(", ");
    match (agents.is_empty(), lines) {
        (true, 0) => crate::i18n::fl!("cloud-setup-summary-nothing"),
        (true, _) => crate::i18n::fl!("cloud-setup-summary-runs", lines = lines),
        (false, 0) => crate::i18n::fl!("cloud-setup-summary-installs", agents = agents),
        (false, _) => crate::i18n::fl!(
            "cloud-setup-summary-installs-runs",
            agents = agents,
            lines = lines
        ),
    }
}

#[cfg(test)]
mod tests;
