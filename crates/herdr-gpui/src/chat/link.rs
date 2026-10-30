//! One connection to a chat program. A reader thread connects, starts a
//! writer thread, and reads; the window only ever polls and queues, through
//! bounded channels, so no socket I/O or thread join reaches the UI thread.
//! The endpoint is `herdr_client::private_endpoint`: a socket on Unix, and on
//! Windows a pipe checked to be this user's before anything is sent. A chat
//! on an SSH host is reached through a child process instead, `ssh` running
//! `herdr-acp connect` there, whose standard streams carry the same frames.
//! A reader that gets ahead of the window by [`MAX_EVENTS`] messages gives the
//! connection up rather than buffering more, and the window reconnects from
//! its cursor.

use super::{Error, MAX_EVENTS, MAX_PENDING, Result};
use herdr_chat_protocol::{
    ClientKind, ClientMessage, Cursor, REVISION, ServerMessage, VERSION, read_frame, write_frame,
};
use herdr_client::private_endpoint::Connection;
use std::{
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
    time::Duration,
};

/// How long connecting waits for a free pipe instance on Windows.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Commands queued for the writer: room for every command a session may
/// have waiting, resent on a new connection, and as many more.
const MAX_COMMANDS: usize = 2 * MAX_PENDING;

/// What one poll of a link found.
#[derive(Debug, Default)]
pub(crate) struct Polled {
    pub(crate) messages: Vec<ServerMessage>,
    /// Why the connection ended, once every message before it was taken.
    pub(crate) end: Option<Error>,
}

/// How a link reaches its program.
pub(crate) enum Dial {
    /// The program's private endpoint on this machine.
    Socket(PathBuf),
    /// A child whose stdin and stdout carry the connection, such as `ssh`
    /// running `herdr-acp connect` on the program's host.
    Process(Command),
}

impl From<PathBuf> for Dial {
    fn from(path: PathBuf) -> Self {
        Self::Socket(path)
    }
}

/// Ends a connection from any thread without waiting: shuts the socket
/// down, or kills the child, whose reader then reaps it.
enum Closer {
    Socket(herdr_client::private_endpoint::Closer),
    Process(Arc<Mutex<Child>>),
}

impl Closer {
    fn try_clone(&self) -> std::io::Result<Self> {
        Ok(match self {
            Self::Socket(closer) => Self::Socket(closer.try_clone()?),
            Self::Process(child) => Self::Process(child.clone()),
        })
    }

    fn close(&self) {
        match self {
            Self::Socket(closer) => closer.close(),
            // Held only while the reader reaps a child already killed.
            Self::Process(child) => {
                if let Ok(mut child) = child.try_lock() {
                    let _ = child.kill();
                }
            }
        }
    }

    /// Reaps a killed child; called only from the reader thread.
    fn reap(&self) {
        if let Self::Process(child) = self
            && let Ok(mut child) = child.lock()
        {
            let _ = child.wait();
        }
    }
}

type Parts = (Box<dyn Read + Send>, Box<dyn Write + Send>, Closer);

/// Connects as `dial` says, into a reader, a writer, and their closer.
fn open(dial: Dial) -> std::io::Result<Parts> {
    match dial {
        Dial::Socket(path) => {
            let (reader, writer, closer) = Connection::connect(&path, CONNECT_TIMEOUT)?.split()?;
            Ok((Box::new(reader), Box::new(writer), Closer::Socket(closer)))
        }
        Dial::Process(mut command) => {
            let mut child = command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let streams = child.stdout.take().zip(child.stdin.take());
            let closer = Closer::Process(Arc::new(Mutex::new(child)));
            let Some((stdout, stdin)) = streams else {
                closer.close();
                closer.reap();
                return Err(std::io::ErrorKind::BrokenPipe.into());
            };
            Ok((Box::new(stdout), Box::new(stdin), closer))
        }
    }
}

pub(crate) struct Link {
    commands: SyncSender<ClientMessage>,
    events: Receiver<ServerMessage>,
    end: Arc<Mutex<Option<Error>>>,
    socket: Arc<Socket>,
}

/// The connection's closer, kept so dropping the link closes it at once: the
/// threads end on their own, and nothing joins them.
#[derive(Default)]
struct Socket {
    closed: AtomicBool,
    closer: Mutex<Option<Closer>>,
}

impl Socket {
    /// Keeps `closer`, closing at once if the link was dropped while it
    /// connected. The flag is read after the store, so a close that missed
    /// the connection is seen here.
    fn attach(&self, closer: &Closer) {
        if let Ok(clone) = closer.try_clone()
            && let Ok(mut slot) = self.closer.lock()
        {
            *slot = Some(clone);
        }
        if self.closed.load(Ordering::SeqCst) {
            closer.close();
        }
    }

    /// Never waits: a connection being attached meanwhile sees the flag.
    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        if let Ok(slot) = self.closer.try_lock()
            && let Some(closer) = slot.as_ref()
        {
            closer.close();
        }
    }
}

impl Link {
    /// Starts connecting to the program `dial` reaches, resuming after
    /// `since`. `resend` goes out right after the greeting: commands an
    /// earlier connection may not have delivered, which the program runs once.
    pub(crate) fn start(
        dial: impl Into<Dial>,
        since: Option<Cursor>,
        resend: impl IntoIterator<Item = ClientMessage>,
    ) -> Result<Self> {
        let dial = dial.into();
        let (commands, queued) = mpsc::sync_channel(MAX_COMMANDS);
        for message in resend {
            commands.try_send(message).map_err(|_| Error::Busy)?;
        }
        let (sender, events) = mpsc::sync_channel(MAX_EVENTS);
        let end = Arc::new(Mutex::new(None));
        let socket = Arc::new(Socket::default());
        let (thread_end, thread_socket) = (end.clone(), socket.clone());
        thread::Builder::new()
            .name("herdr-gui-chat".into())
            .spawn(move || {
                let error = run(dial, since, queued, &sender, &thread_socket);
                if let Ok(mut slot) = thread_end.lock() {
                    *slot = Some(error);
                }
                // The sender drops after the reason is stored, so a window
                // that sees the channel close always finds it.
                drop(sender);
            })
            .map_err(Error::Spawn)?;
        Ok(Self {
            commands,
            events,
            end,
            socket,
        })
    }

    /// Queues `message` for the writer without waiting.
    pub(crate) fn send(&self, message: ClientMessage) -> Result<()> {
        self.commands
            .try_send(message)
            .map_err(|error| match error {
                TrySendError::Full(_) => Error::Busy,
                TrySendError::Disconnected(_) => Error::NotAttached,
            })
    }

    /// Takes at most `budget` messages, and the reason the connection ended
    /// once none are left before it.
    pub(crate) fn poll(&self, budget: usize) -> Polled {
        let mut polled = Polled::default();
        while polled.messages.len() < budget {
            match self.events.try_recv() {
                Ok(message) => polled.messages.push(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    let end = self.end.try_lock().ok().and_then(|mut end| end.take());
                    polled.end = Some(end.unwrap_or(Error::Closed));
                    break;
                }
            }
        }
        polled
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.socket.close();
    }
}

/// The reader thread: connects, greets through the writer, and reads until
/// the connection ends, returning why.
fn run(
    dial: Dial,
    since: Option<Cursor>,
    queued: Receiver<ClientMessage>,
    events: &SyncSender<ServerMessage>,
    socket: &Socket,
) -> Error {
    let parts = open(dial).and_then(|(reader, writer, closer)| {
        let writer_closer = closer.try_clone()?;
        Ok((reader, writer, closer, writer_closer))
    });
    let (reader, writer, closer, writer_closer) = match parts {
        Ok(parts) => parts,
        Err(error) => return Error::Connect(error),
    };
    socket.attach(&closer);
    let hello = ClientMessage::Hello {
        client: ClientKind::Gui,
        revision: REVISION,
        since,
        version: VERSION,
    };
    if let Err(error) = thread::Builder::new()
        .name("herdr-gui-chat-writer".into())
        .spawn(move || write(writer, &writer_closer, hello, &queued))
    {
        closer.close();
        return Error::Spawn(error);
    }
    let mut reader = BufReader::new(reader);
    let end = loop {
        match read_frame::<_, ServerMessage>(&mut reader) {
            Ok(Some(message)) => match events.try_send(message) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => break Error::Overflow,
                Err(TrySendError::Disconnected(_)) => break Error::NotAttached,
            },
            Ok(None) => break Error::Closed,
            Err(error) => break Error::Read(error),
        }
    };
    closer.close();
    closer.reap();
    end
}

/// The writer thread: the greeting, then each queued command in order,
/// until the link is dropped or a write fails.
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
