//! Moving a terminal agent into a chat: same pane, same conversation. The
//! agent's own CLI stops, and `herdr-acp --resume` continues its session in
//! the shell it leaves behind; discovery then attaches the chat view as for
//! any chat. The session's history stays with the agent, which the program
//! reads back (`sheprd-acp`'s `history`, or an ACP agent's `session/load`).
//!
//! Only an idle agent moves: stopping one mid-turn would lose the turn.
//! Everything that could refuse is checked before the agent is touched, so
//! a refusal leaves it running. A CLI stops on Ctrl+C alone, which every
//! agent here takes as "clear the draft" and then "quit". A signal is not
//! used because Codex dies on SIGTERM with its terminal modes still set, and
//! the line typed next would reach the shell in the wrong keyboard mode; a
//! typed `/exit` is not used because a draft would be sent with it.

mod error;

pub use error::Error;

use super::{Route, discovery, existing, restore::plan};
use crate::{
    SheprdWindow,
    i18n::{Localize, fl},
    teleport::{
        self, AgentKind, Envelope, Host, ProcessInfo, ProcessInfoResult, SessionKind,
        snapshot::PaneStatus,
    },
    window::Flash,
};
use gpui::Context;
use sheprd_chat_protocol::{Agent, restore};
use sheprd_client::{
    protocol::{AgentStatus, ClientShellAgent},
    shell_quote,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

/// How often the pane is asked whether its shell is back.
const POLL: Duration = Duration::from_millis(500);
/// How many times it is asked. Copilot takes about ten seconds to leave.
const ROUNDS: u32 = 60;
/// Ctrl+C presses sent, one each round: enough for a draft, a menu, and the
/// two presses that quit, with some to spare. Past these the agent only
/// needs time.
const INTERRUPTS: u32 = 8;

/// The agents that can move: Herdr reports their session, and either
/// `herdr-acp` reads their log or their ACP server replays it. Herdr has no
/// session for Gemini, and nothing shows Pi's history yet, so it would open
/// as an empty chat.
fn convertible(agent: Agent) -> bool {
    match agent {
        Agent::Claude | Agent::Codex | Agent::Copilot | Agent::Opencode => true,
        Agent::Gemini | Agent::Pi => false,
    }
}

/// Whether what the window sees of a pane lets its agent move.
fn check(reported: &ClientShellAgent) -> Result<Agent, Error> {
    if discovery::endpoint(&reported.tokens).is_some() {
        return Err(Error::AlreadyChat);
    }
    let name = reported.agent.as_deref().ok_or(Error::NoAgent)?;
    let agent: Agent = name.parse().map_err(|_| Error::Unknown(name.to_owned()))?;
    if !convertible(agent) {
        return Err(Error::Unsupported(agent));
    }
    match reported.agent_status {
        AgentStatus::Idle | AgentStatus::Done => Ok(agent),
        AgentStatus::Working | AgentStatus::Blocked | AgentStatus::Unknown => {
            Err(Error::Busy(agent))
        }
    }
}

/// Whether the host's own answer, fresher than the window's snapshot, still
/// shows `agent` settled in the pane. Only a turn known to be over may end:
/// an agent waiting for an answer, or one the daemon cannot place, stays.
fn settled(pane: &teleport::snapshot::Pane, agent: Agent) -> Result<(), Error> {
    if pane.agent.as_deref() != Some(agent.name()) {
        return Err(Error::Moved);
    }
    match pane.status {
        PaneStatus::Done | PaneStatus::Idle => Ok(()),
        PaneStatus::Other | PaneStatus::Working => Err(Error::Busy(agent)),
    }
}

/// What continues the conversation, read from the running agent.
#[derive(Debug, PartialEq, Eq)]
struct Resume {
    agent: Agent,
    cwd: String,
    /// The original command line's flags, for an agent that takes them.
    flags: Vec<String>,
    session: String,
}

impl Resume {
    /// The arguments of `herdr-acp`, none of them needing a shell.
    fn words(&self) -> Vec<&str> {
        let mut words = vec![
            "--resume",
            self.session.as_str(),
            "--cwd",
            self.cwd.as_str(),
            self.agent.name(),
        ];
        if !self.flags.is_empty() {
            words.push("--");
            words.extend(self.flags.iter().map(String::as_str));
        }
        words
    }

    /// The line typed into the pane's shell. Refusing happens here, before
    /// the agent stops, so a folder or flag no line can carry costs nothing.
    fn line(&self, program: Option<&Path>) -> Result<String, Error> {
        let words = self.words();
        let line = match program {
            Some(program) => {
                let program = program.to_str().ok_or(Error::Line)?;
                let mut quoted = vec![program];
                quoted.extend(&words);
                plan::quoted_line(&quoted)
            }
            None => remote_line(&words),
        };
        line.ok_or(Error::Line)
    }
}

/// The line for an SSH host, which finds its own program. The words ride
/// double-quoted inside a single-quoted script, so anything those read
/// specially makes no line.
fn remote_line(words: &[&str]) -> Option<String> {
    let special = |c: char| "'\"$`\\".contains(c) || c.is_control();
    if words.iter().any(|word| word.contains(special)) {
        return None;
    }
    let args: Vec<String> = words.iter().map(|word| format!("\"{word}\"")).collect();
    Some(format!(
        " sh -c '{}'",
        super::remote_program_script(&args.join(" "))
    ))
}

/// The session in the pane, as Herdr's own integration for `agent` reported
/// it, and in a form `herdr-acp --resume` takes.
fn session(pane: &teleport::snapshot::Pane, agent: Agent) -> Result<String, Error> {
    let reported = pane.agent_session.as_ref().ok_or(Error::NoSession(agent))?;
    AgentKind::parse(agent.name())
        .filter(|kind| kind.accepts(reported))
        .filter(|_| reported.kind == SessionKind::Id)
        .filter(|_| restore::session_id_is_plain(&reported.value))
        .map(|_| reported.value.clone())
        .ok_or(Error::NoSession(agent))
}

/// Whether the pane's shell, rather than a job it started, has the terminal.
fn shell_in_front(info: &ProcessInfo) -> bool {
    info.foreground_process_group_id.is_some() && info.foreground_process_group_id == info.shell_pid
}

/// What `info` says the pane's agent runs as, and the conversation to
/// continue. Claude's flags ride along, as the model the terminal agent was
/// started with; the other agents run a server with flags of its own.
fn resume(
    pane: &teleport::snapshot::Pane,
    info: &ProcessInfo,
    agent: Agent,
) -> Result<Resume, Error> {
    let session = session(pane, agent)?;
    let processes = existing::agent_processes(agent, info).unwrap_or_default();
    let process = processes.first().ok_or(Error::NotRunning(agent))?;
    let cwd = process
        .cwd
        .clone()
        .or_else(|| pane.foreground_cwd.clone())
        .or_else(|| pane.cwd.clone())
        .ok_or(Error::NotRunning(agent))?;
    let flags = match agent {
        Agent::Claude => AgentKind::Claude.kept_flags(&process.argv),
        _ => Vec::new(),
    };
    Ok(Resume {
        agent,
        cwd,
        flags,
        session,
    })
}

/// Stops the agent: Ctrl+C until the shell is back, at most [`INTERRUPTS`]
/// of them, then waits out the remaining rounds. It never presses Enter, so
/// nothing in the agent's input can be answered or sent. Once a key is sent
/// the agent may be leaving, so a question that fails, such as one dropped
/// SSH round trip, is asked again rather than abandoning the agent; a lost
/// connection ends the move through its fence.
fn stop(
    agent: Agent,
    mut returned: impl FnMut() -> Result<bool, Error>,
    mut interrupt: impl FnMut() -> Result<(), Error>,
    mut wait: impl FnMut(),
    cancelled: &AtomicBool,
) -> Result<(), Error> {
    for round in 0..ROUNDS {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Stopped);
        }
        match returned() {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if round == 0 => return Err(error),
            Err(_) => {}
        }
        if round < INTERRUPTS {
            interrupt()?;
        }
        wait();
    }
    Err(Error::StillRunning(agent))
}

fn process_info(host: &Host, pane: &str, cancelled: &AtomicBool) -> Result<ProcessInfo, Error> {
    let pane = shell_quote(pane);
    // i18n: data
    let body = format!("herdr_cli pane process-info --pane {pane}\n");
    let bytes = existing::query(host, &body, cancelled)?;
    serde_json::from_slice::<Envelope<ProcessInfoResult>>(&bytes)
        .map(|answer| answer.result.process_info)
        .map_err(Error::Answer)
}

/// What the window asks of its worker.
struct Request {
    agent: Agent,
    pane: String,
    /// This machine's program, or `None` for the one on an SSH host.
    program: Option<PathBuf>,
    scope: existing::Scope,
}

/// The worker's whole job: the line to type once the agent has stopped.
fn run(request: &Request, cancelled: &AtomicBool) -> Result<String, Error> {
    let Request {
        agent,
        pane,
        program,
        scope,
    } = request;
    let host = teleport::host_for(&scope.target).map_err(existing::Error::from)?;
    let snapshot = existing::snapshot(&host, cancelled)?.snapshot;
    let found = snapshot.pane(pane).ok_or(Error::Moved)?;
    settled(found, *agent)?;
    let line =
        resume(found, &process_info(&host, pane, cancelled)?, *agent)?.line(program.as_deref())?;
    if program.is_none() {
        // i18n: data
        let body = format!(" sh -c '{}'\n", super::remote_program_script("--version"));
        existing::query(&host, &body, cancelled).map_err(Error::HostProgram)?;
    }
    let keys = {
        let pane = shell_quote(pane);
        // i18n: data
        format!("herdr_cli pane send-keys {pane} ctrl+c\n")
    };
    stop(
        *agent,
        || process_info(&host, pane, cancelled).map(|info| shell_in_front(&info)),
        || Ok(existing::query(&host, &keys, cancelled).map(drop)?),
        || std::thread::sleep(POLL),
        cancelled,
    )?;
    Ok(line)
}

/// An agent on its way into a chat. Dropping it stops the worker from going
/// further, as a window that closes or a pane that moves on must.
#[derive(Debug)]
pub(crate) struct Job {
    cancelled: Arc<AtomicBool>,
    pane: String,
    result: mpsc::Receiver<Result<String, Error>>,
    scope: existing::Scope,
}

#[cfg(test)]
impl Job {
    /// A job whose worker is `result`'s sender, and the flag dropping it sets.
    pub(in crate::chat) fn waiting(
        pane: &str,
        scope: existing::Scope,
    ) -> (
        Self,
        mpsc::SyncSender<Result<String, Error>>,
        Arc<AtomicBool>,
    ) {
        let (sender, result) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let job = Self {
            cancelled: cancelled.clone(),
            pane: pane.to_owned(),
            result,
            scope,
        };
        (job, sender, cancelled)
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

fn spawn(request: Request) -> Result<Job, Error> {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let (sender, result) = mpsc::sync_channel(1);
    let pane = request.pane.clone();
    let scope = request.scope.clone();
    std::thread::Builder::new()
        .name("herdr-chat-convert".into())
        .spawn(move || {
            let _ = sender.send(run(&request, &flag));
        })
        .map_err(Error::Worker)?;
    Ok(Job {
        cancelled,
        pane,
        result,
        scope,
    })
}

impl SheprdWindow {
    /// Starts moving the agent in `pane`, else the focused pane, into a
    /// chat, and says which pane and agent. It finishes in the background.
    pub(crate) fn convert_to_chat(&mut self, pane: Option<&str>) -> Result<(String, Agent), Error> {
        if !self.config.features.acp_chats {
            return Err(Error::Disabled);
        }
        if self.chats.converting.is_some() {
            return Err(Error::AlreadyConverting);
        }
        let program = match self.chat_route().ok_or(Error::NotReachable)? {
            // `herdr pane process-info` and the typed `sh` line are Unix.
            Route::Local if cfg!(windows) => return Err(Error::NotReachable),
            Route::Local => Some(self.chats.program.found()?.to_owned()),
            Route::Ssh(_) => None,
        };
        let snapshot = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
            .ok_or(Error::NotConnected)?;
        let pane = pane
            .or(snapshot.focused_pane_id.as_deref())
            .ok_or(Error::NoAgent)?
            .to_owned();
        let reported = snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane)
            .ok_or(Error::NoAgent)?;
        let agent = check(reported)?;
        let endpoint = &self.endpoints[self.selected_endpoint];
        let scope = existing::Scope {
            target: endpoint.connection.target.clone(),
            boot: snapshot.boot_id.clone(),
            selection: self.selection_epoch,
            connection: endpoint.generation,
        };
        self.chats.converting = Some(spawn(Request {
            agent,
            pane: pane.clone(),
            program,
            scope,
        })?);
        Ok((pane, agent))
    }

    /// [`Self::convert_to_chat`], reporting a failure as a flash.
    pub(crate) fn convert_to_chat_or_flash(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.convert_to_chat(None) {
            self.flash_convert_error(&error, cx);
        }
        cx.notify();
    }

    fn flash_convert_error(&mut self, error: &Error, cx: &mut Context<Self>) {
        self.show_flash(
            Flash::warning(fl!("chat-convert-failed", error = error.localize())),
            cx,
        );
    }

    /// Why a move asked of `scope` must not go on, if it must not.
    fn convert_fence(&self, scope: &existing::Scope) -> Option<Error> {
        if !self.config.features.acp_chats {
            return Some(Error::Disabled);
        }
        (!self.convert_scope_current(scope)).then_some(Error::Stale)
    }

    /// Whether the window still shows the daemon `scope` was asked of.
    fn convert_scope_current(&self, scope: &existing::Scope) -> bool {
        self.live.status.is_connected()
            && self.selection_epoch == scope.selection
            && self
                .endpoints
                .get(self.selected_endpoint)
                .is_some_and(|endpoint| endpoint.generation == scope.connection)
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == scope.boot)
    }

    /// Runs every tick: once the agent has stopped, types the line that
    /// continues it. The chat view attaches when the program reports.
    ///
    /// The fence comes before the worker's answer, not after it: a window
    /// that left the host could never type the line, so the worker must
    /// stop pressing keys there rather than finish stopping the agent.
    pub(super) fn poll_chat_convert(&mut self, cx: &mut Context<Self>) {
        let Some(job) = &self.chats.converting else {
            return;
        };
        let result = match self.convert_fence(&job.scope) {
            Some(error) => Err(error),
            None => match job.result.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => Err(Error::Stopped),
            },
        };
        let Some(job) = self.chats.converting.take() else {
            return;
        };
        let typed = result.and_then(|line| {
            self.type_script_line(&job.scope.boot, &job.pane, &line)
                .map_err(|error| Error::Type(Arc::new(error)))
        });
        if let Err(error) = typed {
            self.flash_convert_error(&error, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
