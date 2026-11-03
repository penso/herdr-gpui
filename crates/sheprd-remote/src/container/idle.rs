//! Stopping containers nobody is using, when `[container] auto_stop` asks for
//! it. A container's agents keep working with no window attached, so a
//! closed window says nothing; instead, each minute, the container's own
//! Herdr is asked whether any agent is working or waiting for input. One
//! that has had none for `auto_stop` is stopped: its files stay in its
//! volume, and the next connection starts it again.

use super::{Engine, machine};
use crate::cloud::{CloudProvider, SavedDevice};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// How often the app checks for idle containers.
pub const CHECK: Duration = Duration::from_secs(60);
const PROBE: Duration = Duration::from_secs(20);

/// Prints the container's agents as Herdr's API describes them.
pub(super) const AGENTS: &str = r#"PATH="$HOME/.local/bin:$PATH"; exec herdr agent list"#;

/// Whether `herdr agent list` printed `output` for a container with an agent
/// at work. Only a reply that names every agent's status can say no: an
/// agent working, waiting for input, or whose state Herdr cannot tell keeps
/// the container, and so does a reply this build cannot read. A server that
/// is not running has no agents.
pub(super) fn busy_reply(output: &[u8]) -> bool {
    #[derive(serde::Deserialize)]
    struct Reply {
        result: Option<List>,
        error: Option<Failure>,
    }
    #[derive(serde::Deserialize)]
    struct List {
        agents: Vec<Agent>,
    }
    #[derive(serde::Deserialize)]
    struct Agent {
        agent_status: String,
    }
    #[derive(serde::Deserialize)]
    struct Failure {
        code: String,
    }
    let Ok(reply) = serde_json::from_slice::<Reply>(output) else {
        return true;
    };
    match (reply.result, reply.error) {
        (Some(list), _) => list
            .agents
            .iter()
            .any(|agent| !matches!(agent.agent_status.as_str(), "idle" | "done")),
        (None, Some(failure)) => failure.code != "server_not_running",
        (None, None) => true,
    }
}

/// When each container last had an agent at work, or was first seen running.
static BUSY_AT: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// What to do with one running container.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verdict {
    Keep,
    Stop,
}

/// Decide for the container `id`, given whether an agent is at work there
/// now. A container not seen before starts its timeout now, so one the app
/// has not watched since launch is never stopped at the first check.
pub(super) fn verdict(
    seen: &mut HashMap<String, Instant>,
    id: &str,
    busy: bool,
    now: Instant,
    auto_stop: Duration,
) -> Verdict {
    let since = seen.entry(id.to_owned()).or_insert(now);
    if busy {
        *since = now;
        return Verdict::Keep;
    }
    if now.duration_since(*since) >= auto_stop {
        seen.remove(id);
        return Verdict::Stop;
    }
    Verdict::Keep
}

fn busy(engine: Engine, id: &str) -> bool {
    let probe = [
        "exec",
        "--user",
        machine::USER,
        "--env",
        "HOME=/home/herdr",
        id,
        "/bin/sh",
        "-c",
        AGENTS,
    ];
    // A probe that cannot run says nothing; keeping the container is safe.
    match engine.output("exec", probe, PROBE, &|| false) {
        Ok((_, stdout, _)) => busy_reply(&stdout),
        Err(_) => true,
    }
}

fn check(devices: &[SavedDevice], auto_stop: Duration) {
    let now = Instant::now();
    for device in devices {
        let Some(engine) = Engine::from_key(&device.account) else {
            continue;
        };
        if machine::state(engine, &device.id, &|| false).ok() != Some(machine::State::Running) {
            if let Ok(mut seen) = BUSY_AT.lock() {
                seen.get_or_insert_with(HashMap::new).remove(&device.id);
            }
            continue;
        }
        let busy = busy(engine, &device.id);
        let Ok(mut seen) = BUSY_AT.lock() else {
            return;
        };
        let seen = seen.get_or_insert_with(HashMap::new);
        if verdict(seen, &device.id, busy, now, auto_stop) == Verdict::Stop {
            tracing::info!(
                category = "container_idle",
                id = device.id,
                "Stopping a container with no agent at work"
            );
            if let Err(error) = super::lifecycle::stop(engine, &device.id, || {
                machine::stop(engine, &device.id, &|| false)
            }) {
                tracing::warn!(category = "container_idle", id = device.id, %error, "Could not stop an idle container");
            }
        }
    }
}

/// Stops idle containers. `config` reads the `[container]` table, without
/// the maintenance a full load does; it runs only when a container is saved.
pub fn check_all(config: impl FnOnce() -> Option<crate::config::ContainerConfig>) {
    let Ok(devices) = crate::cloud::load() else {
        return;
    };
    let devices: Vec<_> = devices
        .into_iter()
        .filter(|device| device.provider == CloudProvider::Container)
        .collect();
    if devices.is_empty() {
        return;
    }
    let Some(config) = config() else {
        return;
    };
    let Ok(settings) = config.settings() else {
        return;
    };
    if let Some(auto_stop) = settings.auto_stop {
        check(&devices, auto_stop);
    }
}
