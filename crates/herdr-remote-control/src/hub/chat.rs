//! One shared connection to a chat program, however many phones watch it.
//! A reader thread connects to the program's private endpoint, keeps the
//! transcript with the protocol's own reducer, and broadcasts each step; it
//! reconnects from its cursor when the connection drops, until the pane
//! leaves the layout. Phones read the transcript and subscribe; nothing on
//! the async side touches the socket.

use super::{Error, Result};
use herdr_chat_protocol::{
    ClientKind, ClientMessage, Command, CommandId, Cursor, ServerMessage, Transcript, VERSION,
    read_frame, write_frame,
};
use herdr_client::private_endpoint::{Closer, Connection};
use std::{
    collections::HashMap,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::Duration,
};
use tokio::sync::{broadcast, oneshot};

/// How long connecting waits for a free pipe instance on Windows.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// The longest wait between reconnects.
const MAX_RETRY: Duration = Duration::from_secs(10);
/// Steps queued per subscriber before it lags and reloads.
const SUBSCRIBERS_QUEUE: usize = 1024;
/// Commands waiting for the writer.
const MAX_COMMANDS: usize = 64;
/// How long a command waits for the program's `Ack`.
const ACK_TIMEOUT: Duration = Duration::from_secs(10);

/// One step a subscriber applies.
#[derive(Clone, Debug)]
pub(crate) enum Event {
    /// The transcript was replaced (a new connection or a new run): reload.
    /// It carries the snapshot, so a subscriber that rebuilds from it does
    /// not pick up changes queued after it.
    Reset {
        transcript: Arc<Transcript>,
        cursor: Cursor,
    },
    /// One change to the transcript, and where it leaves the stream.
    Change {
        change: Arc<herdr_chat_protocol::Change>,
        cursor: Cursor,
    },
    /// The program became reachable, or stopped being.
    Connection,
}

/// What a subscriber starts from.
#[derive(Clone, Debug)]
pub(crate) struct View {
    pub(crate) transcript: Transcript,
    pub(crate) connected: bool,
}

/// A chat as a list row shows it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Summary {
    pub(crate) connected: bool,
    pub(crate) status: herdr_chat_protocol::Status,
    /// The oldest question waiting for an answer.
    pub(crate) question: Option<herdr_chat_protocol::Permission>,
    pub(crate) questions: usize,
    /// The start of the agent's latest answer, on one line.
    pub(crate) last_text: Option<String>,
    pub(crate) turn_started_ms: Option<u64>,
    pub(crate) agent: String,
    pub(crate) usage_percent: Option<u8>,
}

/// The first `limit` characters of `text` on one line.
pub(crate) fn excerpt(text: &str, limit: usize) -> String {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= limit {
        return line;
    }
    let mut cut: String = line.chars().take(limit.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[derive(Default)]
struct State {
    cursor: Option<Cursor>,
    transcript: Transcript,
    connected: bool,
}

pub(crate) struct Chat {
    pub(crate) pane: String,
    state: Mutex<State>,
    events: broadcast::Sender<Event>,
    writer: Mutex<Option<SyncSender<ClientMessage>>>,
    acks: Mutex<HashMap<CommandId, oneshot::Sender<Option<herdr_chat_protocol::Rejection>>>>,
    closed: AtomicBool,
    closer: Mutex<Option<Closer>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Chat {
    /// Starts the reader for the program at `address`. `all` receives every
    /// step of every chat, tagged with its pane, for push notifications.
    pub(crate) fn start(
        pane: String,
        address: PathBuf,
        all: broadcast::Sender<(String, Event)>,
    ) -> std::io::Result<Arc<Self>> {
        let (events, _) = broadcast::channel(SUBSCRIBERS_QUEUE);
        let chat = Arc::new(Self {
            pane,
            state: Mutex::new(State::default()),
            events,
            writer: Mutex::new(None),
            acks: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            closer: Mutex::new(None),
        });
        let reader = chat.clone();
        thread::Builder::new()
            .name("herdr-remote-chat".into())
            .spawn(move || reader.run(&address, &all))?;
        Ok(chat)
    }

    /// The transcript now and a subscription to every step after it. The
    /// subscription is taken first, so a step between the two is seen
    /// twice at worst, which only renders the same view again.
    pub(crate) fn subscribe(&self) -> (View, broadcast::Receiver<Event>) {
        let receiver = self.events.subscribe();
        (self.view(), receiver)
    }

    pub(crate) fn view(&self) -> View {
        let state = lock(&self.state);
        View {
            transcript: state.transcript.clone(),
            connected: state.connected,
        }
    }

    /// The transcript now and the stream position it includes, for a
    /// subscriber that must skip the queued changes already in it.
    pub(crate) fn base(&self) -> (Transcript, Option<Cursor>) {
        let state = lock(&self.state);
        (state.transcript.clone(), state.cursor)
    }

    /// What a list row shows, without copying the transcript.
    pub(crate) fn summary(&self) -> Summary {
        let state = lock(&self.state);
        let transcript = &state.transcript;
        Summary {
            connected: state.connected,
            status: transcript.status.clone(),
            question: transcript.permissions.first().cloned(),
            questions: transcript.permissions.len(),
            last_text: transcript
                .entries
                .iter()
                .rev()
                .find_map(|entry| match &entry.kind {
                    herdr_chat_protocol::EntryKind::Agent { text } => Some(excerpt(text, 140)),
                    _ => None,
                }),
            turn_started_ms: transcript.turn.map(|turn| turn.started_ms),
            agent: transcript.agent.clone(),
            usage_percent: transcript
                .usage
                .as_ref()
                .map(herdr_chat_protocol::Usage::percent),
        }
    }

    /// Sends `command` and waits for the program's answer: `None` when it
    /// accepted it.
    pub(crate) async fn command(
        &self,
        command: Command,
    ) -> Result<Option<herdr_chat_protocol::Rejection>> {
        let id = CommandId::random();
        let (sender, receiver) = oneshot::channel();
        {
            let writer = lock(&self.writer);
            let writer = writer.as_ref().ok_or(Error::ChatUnavailable)?;
            let mut acks = lock(&self.acks);
            if acks.len() >= MAX_COMMANDS {
                return Err(Error::Busy);
            }
            writer
                .try_send(ClientMessage::Command { command, id })
                .map_err(|error| match error {
                    TrySendError::Full(_) => Error::Busy,
                    TrySendError::Disconnected(_) => Error::ChatUnavailable,
                })?;
            acks.insert(id, sender);
        }
        let answer = tokio::time::timeout(ACK_TIMEOUT, receiver).await;
        lock(&self.acks).remove(&id);
        answer
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::ChatUnavailable)
    }

    /// Ends the connection for good, without waiting.
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(closer) = self.closer.try_lock()
            && let Some(closer) = closer.as_ref()
        {
            closer.close();
        }
    }

    fn run(&self, address: &Path, all: &broadcast::Sender<(String, Event)>) {
        let mut retry = Duration::from_millis(500);
        while !self.closed.load(Ordering::SeqCst) {
            let since = lock(&self.state).cursor;
            if let Ok(connection) = Connection::connect(address, CONNECT_TIMEOUT)
                && let Ok((reader, writer, closer)) = connection.split()
            {
                retry = Duration::from_millis(500);
                self.connected(reader, writer, closer, since, all);
            }
            self.set_connected(false, all);
            thread::sleep(retry);
            retry = (retry * 2).min(MAX_RETRY);
        }
    }

    fn connected(
        &self,
        reader: impl Read,
        writer: impl Write + Send + 'static,
        closer: Closer,
        since: Option<Cursor>,
        all: &broadcast::Sender<(String, Event)>,
    ) {
        let Ok(writer_closer) = closer.try_clone() else {
            return;
        };
        if let Ok(clone) = closer.try_clone() {
            *lock(&self.closer) = Some(clone);
        }
        // A close that came while connecting is seen here.
        if self.closed.load(Ordering::SeqCst) {
            closer.close();
            return;
        }
        let (sender, queued) = mpsc::sync_channel(MAX_COMMANDS);
        let hello = ClientMessage::Hello {
            client: ClientKind::Remote,
            since,
            version: VERSION,
        };
        if thread::Builder::new()
            .name("herdr-remote-chat-writer".into())
            .spawn(move || write(writer, &writer_closer, hello, &queued))
            .is_err()
        {
            closer.close();
            return;
        }
        *lock(&self.writer) = Some(sender);
        let mut reader = BufReader::new(reader);
        while let Ok(Some(message)) = read_frame::<_, ServerMessage>(&mut reader) {
            if !self.receive(message, all) {
                break;
            }
        }
        *lock(&self.writer) = None;
        closer.close();
        // Commands in flight end with the connection.
        lock(&self.acks).clear();
    }

    /// Applies one message; false ends the connection.
    fn receive(&self, message: ServerMessage, all: &broadcast::Sender<(String, Event)>) -> bool {
        match message {
            ServerMessage::Welcome { .. } => {
                self.set_connected(true, all);
                true
            }
            ServerMessage::Snapshot { cursor, transcript } => {
                let transcript = Arc::new(*transcript);
                {
                    let mut state = lock(&self.state);
                    state.cursor = Some(cursor);
                    state.transcript = Transcript::clone(&transcript);
                }
                self.publish(Event::Reset { transcript, cursor }, all);
                true
            }
            ServerMessage::Change { change, cursor } => {
                let change = Arc::new(change);
                {
                    let mut state = lock(&self.state);
                    if state.transcript.apply(&change).is_err() {
                        // Start over from a snapshot.
                        state.cursor = None;
                        return false;
                    }
                    state.cursor = Some(cursor);
                }
                self.publish(Event::Change { change, cursor }, all);
                true
            }
            ServerMessage::Ack { id, rejected } => {
                if let Some(sender) = lock(&self.acks).remove(&id) {
                    let _ = sender.send(rejected);
                }
                true
            }
            ServerMessage::Bye { .. } => false,
        }
    }

    fn set_connected(&self, connected: bool, all: &broadcast::Sender<(String, Event)>) {
        let changed = {
            let mut state = lock(&self.state);
            std::mem::replace(&mut state.connected, connected) != connected
        };
        if changed {
            self.publish(Event::Connection, all);
        }
    }

    fn publish(&self, event: Event, all: &broadcast::Sender<(String, Event)>) {
        let _ = all.send((self.pane.clone(), event.clone()));
        let _ = self.events.send(event);
    }
}

/// The writer thread: the greeting, then each queued command in order,
/// until the connection ends or a write fails.
fn write(
    writer: impl Write,
    closer: &Closer,
    hello: ClientMessage,
    queued: &Receiver<ClientMessage>,
) {
    let mut writer = BufWriter::new(writer);
    if write_frame(&mut writer, &hello).is_ok() {
        while let Ok(message) = queued.recv() {
            if write_frame(&mut writer, &message).is_err() {
                break;
            }
        }
    }
    closer.close();
}
