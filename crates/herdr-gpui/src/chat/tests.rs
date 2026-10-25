use super::*;
use herdr_chat_protocol::{
    Change, ClientMessage, Cursor, Entry, EntryId, EntryKind, ServerMessage, Status, Transcript,
    VERSION, read_frame, write_frame,
};
use herdr_client::private_endpoint::{Closer, Connection, Listener, Reader, Writer};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

mod attachments;
mod chats;
mod composer;
mod controls;
mod discovery;
mod embeds;
mod feature;
mod glide;
mod imessage;
mod layouts;
mod link;
mod list;
mod menus;
mod overlay;
mod prepared;
mod session;
mod start;
mod timeline;
mod visual;

/// How long a test waits on a worker or a socket before failing.
const WAIT: Duration = Duration::from_secs(5);

fn epoch(n: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(n)
}

fn cursor(n: u128, seq: u64) -> Cursor {
    Cursor {
        epoch: epoch(n),
        seq,
    }
}

fn welcome() -> ServerMessage {
    ServerMessage::Welcome {
        agent: "Claude".into(),
        pane: Some("p1".into()),
        version: VERSION,
    }
}

fn agent(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Agent { text: text.into() },
    }
}

fn snapshot(at: Cursor, entries: Vec<Entry>) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = entries;
    transcript.status = Status::Idle;
    ServerMessage::Snapshot {
        cursor: at,
        transcript: Box::new(transcript),
    }
}

fn change(change: Change, at: Cursor) -> ServerMessage {
    ServerMessage::Change { change, cursor: at }
}

fn append(id: u64, text: &str) -> Change {
    Change::Append {
        id: EntryId(id),
        text: text.into(),
    }
}

/// Polls until `done` holds, at most [`WAIT`].
fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// An address nothing listens at, unique to the test.
fn address(name: &str) -> (Option<tempfile::TempDir>, PathBuf) {
    #[cfg(unix)]
    {
        // macOS's default temporary directory can exceed sockaddr_un's bound.
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let path = dir.path().join(format!("{name}.sock"));
        (Some(dir), path)
    }
    #[cfg(windows)]
    {
        let unique = uuid::Uuid::new_v4().simple();
        (
            None,
            PathBuf::from(format!(r"\\.\pipe\herdr-gui-test-{name}-{unique}")),
        )
    }
}

/// A stand-in for `herdr-acp`: a private endpoint whose connections a thread
/// accepts, since a pipe listener cannot be polled.
struct Program {
    _dir: Option<tempfile::TempDir>,
    accepted: mpsc::Receiver<Connection>,
    /// The accepting thread, until [`Program::stop`].
    listening: Option<std::thread::JoinHandle<()>>,
    path: PathBuf,
    stopping: Arc<AtomicBool>,
}

impl Program {
    fn new() -> Self {
        let (dir, path) = address("chat");
        let listener = Listener::bind(&path).unwrap();
        let (sender, accepted) = mpsc::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopping);
        // Blocks in `accept` until the test process ends or `stop` wakes it.
        let listening = std::thread::spawn(move || {
            while let Ok(connection) = listener.accept() {
                if stop.load(Ordering::Acquire) || sender.send(connection).is_err() {
                    return;
                }
            }
        });
        Self {
            _dir: dir,
            accepted,
            listening: Some(listening),
            path,
            stopping,
        }
    }

    /// Stops listening, as a program that went away does: on Windows its
    /// pipe is gone, on Unix nothing answers at its socket.
    fn stop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        // Wakes the blocked `accept`.
        let _ = Connection::connect(&self.path, WAIT);
        if let Some(listening) = self.listening.take() {
            listening.join().unwrap();
        }
    }

    /// Accepts the next client and reads its greeting.
    fn accept(&self) -> (Peer, ClientMessage) {
        let connection = self
            .accepted
            .recv_timeout(WAIT)
            .expect("no client connected");
        let (mut reader, writer, closer) = connection.split().unwrap();
        reader.set_timeout(Some(WAIT)).unwrap();
        let mut peer = Peer {
            _closer: closer,
            reader,
            writer,
        };
        let hello = read(&mut peer).expect("a greeting");
        (peer, hello)
    }
}

/// The program's end of one connection.
struct Peer {
    _closer: Closer,
    reader: Reader,
    writer: Writer,
}

impl Read for Peer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

impl Write for Peer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn read(stream: &mut Peer) -> Option<ClientMessage> {
    read_frame(stream).unwrap()
}

fn send(stream: &mut Peer, message: &ServerMessage) {
    write_frame(stream, message).unwrap();
}

fn hello(since: Option<Cursor>) -> ClientMessage {
    ClientMessage::Hello {
        client: herdr_chat_protocol::ClientKind::Gui,
        since,
        version: VERSION,
    }
}
