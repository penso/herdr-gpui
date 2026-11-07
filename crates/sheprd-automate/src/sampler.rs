//! Measuring the agents automations started, once a minute: tokens from
//! their session files, CPU and memory from their panes' processes. An
//! agent counts as done, and stops counting toward the host's agent cap,
//! when its pane closes, its shell is back at the prompt, Herdr has called
//! it idle or done for ten minutes, or a day has passed. A read that fails
//! for any other reason, such as Herdr not answering, proves nothing: the
//! agent keeps counting and is read again next time.

use crate::{
    herdr::{self, Herdr},
    logbook::Logbook,
    paths::Paths,
    process::Meter,
    runner::{now, start_of_day},
    store::{AgentRow, Store},
    usage,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

pub const INTERVAL: Duration = Duration::from_secs(60);
const IDLE_DONE: i64 = 600;
const AT_MOST: i64 = 86_400;

/// Samples until `stop` is set; checks it every second.
pub fn run(paths: &Paths, herdr: &Herdr, log: &Logbook, stop: &Arc<AtomicBool>) {
    let mut sampler = Sampler::default();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    loop {
        match Store::open(&paths.database()) {
            Ok(store) => {
                if let Err(error) = sampler.sample(&store, herdr, &home) {
                    log.warn(format!("sampling agents: {error}"));
                }
            }
            Err(error) => log.warn(format!("sampling agents: {error}")),
        }
        for _ in 0..INTERVAL.as_secs() {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }
}

#[derive(Default)]
struct Sampler {
    meters: HashMap<(i64, u32), Meter>,
    idle_since: HashMap<(i64, u32), i64>,
}

impl Sampler {
    fn sample(
        &mut self,
        store: &Store,
        herdr: &Herdr,
        home: &std::path::Path,
    ) -> crate::Result<()> {
        let now = now();
        let day = start_of_day();
        for mut agent in store.agents(true)? {
            let key = (agent.run, agent.position);
            let pane = herdr.pane_agent(&agent.pane).ok();
            let process = herdr.process_info(&agent.pane);
            let gone = match &process {
                Ok(process) => process.at_prompt(),
                Err(herdr::Error::PaneNotFound { .. }) => true,
                Err(_) => false,
            };
            let process = process.ok();
            let session = pane.as_ref().and_then(|pane| pane.agent_session.as_ref());
            if let Some(tokens) =
                usage::tokens(&agent.harness, &agent.cwd, agent.started_at, session, home)
            {
                agent.tokens = Some(tokens);
            }
            if let Some(shell) = process.as_ref().and_then(|process| process.shell_pid) {
                let meter = self.meters.entry(key).or_insert_with(|| Meter::new(shell));
                meter.sample();
                agent.cpu_ms = meter.cpu_ms().or(agent.cpu_ms);
                agent.peak_memory = meter.peak_memory().max(agent.peak_memory);
            }
            let quiet = pane
                .as_ref()
                .and_then(|pane| pane.agent_status.as_deref())
                .is_some_and(|status| matches!(status, "idle" | "done"));
            if quiet {
                self.idle_since.entry(key).or_insert(now);
            } else {
                self.idle_since.remove(&key);
            }
            let idle_long = self
                .idle_since
                .get(&key)
                .is_some_and(|since| now - since >= IDLE_DONE);
            agent.sampled_at = Some(now);
            if gone || idle_long || now - agent.started_at >= AT_MOST {
                agent.done_at = Some(now);
                self.forget(key);
            }
            store.sample_agent(&agent, day)?;
        }
        Ok(())
    }

    fn forget(&mut self, key: (i64, u32)) {
        self.meters.remove(&key);
        self.idle_since.remove(&key);
    }
}

/// An agent's usage as reports describe it.
pub fn usage_of(agent: &AgentRow) -> sheprd_automation::report::Usage {
    sheprd_automation::report::Usage {
        tokens: agent.tokens,
        cpu_ms: agent.cpu_ms,
        peak_memory: agent.peak_memory,
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
