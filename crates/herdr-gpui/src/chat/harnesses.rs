//! Which agents a new chat can run on the selected host: those whose own
//! ACP command `herdr-acp agents` finds there, run here off the UI thread or
//! on an SSH host over `ssh`. The answer is kept per Herdr connection and
//! asked again now and then, so an agent installed meanwhile shows up.

use super::{Route, discovery, start::Program};
use crate::HerdrWindow;
use gpui::prelude::*;
use herdr_chat_protocol::Agent;
use herdr_client::ScriptLimits;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

/// How often the answer is asked again on this machine, and over SSH.
const LOCAL_REFRESH: Duration = Duration::from_secs(60);
const REMOTE_REFRESH: Duration = Duration::from_secs(300);
/// The answer is a handful of short lines.
const LIMITS: ScriptLimits = ScriptLimits {
    output: 4096,
    idle: Duration::from_secs(20),
};

/// The installed agents `output` names, one per line, in table order;
/// unknown names, as from a newer program, are skipped.
pub(crate) fn parse(output: &[u8]) -> Vec<Agent> {
    let text = String::from_utf8_lossy(output);
    let named: Vec<Agent> = text
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();
    Agent::ALL
        .iter()
        .copied()
        .filter(|agent| named.contains(agent))
        .collect()
}

#[derive(Debug)]
struct Probe {
    /// The last answer; `None` until one came, or when the last ask failed.
    agents: Option<Vec<Agent>>,
    asked: Instant,
    running: bool,
}

/// The installed agents of each Herdr connection, by its id.
#[derive(Debug, Default)]
pub(crate) struct Harnesses {
    probes: HashMap<String, Probe>,
}

impl Harnesses {
    pub(crate) fn agents(&self, endpoint: &str) -> Option<&[Agent]> {
        self.probes.get(endpoint)?.agents.as_deref()
    }

    /// Whether `endpoint` should be asked now; marks it asked when so.
    fn begin(&mut self, endpoint: &str, refresh: Duration, now: Instant) -> bool {
        match self.probes.get_mut(endpoint) {
            Some(probe) if probe.running || now.duration_since(probe.asked) < refresh => false,
            Some(probe) => {
                probe.running = true;
                probe.asked = now;
                true
            }
            None => {
                self.probes.insert(
                    endpoint.to_owned(),
                    Probe {
                        agents: None,
                        asked: now,
                        running: true,
                    },
                );
                true
            }
        }
    }

    /// Sets `endpoint`'s answer, as a probe would.
    #[cfg(test)]
    pub(crate) fn record(&mut self, endpoint: &str, agents: Vec<Agent>) {
        self.begin(endpoint, Duration::ZERO, Instant::now());
        self.finish(endpoint, Some(agents));
    }

    /// Records an answer; a failed ask keeps the last one known.
    fn finish(&mut self, endpoint: &str, agents: Option<Vec<Agent>>) {
        if let Some(probe) = self.probes.get_mut(endpoint) {
            probe.running = false;
            if agents.is_some() {
                probe.agents = agents;
            }
        }
    }
}

/// Where a probe runs: this app's program here, or the host's own.
enum Probing {
    Local(PathBuf),
    Ssh(String),
}

impl Probing {
    /// Runs `herdr-acp agents`; blocking, for the background executor.
    fn run(self) -> Option<Vec<Agent>> {
        // Held until the lookup's child is reaped below.
        let (command, _channel) = match self {
            Self::Local(program) => {
                let mut command = herdr_client::new_command(program);
                command.arg("agents");
                if let Some(path) = crate::login_env::path() {
                    command.env("PATH", path);
                }
                (command, None)
            }
            Self::Ssh(host) => {
                let script = super::remote_program_script("agents");
                herdr_client::shared_script_command(&host, &script, herdr_client::SshLane::Quick)
                    .ok()?
            }
        };
        let mut output = Vec::new();
        let cancelled = AtomicBool::new(false);
        herdr_client::run_script_command(
            command,
            std::io::empty(),
            &mut output,
            LIMITS,
            &cancelled,
        )
        .ok()?;
        Some(parse(&output))
    }
}

impl HerdrWindow {
    /// Runs every tick: asks the selected host which agents it has, when
    /// its answer is missing or old.
    pub(super) fn probe_chat_agents(&mut self, cx: &mut Context<Self>) {
        let Some(endpoint) = self.endpoints.get(self.selected_endpoint) else {
            return;
        };
        let (probing, refresh) = match discovery::route(&endpoint.connection.target, &self.live) {
            Some(Route::Local) => match &self.chats.program {
                Program::Found(program) => (Probing::Local(program.clone()), LOCAL_REFRESH),
                _ => return,
            },
            Some(Route::Ssh(host)) => (Probing::Ssh(host), REMOTE_REFRESH),
            None => return,
        };
        let id = endpoint.id.clone();
        if !self.chats.agents.begin(&id, refresh, Instant::now()) {
            return;
        }
        let answer = cx.background_spawn(async move { probing.run() });
        cx.spawn(async move |this, cx| {
            let agents = answer.await;
            let _ = this.update(cx, |this, cx| {
                this.chats.agents.finish(&id, agents);
                cx.notify();
            });
        })
        .detach();
    }

    /// The agents a new chat may run on the selected host: the installed
    /// ones once known, none before.
    pub(crate) fn chat_agents(&self) -> Vec<Agent> {
        if !self.can_start_chat() {
            return Vec::new();
        }
        self.endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| self.chats.agents.agents(&endpoint.id))
            .map(<[Agent]>::to_vec)
            .unwrap_or_default()
    }

    /// Whether `agent` is known not to be installed on the selected host.
    pub(super) fn chat_agent_missing(&self, agent: Agent) -> bool {
        self.endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| self.chats.agents.agents(&endpoint.id))
            .is_some_and(|agents| !agents.contains(&agent))
    }
}

#[cfg(test)]
mod tests;
