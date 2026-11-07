//! Going back to before an earlier prompt with an agent that can only do so
//! when it starts: the running process is replaced with one forked from that
//! point. An agent that can do it while running answers
//! [`crate::harness::out::REWIND`] instead.

mod error;
mod log;

pub use error::Error;

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use crossbeam_channel::Sender;

use crate::{
    harness::{Pipes, Reach, out},
    process::{self, AgentProcess},
    rpc,
    session::{Input, Restarted},
};

/// Starts the chat's agent process and keeps it, so a rewind can replace it
/// and whoever ends the chat stops the one running then.
#[derive(Clone)]
pub(crate) struct Launcher {
    cwd: PathBuf,
    env: Vec<(&'static str, &'static str)>,
    /// Arguments the user gave for the agent, kept across a restart.
    extra: Vec<OsString>,
    /// Bumped per process: what an earlier process still says or does as it
    /// ends is not this chat's business any more.
    generation: Arc<AtomicU64>,
    inputs: Sender<Input>,
    process: Arc<Mutex<Option<AgentProcess>>>,
    program: OsString,
}

impl Launcher {
    pub(crate) fn new(
        program: OsString,
        env: Vec<(&'static str, &'static str)>,
        extra: Vec<OsString>,
        cwd: &Path,
        inputs: Sender<Input>,
    ) -> Self {
        Self {
            cwd: cwd.to_owned(),
            env,
            extra,
            generation: Arc::default(),
            inputs,
            process: Arc::default(),
            program,
        }
    }

    /// Starts the agent with `args`, wired to the session through `reach`.
    pub(crate) fn start(
        &self,
        mut args: Vec<OsString>,
        reach: Box<dyn Reach>,
    ) -> crate::Result<rpc::Outbox> {
        args.extend(self.extra.iter().cloned());
        let spawned = process::spawn(&self.program, &args, &self.env, &self.cwd)?;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = Arc::clone(&self.generation);
        let to_session = self.inputs.clone();
        let outbox = reach.connect(
            Pipes {
                input: Box::new(spawned.input),
                output: Box::new(spawned.output),
            },
            Box::new(move |inbound| {
                current.load(Ordering::SeqCst) == generation
                    && to_session.send(Input::Agent(inbound)).is_ok()
            }),
        )?;
        let watched = spawned.process.clone();
        let waiter = self.inputs.clone();
        let current = Arc::clone(&self.generation);
        thread::Builder::new()
            .name("acp-wait".to_owned())
            .spawn(move || {
                let code = watched.wait().ok().and_then(|status| status.code());
                let stderr = watched.stderr_tail();
                if current.load(Ordering::SeqCst) == generation {
                    let _ = waiter.send(Input::AgentExited { code, stderr });
                }
            })?;
        *self.slot() = Some(spawned.process);
        Ok(outbox)
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<AgentProcess>> {
        self.process.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stops the running agent, whose ending is then nothing to report.
    pub(crate) fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let process = self.slot().take();
        if let Some(process) = process {
            process.stop();
        }
    }

    /// Replaces the running `claude` with a fork of its session that ends
    /// just before the prompt. A prompt that opened the session leaves
    /// nothing to fork, so a new session starts. The log is read first: a
    /// prompt it does not have stops nothing.
    pub(crate) fn rewind_claude(&self, rewind: &out::Rewind) -> crate::Result<Restarted> {
        let path =
            crate::history::claude_log(&rewind.session_id, &self.cwd).map_err(Error::History)?;
        let anchor = log::claude_anchor(&path, rewind)?;
        let session = uuid::Uuid::new_v4().to_string();
        let args = crate::claude::fork_args(&rewind.session_id, anchor.as_deref(), &session);
        self.stop();
        let outbox = self.start(args, crate::claude::reach(session.clone()))?;
        Ok(Restarted { outbox, session })
    }
}

// The fake agents are shell commands.
#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
