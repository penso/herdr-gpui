//! The conversation's single owner: it talks to the agent, keeps the
//! transcript, and fans every change out to the attached clients.
//!
//! Everything runs on one thread fed by one bounded input queue, so changes
//! have one order and a client's catch-up can never interleave with a change.

mod agent;
mod commands;
mod pictures;

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

use acp::v1 as wire;
use crossbeam_channel::{Sender, TrySendError};
use herdr_chat_protocol::{
    ByeReason, Change, Command, CommandId, Cursor, PermissionId, Queued, Rejection, ServerMessage,
    Status, Transcript, VERSION,
};
use uuid::Uuid;

use crate::{project::Projector, rpc};

/// Changes kept for clients that reconnect with a cursor.
const LOG: usize = 8192;
/// Bytes of logged changes kept, so a run of pictures cannot pin the log's
/// memory; a client older than the log gets a snapshot.
const LOG_BYTES: usize = 32 * 1024 * 1024;
/// Commands remembered so a resent one is not run twice.
const HANDLED: usize = 256;
/// The most entries a transcript keeps.
pub(crate) const MAX_ENTRIES: usize = 2000;
/// The most text a transcript keeps, so a snapshot always fits in one frame.
pub(crate) const MAX_TRANSCRIPT_TEXT: usize = 2 * 1024 * 1024;
/// The most image data a transcript keeps, apart from its text, so a
/// snapshot with both still fits in one frame.
pub(crate) const MAX_TRANSCRIPT_MEDIA: usize = 8 * 1024 * 1024;

/// A connection to the session: the in-pane view or an attached client.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ConnId(pub u64);

#[derive(Debug)]
pub(crate) enum Input {
    Agent(rpc::Inbound),
    /// The agent process ended; carries its last standard error lines.
    AgentExited {
        code: Option<i32>,
        stderr: Vec<String>,
    },
    Attach(Attach),
    Command {
        command: Command,
        conn: ConnId,
        id: CommandId,
    },
    Detach(ConnId),
    Shutdown,
    /// A command this version does not know, from a newer client: refused
    /// with an acknowledgement rather than by dropping the connection.
    Unsupported {
        conn: ConnId,
        id: CommandId,
    },
}

#[derive(Debug)]
pub(crate) struct Attach {
    pub conn: ConnId,
    /// The program's own view, which does not count as an attached client.
    pub local: bool,
    pub outbox: Sender<ServerMessage>,
    pub since: Option<Cursor>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Flow {
    Continue,
    Exit,
}

/// What Herdr is told about the pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Activity {
    Idle,
    Working,
    Blocked,
}

/// What the session passes on for Herdr.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Update {
    Activity(Activity),
    /// A prompt just sent to the agent, as the transcript shows it, which
    /// names the pane and its tab.
    Prompt(String),
}

pub(crate) struct Config {
    pub agent: String,
    pub cwd: PathBuf,
    /// Exit when the agent does, rather than leave its last words on screen.
    pub exit_with_agent: bool,
    pub load: Option<String>,
    pub login_hint: Option<String>,
    pub pane: Option<String>,
}

#[derive(Debug)]
enum Pending {
    Initialize,
    NewSession,
    LoadSession,
    Prompt,
    SetMode(String),
    SetOption,
}

struct Subscriber {
    conn: ConnId,
    local: bool,
    outbox: Sender<ServerMessage>,
}

struct Asked {
    options: Vec<String>,
    request: wire::RequestId,
}

pub(crate) struct Session<R: FnMut(Update)> {
    /// What the agent takes in a prompt besides text, from its handshake.
    accepts: crate::files::Accepts,
    acp_session: Option<wire::SessionId>,
    activity: Option<Activity>,
    asked: HashMap<PermissionId, Asked>,
    config: Config,
    epoch: Uuid,
    handled: VecDeque<(CommandId, Option<Rejection>)>,
    log: VecDeque<(u64, Change)>,
    /// What the logged changes weigh, by [`change_size`].
    log_bytes: usize,
    next_permission: u64,
    next_request: i64,
    outbox: rpc::Outbox,
    pending: HashMap<i64, Pending>,
    pictures: pictures::Pictures,
    projector: Projector,
    ready: bool,
    report: R,
    seq: u64,
    subscribers: Vec<Subscriber>,
    transcript: Transcript,
    turn: Option<i64>,
}

impl<R: FnMut(Update)> Session<R> {
    pub(crate) fn new(config: Config, outbox: rpc::Outbox, report: R) -> Self {
        let transcript = Transcript::new(config.agent.clone());
        Self {
            accepts: crate::files::Accepts::default(),
            acp_session: None,
            activity: None,
            asked: HashMap::new(),
            config,
            epoch: Uuid::new_v4(),
            handled: VecDeque::new(),
            log: VecDeque::new(),
            log_bytes: 0,
            next_permission: 0,
            next_request: 0,
            outbox,
            pending: HashMap::new(),
            pictures: pictures::Pictures::default(),
            projector: Projector::default(),
            ready: false,
            report,
            seq: 0,
            subscribers: Vec::new(),
            transcript,
            turn: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    /// Says something about the session in its transcript.
    pub(crate) fn notice(&mut self, level: herdr_chat_protocol::NoticeLevel, text: String) {
        let notice = self.projector.notice(level, text);
        self.emit(notice);
    }

    /// Opens the ACP connection.
    pub(crate) fn start(&mut self) {
        let request = wire::InitializeRequest::new(acp::ProtocolVersion::V1).client_info(
            wire::Implementation::new("herdr-acp", env!("CARGO_PKG_VERSION")),
        );
        self.request("initialize", &request, Pending::Initialize);
        self.report_activity();
    }

    pub(crate) fn handle(&mut self, input: Input) -> Flow {
        match input {
            Input::Agent(inbound) => self.agent(inbound),
            Input::AgentExited { code, stderr } => {
                self.exited(code, &stderr);
                if self.config.exit_with_agent {
                    self.bye(ByeReason::Exiting);
                    return Flow::Exit;
                }
            }
            Input::Attach(attach) => self.attach(attach),
            Input::Command { command, conn, id } => self.command(conn, id, command),
            Input::Detach(conn) => self.detach(conn),
            Input::Unsupported { conn, id } => {
                self.ack(conn, id, Some(Rejection::Unsupported));
            }
            Input::Shutdown => {
                self.bye(ByeReason::Exiting);
                return Flow::Exit;
            }
        }
        Flow::Continue
    }

    fn cursor(&self) -> Cursor {
        Cursor {
            epoch: self.epoch,
            seq: self.seq,
        }
    }

    /// Applies a change and sends it to every client.
    fn emit(&mut self, change: Change) {
        let change = self.pictures_before(change);
        if self.transcript.apply(&change).is_err() {
            // The session only emits changes made from its own transcript, so
            // one that does not fit is a bug; dropping it keeps every client
            // consistent with the transcript.
            return;
        }
        if let Change::Truncate { before } = &change {
            self.projector.forget_before(*before);
        }
        self.seq += 1;
        let size = change_size(&change);
        while self.log.len() == LOG || (self.log_bytes + size > LOG_BYTES && !self.log.is_empty()) {
            if let Some((_, old)) = self.log.pop_front() {
                self.log_bytes -= change_size(&old);
            }
        }
        self.log_bytes += size;
        self.log.push_back((self.seq, change.clone()));
        let message = ServerMessage::Change {
            change: change.clone(),
            cursor: self.cursor(),
        };
        let before = self.remote_clients();
        // A client that cannot keep up is dropped; its connection closes and
        // it reconnects with its cursor.
        self.subscribers.retain(
            |subscriber| match subscriber.outbox.try_send(message.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
            },
        );
        if matches!(
            change,
            Change::Push { .. } | Change::Append { .. } | Change::Replace { .. }
        ) {
            self.bound();
        }
        if self.remote_clients() != before {
            self.emit(Change::Clients {
                count: self.remote_clients(),
            });
        }
        self.report_activity();
        self.pictures_after(&change);
    }

    fn emit_all(&mut self, changes: impl IntoIterator<Item = Change>) {
        for change in changes {
            self.emit(change);
        }
    }

    /// Drops the oldest entries once the transcript holds too many or too much.
    fn bound(&mut self) {
        let drop = keep_from(&self.transcript.entries);
        if drop > 0 {
            let before = self.transcript.entries[drop].id;
            self.emit(Change::Truncate { before });
        }
    }

    fn remote_clients(&self) -> u32 {
        let count = self.subscribers.iter().filter(|s| !s.local).count();
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    fn report_activity(&mut self) {
        let activity = if self.transcript.is_blocked() {
            Activity::Blocked
        } else if matches!(self.transcript.status, Status::Working) {
            Activity::Working
        } else {
            Activity::Idle
        };
        if self.activity != Some(activity) {
            self.activity = Some(activity);
            (self.report)(Update::Activity(activity));
        }
    }

    fn status(&mut self, status: Status) {
        if self.transcript.status != status {
            self.emit(Change::Status { status });
        }
    }

    fn queue_changed(&mut self, queued: Vec<Queued>) {
        self.emit(Change::Queue { queued });
    }

    fn attach(&mut self, attach: Attach) {
        let Attach {
            conn,
            local,
            outbox,
            since,
        } = attach;
        let welcome = ServerMessage::Welcome {
            agent: self.config.agent.clone(),
            pane: self.config.pane.clone(),
            version: VERSION,
        };
        if outbox.try_send(welcome).is_err() {
            return;
        }
        let first = self.log.front().map_or(self.seq + 1, |(seq, _)| *seq);
        let catch_up = since.filter(|since| {
            since.epoch == self.epoch && since.seq <= self.seq && since.seq + 1 >= first
        });
        let sent = match catch_up {
            Some(since) => {
                self.log
                    .iter()
                    .filter(|(seq, _)| *seq > since.seq)
                    .all(|(seq, change)| {
                        outbox
                            .try_send(ServerMessage::Change {
                                change: change.clone(),
                                cursor: Cursor {
                                    epoch: self.epoch,
                                    seq: *seq,
                                },
                            })
                            .is_ok()
                    })
            }
            None => outbox
                .try_send(ServerMessage::Snapshot {
                    cursor: self.cursor(),
                    transcript: Box::new(self.transcript.clone()),
                })
                .is_ok(),
        };
        if !sent {
            return;
        }
        self.subscribers.push(Subscriber {
            conn,
            local,
            outbox,
        });
        if !local {
            self.emit(Change::Clients {
                count: self.remote_clients(),
            });
        }
    }

    fn detach(&mut self, conn: ConnId) {
        let before = self.remote_clients();
        self.subscribers
            .retain(|subscriber| subscriber.conn != conn);
        if self.remote_clients() != before {
            self.emit(Change::Clients {
                count: self.remote_clients(),
            });
        }
    }

    fn bye(&mut self, reason: ByeReason) {
        for subscriber in self.subscribers.drain(..) {
            let _ = subscriber.outbox.try_send(ServerMessage::Bye { reason });
        }
    }

    fn ack(&mut self, conn: ConnId, id: CommandId, rejected: Option<Rejection>) {
        if self.handled.len() == HANDLED {
            self.handled.pop_front();
        }
        self.handled.push_back((id, rejected));
        self.send_to(conn, ServerMessage::Ack { id, rejected });
    }

    fn send_to(&mut self, conn: ConnId, message: ServerMessage) {
        if let Some(subscriber) = self.subscribers.iter().find(|s| s.conn == conn)
            && subscriber.outbox.try_send(message).is_err()
        {
            self.detach(conn);
        }
    }

    fn request(&mut self, method: &str, params: &impl serde::Serialize, pending: Pending) -> bool {
        self.next_request += 1;
        let id = self.next_request;
        match self.outbox.request(id, method, params) {
            Ok(()) => {
                self.pending.insert(id, pending);
                true
            }
            Err(error) => {
                self.fail(&error.to_string());
                false
            }
        }
    }

    /// The session cannot continue: say why, and stop taking prompts.
    fn fail(&mut self, message: &str) {
        let notice = self
            .projector
            .notice(herdr_chat_protocol::NoticeLevel::Error, message);
        self.emit(notice);
        self.status(Status::Failed {
            message: message.to_owned(),
        });
        self.clear_queue();
    }

    fn clear_queue(&mut self) {
        if !self.transcript.queued.is_empty() {
            self.queue_changed(Vec::new());
        }
    }
}

/// The wall clock in milliseconds since the Unix epoch, which clients on the
/// same machine count a running turn from.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// The index of the oldest entry a transcript keeps: at most [`MAX_ENTRIES`]
/// and within both byte budgets, but always the newest entry. Live chats and
/// mirrored logs share it so neither can hold more than a snapshot frame.
pub(crate) fn keep_from(entries: &[herdr_chat_protocol::Entry]) -> usize {
    let mut drop = entries.len().saturating_sub(MAX_ENTRIES);
    let kept = &entries[drop..];
    let mut text: usize = kept.iter().map(entry_size).sum();
    let mut media: usize = kept.iter().map(entry_media).sum();
    while (text > MAX_TRANSCRIPT_TEXT || media > MAX_TRANSCRIPT_MEDIA) && drop + 1 < entries.len() {
        text -= entry_size(&entries[drop]);
        media -= entry_media(&entries[drop]);
        drop += 1;
    }
    drop
}

/// Roughly how many bytes an entry adds to a snapshot.
pub(crate) fn entry_size(entry: &herdr_chat_protocol::Entry) -> usize {
    use herdr_chat_protocol::{EntryKind, ToolContent};
    match &entry.kind {
        EntryKind::Notice {
            answers,
            details,
            text,
            ..
        } => {
            text.len()
                + details.as_ref().map_or(0, String::len)
                + answers
                    .iter()
                    .map(|answer| answer.question.len() + answer.answer.len())
                    .sum::<usize>()
        }
        EntryKind::Tool(tool) => {
            tool.title.len()
                + tool.locations.iter().map(String::len).sum::<usize>()
                + tool
                    .content
                    .iter()
                    .map(|content| match content {
                        ToolContent::Text { text } => text.len(),
                        ToolContent::Diff { new, old, path } => {
                            new.len() + old.as_ref().map_or(0, String::len) + path.len()
                        }
                        ToolContent::Terminal { id } => id.len(),
                        ToolContent::Image { .. } => 0,
                    })
                    .sum::<usize>()
        }
        EntryKind::Turn(_) => 32,
        kind => kind.text().map_or(0, str::len),
    }
}

/// The image data an entry adds to a snapshot, counted apart from its text.
fn entry_media(entry: &herdr_chat_protocol::Entry) -> usize {
    use herdr_chat_protocol::{EntryKind, ToolContent};
    match &entry.kind {
        EntryKind::Image { image, .. } => image.data.len(),
        EntryKind::Tool(tool) => tool
            .content
            .iter()
            .map(|content| match content {
                ToolContent::Image { image } => image.data.len(),
                _ => 0,
            })
            .sum(),
        _ => 0,
    }
}

/// Roughly how many bytes a logged change holds.
fn change_size(change: &Change) -> usize {
    match change {
        Change::Push { entry } | Change::Replace { entry } => {
            entry_size(entry) + entry_media(entry)
        }
        Change::Append { text, .. } => text.len(),
        _ => 64,
    }
}

#[cfg(test)]
mod tests;
