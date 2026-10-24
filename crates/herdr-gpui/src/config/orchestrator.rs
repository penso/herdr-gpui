//! The `[orchestrator]` table: how the Issues & PRs tab starts agents.
//!
//! ```toml
//! [orchestrator.agents.opencode]
//! args = ["--standalone"]
//! review_args = ["--standalone", "--agent", "review"]
//! ```
//!
//! Each agent's table, named as Herdr's `agent start --kind` names it, holds
//! the arguments its executable starts with: `args` for a dispatch and
//! `review_args` for Review with agent, which falls back to `args`. They are
//! one TOML string per argument and reach the agent as written, through
//! Herdr, never a shell. The dispatch dialog shows them in its Arguments
//! field, where one dispatch can change them.

use crate::teleport::AgentKind;
use herdr_orchestrator::actions::{MAX_ARG_BYTES, MAX_ARGS, valid_args};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "orchestrator.agents.{agent}.{key} holds at most {MAX_ARGS} arguments of at most {MAX_ARG_BYTES} bytes each, without control characters"
    )]
    Arguments { agent: String, key: &'static str },
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct OrchestratorConfig {
    /// By the agent's Herdr name; [`Self::retain_known`] drops the others.
    agents: BTreeMap<String, AgentLaunch>,
}

/// The arguments one agent starts with.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct AgentLaunch {
    args: Vec<String>,
    review_args: Option<Vec<String>>,
}

impl OrchestratorConfig {
    /// Drops the tables of agents this build cannot start, naming them as
    /// unknown keys, so a config written for a newer build still loads.
    pub(super) fn retain_known(&mut self) -> Vec<String> {
        let mut unknown = Vec::new();
        self.agents.retain(|name, _| {
            let known = AgentKind::parse(name).is_some_and(|kind| kind.name() == name);
            if !known {
                unknown.push(format!("orchestrator.agents.{name}"));
            }
            known
        });
        unknown
    }

    /// Refuses arguments Herdr would refuse, so the mistake shows at load
    /// rather than as a failed dispatch.
    pub(super) fn check(&self) -> Result<(), Error> {
        for (agent, launch) in &self.agents {
            let lists = [
                ("args", Some(&launch.args)),
                ("review_args", launch.review_args.as_ref()),
            ];
            for (key, args) in lists {
                if args.is_some_and(|args| !valid_args(args)) {
                    return Err(Error::Arguments {
                        agent: agent.clone(),
                        key,
                    });
                }
            }
        }
        Ok(())
    }

    /// What `kind` starts with: its review arguments for a review, when
    /// set, else its arguments; none when the config names no table for it.
    pub(crate) fn args(&self, kind: AgentKind, review: bool) -> &[String] {
        let Some(launch) = self.agents.get(kind.name()) else {
            return &[];
        };
        match &launch.review_args {
            Some(args) if review => args,
            _ => &launch.args,
        }
    }
}
