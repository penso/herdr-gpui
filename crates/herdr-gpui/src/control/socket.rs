//! The channel behind `herdr-gpui <command>`: a Unix socket, or a named
//! pipe on Windows. A worker accepts connections and serves each on a
//! thread of its own, up to a bound: it reads one bounded request, hands it
//! to the UI through a bounded queue, and writes back the answer the UI
//! gives it. A feedback request may wait minutes for the user, so
//! connections cannot share one thread.
#[cfg(windows)]
mod pipe;
#[cfg(unix)]
mod unix;

use super::protocol::{ErrorCode, FeedbackRequest, MAX_MESSAGE, Request, Response};
#[cfg(windows)]
use pipe as transport;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};
use transport::Connection;
#[cfg(unix)]
use unix as transport;

const IO_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a caller waits for the UI, which may be busy or have no window.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);
/// Requests waiting for the UI. Each connection holds one, so this bounds
/// how many callers are answered `busy` rather than queued.
const QUEUE: usize = 4;
/// Connections served at once, waiting ones included.
const CONNECTIONS: usize = 8;

/// Where this user's app listens, given its state directory.
pub(super) fn address(state: &Path) -> crate::Result<PathBuf> {
    transport::address(state)
}

/// How long the UI may take to answer `request`: one that waits for the
/// user, a page, or an event on purpose gets as long as it asked for.
fn answer_timeout(request: &Request) -> Duration {
    ANSWER_TIMEOUT + Duration::from_secs(request.wait_seconds())
}

/// A request waiting for the UI's answer.
pub(crate) struct Incoming {
    pub request: Request,
    reply: Arc<Mutex<Reply>>,
}

struct Reply {
    live: bool,
    sender: SyncSender<Response>,
}

pub(crate) enum Event {
    Request(Incoming),
    Undelivered {
        request: FeedbackRequest,
        text: String,
    },
}

impl Incoming {
    pub(crate) fn respond(self, response: Response) {
        let _ = self.try_respond(response);
    }

    pub(crate) fn is_live(&self) -> bool {
        self.reply
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .live
    }

    /// Return ownership if the worker stopped waiting, so notes can be kept.
    pub(crate) fn try_respond(self, response: Response) -> Result<(), Response> {
        let reply = self
            .reply
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !reply.live {
            return Err(response);
        }
        reply
            .sender
            .try_send(response)
            .map_err(|error| match error {
                TrySendError::Full(response) | TrySendError::Disconnected(response) => response,
            })
    }
}

pub(crate) struct Server {
    path: PathBuf,
    requests: Receiver<Event>,
}

impl Server {
    /// Listens at `path`. Another live server keeps it and this one does
    /// not start.
    pub(crate) fn bind(path: &Path) -> crate::Result<Self> {
        let listener = transport::Listener::bind(path)?;
        let (sender, requests) = mpsc::sync_channel(QUEUE);
        // The worker blocks in accept for the life of the process. Nothing
        // joins it: the UI must never wait on it, and exit reclaims it.
        thread::Builder::new()
            .name("control-socket".into())
            .spawn(move || {
                let open = Arc::new(AtomicUsize::new(0));
                loop {
                    let mut stream = match listener.accept() {
                        Ok(stream) => stream,
                        Err(error) => {
                            tracing::debug!(%error, "Control accept failed");
                            // A failure that repeats, such as a pipe instance
                            // that cannot be made, must not spin this thread.
                            thread::sleep(Duration::from_millis(100));
                            continue;
                        }
                    };
                    if let Err(error) = stream.prepare(IO_TIMEOUT) {
                        tracing::debug!(%error, "Control connection setup failed");
                        continue;
                    }
                    if open.fetch_add(1, Ordering::AcqRel) >= CONNECTIONS {
                        open.fetch_sub(1, Ordering::AcqRel);
                        refuse(&stream);
                        continue;
                    }
                    let (queue, done) = (sender.clone(), open.clone());
                    let spawned = thread::Builder::new()
                        .name("control-connection".into())
                        .spawn(move || {
                            serve(&stream, &queue);
                            done.fetch_sub(1, Ordering::AcqRel);
                        });
                    if let Err(error) = spawned {
                        tracing::debug!(%error, "Control connection thread failed");
                        open.fetch_sub(1, Ordering::AcqRel);
                    }
                }
            })?;
        Ok(Self {
            path: path.to_owned(),
            requests,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Requests that arrived since the last call.
    pub(crate) fn drain(&self) -> impl Iterator<Item = Event> + '_ {
        self.requests.try_iter()
    }

    #[cfg(test)]
    pub(super) fn next(&self) -> Option<Incoming> {
        match self.requests.recv_timeout(Duration::from_secs(5)).ok()? {
            Event::Request(incoming) => Some(incoming),
            Event::Undelivered { .. } => None,
        }
    }
}

/// Cleans up after the server at `path` when the app quits.
pub(crate) fn release(path: &Path) {
    transport::release(path);
}

/// Reads one line of at most `MAX_MESSAGE` bytes, without its newline.
fn read_line(reader: impl Read) -> crate::Result<Vec<u8>> {
    let mut line = Vec::new();
    BufReader::new(reader.take(MAX_MESSAGE as u64 + 1)).read_until(b'\n', &mut line)?;
    if line.last() == Some(&b'\n') {
        line.pop();
    } else if line.len() > MAX_MESSAGE {
        return Err(crate::control::Error::MessageSize { limit: MAX_MESSAGE }.into());
    }
    Ok(line)
}

fn write_line(mut writer: impl Write, message: &impl serde::Serialize) -> crate::Result<()> {
    let mut bytes = serde_json::to_vec(message)?;
    bytes.push(b'\n');
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}

fn answer(request: Request, queue: &SyncSender<Event>, stream: &Connection) -> Response {
    let timeout = answer_timeout(&request);
    let (sender, answer) = mpsc::sync_channel(1);
    let reply = Arc::new(Mutex::new(Reply { live: true, sender }));
    match queue.try_send(Event::Request(Incoming {
        request,
        reply: reply.clone(),
    })) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            return Response::error(ErrorCode::Busy, "Herdr UI is handling other requests");
        }
        Err(TrySendError::Disconnected(_)) => {
            return Response::error(ErrorCode::NoWindow, "Herdr UI is shutting down");
        }
    }
    let until = Instant::now() + timeout;
    loop {
        if let Ok(response) = answer.recv_timeout(Duration::from_millis(50)) {
            return response;
        }
        if Instant::now() >= until || stream.peer_closed() {
            // No I/O under this lock. Serialize cancellation with try_respond,
            // retaining a response that raced the disconnect instead of dropping it.
            let mut reply = reply
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            reply.live = false;
            return answer.try_recv().unwrap_or_else(|_| {
                Response::error(ErrorCode::Timeout, "Herdr UI did not answer in time")
            });
        }
    }
}

/// Answers a connection over the bound without reading it.
fn refuse(stream: &Connection) {
    let busy = Response::error(ErrorCode::Busy, "Herdr UI is handling other requests");
    if let Err(error) = write_line(stream, &busy) {
        tracing::debug!(%error, "Control refusal failed");
    }
}

/// Serves one request on a connection whose timeouts are already set.
fn serve(stream: &Connection, queue: &SyncSender<Event>) {
    let request = read_line(stream).and_then(|line| Ok(serde_json::from_slice::<Request>(&line)?));
    let (request, response) = match request {
        Ok(request) => {
            if let Err(error) = stream.watch(true) {
                tracing::debug!(%error, "Control disconnect monitoring failed");
                return;
            }
            let response = answer(request.clone(), queue, stream);
            (Some(request), response)
        }
        Err(error) => (
            None,
            Response::error(ErrorCode::InvalidRequest, error.to_string()),
        ),
    };
    finish_response(stream, queue, request, response);
}

/// Whether an answer fits the bound the caller reads with, newline included.
fn fits(response: &Response) -> bool {
    serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() < MAX_MESSAGE)
}

fn finish_response(
    stream: &Connection,
    queue: &SyncSender<Event>,
    request: Option<Request>,
    response: Response,
) {
    // A feature that would send more is refused here rather than cut off
    // mid-line. Notes that cannot go out are kept again, as on any failed write.
    let response = if fits(&response) {
        response
    } else {
        if let (Some(Request::Feedback(request)), Response::Feedback { text: Some(text) }) =
            (&request, response)
        {
            let _ = queue.send(Event::Undelivered {
                request: request.clone(),
                text,
            });
        }
        tracing::warn!("Control answer over {MAX_MESSAGE} bytes refused");
        Response::error(
            ErrorCode::Failed,
            format!("Herdr UI's answer is larger than {MAX_MESSAGE} bytes"),
        )
    };
    let written = stream
        .watch(false)
        .map_err(crate::Error::from)
        .and_then(|()| write_line(stream, &response));
    if let Err(error) = written {
        tracing::debug!(%error, "Control response failed");
        if let (Some(Request::Feedback(request)), Response::Feedback { text: Some(text) }) =
            (request, response)
        {
            // Bounded backpressure is safe here, off the UI thread. The queue
            // and connection limits also bound notes awaiting their return.
            let _ = queue.send(Event::Undelivered { request, text });
        }
    }
}

/// Sends one request to the server at `path` and returns its answer.
pub(crate) fn call(path: &Path, request: &Request) -> crate::Result<Response> {
    // The server waits this long for the UI before answering.
    let stream = transport::connect(path, IO_TIMEOUT, answer_timeout(request) + IO_TIMEOUT)?;
    write_line(&stream, request)?;
    let line = read_line(&stream)?;
    if line.is_empty() {
        return Err(crate::control::Error::NoResponse.into());
    }
    Ok(serde_json::from_slice(&line)?)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) use tests::address as test_address;
