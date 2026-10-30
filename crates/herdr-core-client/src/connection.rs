//! One framed connection to a core. A worker thread owns the socket: it
//! writes queued frames in bounded batches, reads with a short timeout so a
//! partial frame waits for its rest, and routes what it decodes. Calls on
//! channel 0 run side by side, matched to their answers by ID.

use crate::{
    Endpoint, Error, Method, Result, Violation,
    frame::{Decoder, Frame, Kind},
    hello::{GENERATION, Hello},
    stream::{self, Attachment, End, Link, Route, Stream, Subscription},
    wire::{self, Doc, DocSet, List, Subscribed, Terminal},
};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// How long a read waits before the worker writes what is queued.
pub(crate) const POLL: Duration = Duration::from_millis(10);
/// How long a write may take before the connection counts as lost.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the core has to answer the handshake.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest handshake reply accepted.
const MAX_HELLO: usize = 64 << 10;
/// How long a call waits for its answer.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);
/// Calls waiting for answers at once.
const MAX_IN_FLIGHT: usize = 64;
/// Frames queued for the worker to write.
const MAX_OUTBOUND: usize = 1024;
/// Frames written before the worker reads again.
const WRITE_BATCH: usize = 64;

struct Reply {
    result: std::result::Result<Value, Refusal>,
    route: Option<(u32, Arc<Route>)>,
}

/// A call's result, and the stream channel it opened.
struct Answer {
    result: Value,
    route: Option<(u32, Arc<Route>)>,
}

impl Refusal {
    fn into_error(self, method: Method) -> Error {
        Error::Refused {
            method,
            kind: self.data.and_then(|data| data.kind),
            message: self.message,
        }
    }
}

#[derive(serde::Deserialize)]
struct Refusal {
    #[serde(default)]
    data: Option<RefusalData>,
    #[serde(default)]
    message: String,
}

#[derive(serde::Deserialize)]
struct RefusalData {
    #[serde(default)]
    kind: Option<String>,
}

#[derive(Default)]
struct Shared {
    lost: AtomicBool,
    pending: Mutex<HashMap<u64, Sender<Reply>>>,
    routes: Mutex<HashMap<u32, Arc<Route>>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// Fails every waiting call and ends every stream.
    fn lose(&self) {
        self.lost.store(true, Ordering::Release);
        locked(&self.pending).clear();
        for (_, route) in locked(&self.routes).drain() {
            route.end(End::Lost);
        }
    }
}

/// A connection to one core. Dropping it (and every stream from it) ends
/// the connection; the core and its terminals keep running.
pub struct Core {
    hello: Hello,
    link: Link,
    next_id: AtomicU64,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
}

impl Core {
    /// Connects to a running core. Blocking: never call it on the UI thread.
    /// A core that is not running is `Error::Connect`; this never starts one.
    pub fn connect(endpoint: &Endpoint) -> Result<Self> {
        let connect_error = |source| Error::Connect {
            path: endpoint.socket.clone(),
            source,
        };
        let stream = connect_stream(&endpoint.socket).map_err(connect_error)?;
        Self::over(stream)
    }

    /// Runs the protocol over `stream`, whose reads must time out after
    /// about `POLL` so the worker can write in between.
    pub(crate) fn over(mut stream: impl Read + Write + Send + 'static) -> Result<Self> {
        let hello = json!({"jsonrpc": "2.0", "id": 0, "method": Method::Hello.as_str(),
            "params": {"framed": true, "generation": GENERATION}});
        let mut line = serde_json::to_vec(&hello)?;
        line.push(b'\n');
        stream.write_all(&line)?;
        stream.flush()?;
        let (reply, rest) = read_hello(&mut stream)?;
        let hello = answer(Method::Hello, &reply)?;
        let hello: Hello =
            serde_json::from_value(hello).map_err(|error| Error::Invalid(Method::Hello, error))?;
        if !hello.framed {
            return Err(Violation::Unframed.into());
        }
        hello.check()?;

        let (outbound, queue) = crossbeam_channel::bounded(MAX_OUTBOUND);
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let mut decoder = Decoder::default();
        decoder.push(&rest);
        let worker = Worker {
            decoder,
            queue,
            shared: Arc::clone(&shared),
            stop: Arc::clone(&stop),
            stream,
        };
        std::thread::Builder::new()
            .name("herdr-core".into())
            .spawn(move || worker.run())?;
        Ok(Self {
            hello,
            link: Link { outbound },
            next_id: AtomicU64::new(1),
            shared,
            stop,
        })
    }

    pub fn hello(&self) -> &Hello {
        &self.hello
    }

    /// Whether the connection dropped; every later call is `Error::Lost`.
    pub fn is_lost(&self) -> bool {
        self.shared.lost.load(Ordering::Acquire)
    }

    pub fn list(&self, all: bool) -> Result<List> {
        self.typed(Method::TerminalList, &json!({"all": all}))
    }

    /// Every terminal now, and the events after it.
    pub fn subscribe(&self) -> Result<Subscription> {
        let reply = self.call(Method::EventsSubscribe, &json!({}))?;
        let subscribed: Subscribed = parse(Method::EventsSubscribe, reply.result)?;
        let stream = self.stream(Method::EventsSubscribe, reply.route)?;
        Ok(Subscription::new(
            stream,
            subscribed.seq,
            subscribed.terminals,
        ))
    }

    pub fn attach(&self, attach: &wire::Attach<'_>) -> Result<Attachment> {
        let reply = self.call(Method::TerminalAttach, attach)?;
        let attached: wire::Attached = parse(Method::TerminalAttach, reply.result)?;
        let stream = self.stream(Method::TerminalAttach, reply.route)?;
        Ok(Attachment::new(stream, attached.terminal))
    }

    pub fn run(&self, run: &wire::Run<'_>) -> Result<Terminal> {
        self.typed(Method::TerminalRun, run)
    }

    pub fn kill(&self, terminal: &str) -> Result<Terminal> {
        self.typed(Method::TerminalKill, &json!({"terminal": terminal}))
    }

    pub fn doc_get(&self, name: &str) -> Result<Doc> {
        self.typed(Method::DocGet, &json!({"name": name}))
    }

    /// Writes `value` over `version` (from `doc_get`) and returns the new
    /// version; a stale version is refused. `Value::Null` deletes it.
    pub fn doc_set(&self, name: &str, value: &Value, version: u64) -> Result<u64> {
        let set: DocSet = self.typed(
            Method::DocSet,
            &json!({"name": name, "value": value, "version": version}),
        )?;
        Ok(set.version)
    }

    fn typed<R: DeserializeOwned>(&self, method: Method, params: &impl Serialize) -> Result<R> {
        parse(method, self.call(method, params)?.result)
    }

    fn stream(&self, method: Method, route: Option<(u32, Arc<Route>)>) -> Result<Stream> {
        let (channel, route) = route.ok_or(Error::Invalid(
            method,
            serde::de::Error::missing_field("channel"),
        ))?;
        Ok(Stream {
            channel,
            link: self.link.clone(),
            route,
        })
    }

    fn call(&self, method: Method, params: &impl Serialize) -> Result<Answer> {
        if self.is_lost() {
            return Err(Error::Lost);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = crossbeam_channel::bounded(1);
        {
            let mut pending = locked(&self.shared.pending);
            if pending.len() >= MAX_IN_FLIGHT {
                return Err(Error::Busy);
            }
            pending.insert(id, reply);
        }
        let message =
            json!({"jsonrpc": "2.0", "id": id, "method": method.as_str(), "params": params});
        let sent = serde_json::to_vec(&message)
            .map_err(Error::from)
            .and_then(|payload| self.link.send(0, Kind::Data, &payload));
        if let Err(error) = sent {
            locked(&self.shared.pending).remove(&id);
            return Err(error);
        }
        match answer.recv_timeout(CALL_TIMEOUT) {
            Ok(reply) => Ok(reply),
            Err(RecvTimeoutError::Timeout) => {
                locked(&self.shared.pending).remove(&id);
                Err(Error::Timeout(method))
            }
            Err(RecvTimeoutError::Disconnected) => Err(Error::Lost),
        }
        .and_then(|reply| {
            let result = reply.result.map_err(|refusal| refusal.into_error(method))?;
            Ok(Answer {
                result,
                route: reply.route,
            })
        })
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        // Streams keep the worker's queue open; they end with the core's link.
        self.stop.store(true, Ordering::Release);
    }
}

fn parse<R: DeserializeOwned>(method: Method, value: Value) -> Result<R> {
    serde_json::from_value(value).map_err(|error| Error::Invalid(method, error))
}

/// A JSON-RPC answer's result, or the core's refusal.
fn answer(method: Method, message: &Value) -> Result<Value> {
    if let Some(error) = message.get("error") {
        let refusal: Refusal = parse(method, error.clone())?;
        return Err(refusal.into_error(method));
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

/// The plain-mode reply line, and any framed bytes read past it.
fn read_hello(stream: &mut impl Read) -> Result<(Value, Vec<u8>)> {
    let deadline = Instant::now() + HELLO_TIMEOUT;
    let mut buffer = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        if let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
            let rest = buffer.split_off(end + 1);
            return Ok((serde_json::from_slice(&buffer)?, rest));
        }
        if buffer.len() > MAX_HELLO {
            return Err(Violation::LongHello(MAX_HELLO).into());
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout(Method::Hello));
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(Error::Lost),
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(error) if waiting(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn waiting(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

struct Worker<S> {
    decoder: Decoder,
    queue: Receiver<Vec<u8>>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    stream: S,
}

impl<S: Read + Write> Worker<S> {
    fn run(mut self) {
        if let Err(error) = self.serve() {
            tracing::info!(%error, "herdr-core connection ended");
        }
        self.shared.lose();
    }

    fn serve(&mut self) -> Result<()> {
        let mut buffer = vec![0; 64 << 10];
        while !self.stop.load(Ordering::Acquire) {
            if !self.write_queued()? {
                return Ok(());
            }
            match self.stream.read(&mut buffer) {
                Ok(0) => return Err(Error::Lost),
                Ok(read) => self.decoder.push(&buffer[..read]),
                Err(error) if waiting(&error) => {}
                Err(error) => return Err(error.into()),
            }
            while let Some(frame) = self.decoder.next_frame()? {
                self.dispatch(frame)?;
            }
        }
        Ok(())
    }

    /// Writes a bounded batch; false once nobody can queue more.
    fn write_queued(&mut self) -> Result<bool> {
        let mut wrote = false;
        for _ in 0..WRITE_BATCH {
            match self.queue.try_recv() {
                Ok(frame) => {
                    self.stream.write_all(&frame)?;
                    wrote = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(false),
            }
        }
        if wrote {
            self.stream.flush()?;
        }
        Ok(true)
    }

    fn dispatch(&self, frame: Frame) -> Result<()> {
        match (frame.channel, frame.kind) {
            (0, Kind::Data) => self.answer(&frame.payload),
            (0, _) => Ok(()),
            (channel, Kind::Data) => {
                let route = locked(&self.shared.routes).get(&channel).cloned();
                // Bytes for a stream already detached are dropped.
                route.map_or(Ok(()), |route| {
                    route.push(&frame.payload).map_err(Error::from)
                })
            }
            (channel, Kind::Close) => {
                if let Some(route) = locked(&self.shared.routes).remove(&channel) {
                    route.end(stream::closed(&frame.payload));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn answer(&self, payload: &[u8]) -> Result<()> {
        let message: Value = serde_json::from_slice(payload)?;
        // Notifications arrive on subscription channels in framed mode.
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            return Ok(());
        };
        let Some(waiter) = locked(&self.shared.pending).remove(&id) else {
            return Ok(());
        };
        let result = match message.get("error") {
            Some(error) => Err(serde_json::from_value(error.clone()).unwrap_or(Refusal {
                data: None,
                message: error.to_string(),
            })),
            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
        };
        // The route exists before the caller hears back, so stream bytes
        // that follow the answer are never dropped.
        let route = result
            .as_ref()
            .ok()
            .and_then(|result| result.get("channel")?.as_u64())
            .and_then(|channel| u32::try_from(channel).ok())
            .map(|channel| {
                let route = Arc::new(Route::new());
                locked(&self.shared.routes).insert(channel, Arc::clone(&route));
                (channel, route)
            });
        if let Err(unheard) = waiter.try_send(Reply { result, route }) {
            // The caller gave up; its stream has no reader.
            if let Some((channel, _)) = unheard.into_inner().route {
                locked(&self.shared.routes).remove(&channel);
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn connect_stream(path: &std::path::Path) -> io::Result<herdr_client::Stream> {
    let stream = herdr_client::Stream::connect(path)?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    Ok(stream)
}

/// The core's pipe name is open to every local account, so the pipe must be
/// one this user owns before anything is sent.
#[cfg(windows)]
fn connect_stream(path: &std::path::Path) -> io::Result<herdr_client::Stream> {
    let mut stream = herdr_client::Stream::connect_owned(path)?;
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    Ok(stream)
}
