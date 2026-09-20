//! A Unix local/SSH gen1 client. All transport I/O runs on a dedicated worker.
//! No reconnect/replay: commands carry the boot ID of the snapshot they act on.
//! Drain `Client::events` on a GUI background task, never block the UI thread.
#![doc = include_str!("../README.md")]
mod catalog;
mod discovery;
mod ssh;
pub use catalog::{
    SavedHost, load_saved_host_selection, load_saved_hosts, store_saved_host_selection,
};
pub mod presentation;
pub use crossbeam_channel::Receiver;
use crossbeam_channel::{SendTimeoutError, Sender, TrySendError, bounded};
pub use discovery::{ConnectTarget, session_socket};
pub use herdr_protocol as protocol;
use protocol::{endpoint::*, *};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const COMMAND_CAPACITY: usize = 64;
const EVENT_CAPACITY: usize = 8;
const POLL: Duration = Duration::from_millis(10);
const TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectOptions {
    pub surface_size: ClientSurfaceSize,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
}
impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            surface_size: ClientSurfaceSize { cols: 80, rows: 24 },
            cell_width_px: 0,
            cell_height_px: 0,
        }
    }
}

#[derive(Debug)]
pub enum ClientEvent {
    Connected(EndpointServerWelcome),
    Snapshot(Arc<ClientShellSnapshot>),
    /// Complete text baseline, including after a cell patch. Graphics assets are
    /// still connection-relative wire data; this client does not render images.
    Surface(Arc<PaneSurfaceFrame>),
    Response {
        request_id: String,
        response: Value,
    },
    /// Queued command was not sent (stale boot, unsupported method, or busy).
    CommandRejected {
        request_id: Option<String>,
        reason: String,
    },
    /// Notifications, clipboard, title, bell, and other non-surface wire events.
    Message(ServerMessage),
    Disconnected {
        reason: String,
    },
}

pub struct Client {
    pub handle: ClientHandle,
    pub events: Receiver<ClientEvent>,
}
#[derive(Clone)]
pub struct ClientHandle {
    inner: Arc<HandleInner>,
}
struct HandleInner {
    commands: Sender<Command>,
    stop: Arc<AtomicBool>,
    next_request: AtomicU64,
}
impl Drop for HandleInner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
struct Command {
    boot_id: String,
    bytes: Vec<u8>,
    request: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    Full,
    Disconnected,
    Invalid(String),
}
impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => f.write_str("client command queue is full"),
            Self::Disconnected => f.write_str("client is disconnected"),
            Self::Invalid(reason) => write!(f, "invalid client command: {reason}"),
        }
    }
}
impl std::error::Error for SendError {}

/// Returns immediately after spawning. Connection/handshake errors arrive as events.
pub fn connect(target: ConnectTarget, options: ConnectOptions) -> io::Result<Client> {
    connect_with_surface_active(target, options, true)
}

/// Connect without changing `ConnectOptions` literals. Inactive connections require
/// negotiated surface interest and presentation-effect fencing. SSH additionally
/// requires endpoint health checks. All transport work runs off the caller thread.
pub fn connect_with_surface_active(
    target: ConnectTarget,
    options: ConnectOptions,
    surface_active: bool,
) -> io::Result<Client> {
    connect_with_connector(target, options, surface_active, |target, _| {
        target.socket_path().and_then(UnixStream::connect)
    })
}

/// Connect using application-specific local socket setup on the I/O worker.
/// SSH targets always use the remote bridge, never the local connector.
/// The connector should observe `stop` during waits so detach cancels setup.
pub fn connect_with_connector(
    target: ConnectTarget,
    options: ConnectOptions,
    surface_active: bool,
    connector: impl FnOnce(&ConnectTarget, &AtomicBool) -> io::Result<UnixStream> + Send + 'static,
) -> io::Result<Client> {
    validate_options(options)?;
    if let ConnectTarget::Ssh { target, session } = &target {
        catalog::validate_target(target)?;
        session_socket(std::path::Path::new(""), session)?;
    }
    let (commands, rx) = bounded(COMMAND_CAPACITY);
    let (tx, events) = bounded(EVENT_CAPACITY);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    thread::Builder::new()
        .name("herdr-client-io".into())
        .spawn(move || {
            let result = (|| {
                let (stream, child) = match &target {
                    ConnectTarget::Ssh { target, session } => {
                        let (stream, child) = ssh::connect(target, session, &worker_stop)?;
                        (stream, Some(child))
                    }
                    _ => (connector(&target, &worker_stop)?, None),
                };
                run_connection(
                    stream,
                    options,
                    surface_active,
                    child.is_some(),
                    rx,
                    &tx,
                    &worker_stop,
                )
                // The child guard is dropped before delivering a disconnect event.
            })();
            if !worker_stop.load(Ordering::Acquire) {
                let reason = result
                    .err()
                    .map(|e| {
                        e.to_string()
                            .chars()
                            .filter(|c| !c.is_control())
                            .take(1024)
                            .collect()
                    })
                    .unwrap_or_else(|| "server disconnected".into());
                let _ = deliver(&tx, ClientEvent::Disconnected { reason }, &worker_stop);
            }
            worker_stop.store(true, Ordering::Release);
        })?;
    Ok(Client {
        handle: ClientHandle {
            inner: Arc::new(HandleInner {
                commands,
                stop,
                next_request: AtomicU64::new(1),
            }),
        },
        events,
    })
}

impl ClientHandle {
    pub fn disconnect(&self) {
        self.inner.stop.store(true, Ordering::Release);
    }
    pub fn is_disconnected(&self) -> bool {
        self.inner.stop.load(Ordering::Acquire)
    }

    fn enqueue(
        &self,
        boot_id: &str,
        message: ClientMessage,
        request: Option<(String, String)>,
    ) -> Result<(), SendError> {
        if self.is_disconnected() {
            return Err(SendError::Disconnected);
        }
        if boot_id.is_empty() {
            return Err(SendError::Invalid("snapshot boot ID required".into()));
        }
        let bytes = encode_message(&message, MAX_FRAME_SIZE)
            .map_err(|e| SendError::Invalid(e.to_string()))?;
        self.inner
            .commands
            .try_send(Command {
                boot_id: boot_id.into(),
                bytes,
                request,
            })
            .map_err(|e| match e {
                TrySendError::Full(_) => SendError::Full,
                TrySendError::Disconnected(_) => SendError::Disconnected,
            })
    }
    pub fn send_input(
        &self,
        boot_id: &str,
        pane_id: &str,
        events: impl IntoIterator<Item = ClientPaneInputEvent>,
    ) -> Result<(), SendError> {
        self.enqueue(
            boot_id,
            ClientMessage::ClientShellPaneInput {
                pane_id: pane_id.into(),
                events: events.into_iter().collect(),
            },
            None,
        )
    }
    pub fn send_popup_input(
        &self,
        boot_id: &str,
        terminal_id: &str,
        events: impl IntoIterator<Item = ClientPaneInputEvent>,
    ) -> Result<(), SendError> {
        self.enqueue(
            boot_id,
            ClientMessage::ClientShellPopupInput {
                terminal_id: terminal_id.into(),
                events: events.into_iter().collect(),
            },
            None,
        )
    }
    pub fn resize(&self, boot_id: &str, options: ConnectOptions) -> Result<(), SendError> {
        validate_options(options).map_err(|e| SendError::Invalid(e.to_string()))?;
        self.enqueue(
            boot_id,
            ClientMessage::ClientShellResize {
                cell_width_px: options.cell_width_px,
                cell_height_px: options.cell_height_px,
                surface_size: options.surface_size,
                pixel_mouse: false,
            },
            None,
        )
    }
    pub fn set_focus(&self, boot_id: &str, focused: bool) -> Result<(), SendError> {
        self.enqueue(boot_id, ClientMessage::ClientShellFocus { focused }, None)
    }
    /// Queue upstream's surface-interest API, not window focus. Wait for the
    /// matching Response before considering a host activation/deactivation complete.
    pub fn set_surface_active(&self, boot_id: &str, active: bool) -> Result<String, SendError> {
        self.request(
            boot_id,
            "client_shell.surface.set",
            json!({"active": active}),
        )
    }
    /// Serialize the API envelope, generate an ID, and queue on the ordered writer.
    /// Only methods advertised in Connected are sent. Responses retain API errors.
    pub fn request(&self, boot_id: &str, method: &str, params: Value) -> Result<String, SendError> {
        let id = format!(
            "gpui-{}",
            self.inner.next_request.fetch_add(1, Ordering::Relaxed)
        );
        let request = json!({"id": id, "method": method, "params": params}).to_string();
        self.enqueue(
            boot_id,
            ClientMessage::ClientShellEndpointRequest {
                boot_id: boot_id.into(),
                request,
            },
            Some((id.clone(), method.into())),
        )?;
        Ok(id)
    }
    pub fn focus_pane(&self, boot_id: &str, pane_id: &str) -> Result<String, SendError> {
        self.request(boot_id, "pane.focus", json!({"pane_id": pane_id}))
    }
    pub fn focus_tab(&self, boot_id: &str, tab_id: &str) -> Result<String, SendError> {
        self.request(boot_id, "tab.focus", json!({"tab_id": tab_id}))
    }
    pub fn focus_workspace(&self, boot_id: &str, workspace_id: &str) -> Result<String, SendError> {
        self.request(
            boot_id,
            "workspace.focus",
            json!({"workspace_id": workspace_id}),
        )
    }
}

fn invalid(reason: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason.to_string())
}
fn validate_options(options: ConnectOptions) -> io::Result<()> {
    let size = options.surface_size;
    if size.cols == 0 || size.rows == 0 {
        return Err(invalid("surface dimensions must be nonzero"));
    }
    if size.cols > 4096
        || size.rows > 4096
        || u32::from(size.cols) * u32::from(size.rows) > 1_000_000
        || options.cell_width_px > 4096
        || options.cell_height_px > 4096
    {
        return Err(invalid("surface geometry exceeds endpoint limits"));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
fn deliver(tx: &Sender<ClientEvent>, mut event: ClientEvent, stop: &AtomicBool) -> io::Result<()> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "client stopped"));
        }
        match tx.send_timeout(event, POLL) {
            Ok(()) => return Ok(()),
            Err(SendTimeoutError::Timeout(e)) => event = e,
            Err(SendTimeoutError::Disconnected(_)) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "event receiver dropped",
                ));
            }
        }
    }
}

/// Keep partially read prefixes/payloads across socket timeouts. Restarting
/// read_exact after a timeout would silently corrupt framing.
struct FrameReader {
    bytes: Vec<u8>,
    target: usize,
    started: Option<Instant>,
}
impl FrameReader {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            target: 4,
            started: None,
        }
    }
    fn poll(&mut self, stream: &mut (impl Read + ?Sized)) -> io::Result<Option<ServerMessage>> {
        if self.started.is_some_and(|t| t.elapsed() > TIMEOUT) {
            return Err(invalid("partial frame timed out"));
        }
        let mut buf = [0; 8192];
        let len = buf.len().min(self.target - self.bytes.len());
        match stream.read(&mut buf[..len]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "socket closed",
                ));
            }
            Ok(n) => {
                self.started.get_or_insert_with(Instant::now);
                self.bytes.extend_from_slice(&buf[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e),
        }
        if self.bytes.len() != self.target {
            return Ok(None);
        }
        if self.target == 4 {
            let prefix = self
                .bytes
                .as_slice()
                .try_into()
                .map_err(|_| invalid("invalid frame prefix"))?;
            let len = u32::from_le_bytes(prefix) as usize;
            if len == 0 || len > MAX_GRAPHICS_FRAME_SIZE {
                return Err(invalid("invalid frame length"));
            }
            self.target += len;
            return Ok(None);
        }
        let message = decode_payload(&self.bytes[4..])?;
        self.bytes.clear();
        self.target = 4;
        self.started = None;
        Ok(Some(message))
    }
}

struct Pending {
    id: String,
    bytes: Vec<u8>,
    started: Instant,
}
fn supports_surface_interest(welcome: &EndpointServerWelcome) -> bool {
    ["surface_interest", "presentation_effects_fence"]
        .iter()
        .all(|cap| welcome.capabilities.iter().any(|c| c == cap))
        && welcome
            .methods
            .iter()
            .any(|m| m == "client_shell.surface.set")
}

struct Health {
    received: Instant,
    ping: Option<Instant>,
}
impl Health {
    fn received(&mut self, now: Instant) {
        self.received = now;
        self.ping = None;
    }
    fn tick(&mut self, now: Instant) -> io::Result<bool> {
        if self
            .ping
            .is_some_and(|sent| now.saturating_duration_since(sent) >= Duration::from_secs(10))
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "endpoint health check timed out",
            ));
        }
        if self.ping.is_none()
            && now.saturating_duration_since(self.received) >= Duration::from_secs(5)
        {
            self.ping = Some(now);
            return Ok(true);
        }
        Ok(false)
    }
}

struct Session {
    started: Instant,
    surface_active: bool,
    remote: bool,
    health: Option<Health>,
    welcome: Option<EndpointServerWelcome>,
    snapshot: Option<Arc<ClientShellSnapshot>>,
    surface: Option<Arc<PaneSurfaceFrame>>,
    pending: Option<Pending>,
}

impl Session {
    fn new(surface_active: bool, remote: bool) -> Self {
        Self {
            started: Instant::now(),
            surface_active,
            remote,
            health: None,
            welcome: None,
            snapshot: None,
            surface: None,
            pending: None,
        }
    }

    fn check_timeouts(&self) -> io::Result<()> {
        if self.snapshot.is_none() && self.started.elapsed() > TIMEOUT {
            return Err(invalid("handshake/snapshot timed out"));
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.started.elapsed() > COMMAND_TIMEOUT)
        {
            return Err(invalid("endpoint request timed out; not replayed"));
        }
        Ok(())
    }

    fn rejection_reason(&self, command: &Command) -> Option<&'static str> {
        if self
            .snapshot
            .as_ref()
            .is_none_or(|s| s.boot_id != command.boot_id)
        {
            Some("command does not match a ready snapshot boot")
        } else if let Some((_, method)) = &command.request
            && self
                .welcome
                .as_ref()
                .is_none_or(|w| !w.methods.contains(method))
        {
            Some("method not advertised by endpoint")
        } else if let Some((_, method)) = &command.request
            && method == "client_shell.surface.set"
            && self
                .welcome
                .as_ref()
                .is_none_or(|w| !supports_surface_interest(w))
        {
            Some("surface interest capabilities not advertised by endpoint")
        } else {
            None
        }
    }
}

fn run_connection(
    mut stream: UnixStream,
    options: ConnectOptions,
    surface_active: bool,
    remote: bool,
    commands: Receiver<Command>,
    tx: &Sender<ClientEvent>,
    stop: &AtomicBool,
) -> io::Result<()> {
    stream.set_read_timeout(Some(POLL))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let hello = EndpointClientHello {
        generation: ENDPOINT_PROTOCOL_GENERATION,
        cell_width_px: options.cell_width_px,
        cell_height_px: options.cell_height_px,
        surface_size: options.surface_size,
        pixel_mouse: false,
        direct_graphics: false,
        endpoint_keybindings: false,
        mouse_capture: false,
        surface_active,
        surface_reuse: false,
        surface_delta: false,
        snapshot_codecs: vec![SNAPSHOT_CODEC_V1.into()],
        surface_codecs: vec![SURFACE_CODEC_V1.into()],
        input_codecs: vec![INPUT_CODEC_V1.into()],
        blob_codecs: vec![BLOB_CODEC_V1.into()],
    };
    write_message(
        &mut stream,
        &ClientMessage::EndpointControl {
            kind: ENDPOINT_HELLO_KIND.into(),
            data: serde_json::to_string(&hello)?,
        },
        MAX_FRAME_SIZE,
    )?;
    let mut reader = FrameReader::new();
    let mut session = Session::new(surface_active, remote);
    let mut queued: Option<Command> = None;
    while !stop.load(Ordering::Acquire) {
        session.check_timeouts()?;
        if let Some(health) = &mut session.health
            && health.tick(Instant::now())?
        {
            write_message(
                &mut stream,
                &ClientMessage::EndpointControl {
                    kind: "endpoint.health.ping.v1".into(),
                    data: String::new(),
                },
                MAX_FRAME_SIZE,
            )?;
        }
        // Bound the batch so continuous input cannot starve reads.
        for _ in 0..16 {
            // Finish an inbound frame before dispatching against its old snapshot.
            if reader.started.is_some() {
                break;
            }
            let Some(command) = queued.take().or_else(|| commands.try_recv().ok()) else {
                break;
            };
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            if let Some(reason) = session.rejection_reason(&command) {
                deliver(
                    tx,
                    ClientEvent::CommandRejected {
                        request_id: command.request.map(|r| r.0),
                        reason: reason.into(),
                    },
                    stop,
                )?;
                continue;
            }
            // The daemon has one API command lease per connection. Keep FIFO order,
            // including input behind a waiting request, without draining the bound.
            if command.request.is_some() && session.pending.is_some() {
                queued = Some(command);
                break;
            }
            stream.write_all(&command.bytes)?;
            if let Some((id, _)) = command.request {
                session.pending = Some(Pending {
                    id,
                    bytes: Vec::new(),
                    started: Instant::now(),
                });
            }
        }
        let Some(message) = reader.poll(&mut stream)? else {
            continue;
        };
        session.handle_message(message, |event| deliver(tx, event, stop))?;
    }
    // No queued commands are flushed on cancellation and nothing is replayed.
    Ok(())
}

impl Session {
    fn handle_message(
        &mut self,
        message: ServerMessage,
        mut emit: impl FnMut(ClientEvent) -> io::Result<()>,
    ) -> io::Result<()> {
        if let Some(health) = &mut self.health {
            health.received(Instant::now());
        }
        let Self {
            surface_active,
            remote,
            health,
            welcome,
            snapshot,
            surface,
            pending,
            ..
        } = self;
        if welcome.is_none() {
            let ServerMessage::EndpointControl { kind, data } = message else {
                return Err(invalid("expected stable endpoint welcome"));
            };
            if kind != ENDPOINT_WELCOME_KIND {
                return Err(invalid("expected endpoint.welcome.v1"));
            }
            let w: EndpointServerWelcome = serde_json::from_str(&data)?;
            if let Some(error) = &w.error {
                return Err(invalid(format!("{}: {}", error.code, error.message)));
            }
            if w.generation != ENDPOINT_PROTOCOL_GENERATION
                || w.snapshot_codec != SNAPSHOT_CODEC_V1
                || w.surface_codec != SURFACE_CODEC_V1
                || w.input_codec != INPUT_CODEC_V1
                || w.blob_codec != BLOB_CODEC_V1
            {
                return Err(invalid("incompatible endpoint generation/codecs"));
            }
            if (*remote || !*surface_active) && !supports_surface_interest(&w) {
                return Err(invalid("endpoint lacks safe surface interest support"));
            }
            if *remote {
                if !w.capabilities.iter().any(|c| c == "health_check") {
                    return Err(invalid("SSH endpoint lacks health_check capability"));
                }
                *health = Some(Health {
                    received: Instant::now(),
                    ping: None,
                });
            }
            emit(ClientEvent::Connected(w.clone()))?;
            *welcome = Some(w);
            return Ok(());
        }
        match message {
            ServerMessage::EndpointControl { kind, data } if kind == ENDPOINT_SNAPSHOT_KIND => {
                let next: ClientShellSnapshot = serde_json::from_str(&data)?;
                if next.boot_id.is_empty()
                    || snapshot
                        .as_ref()
                        .is_some_and(|s| s.boot_id != next.boot_id || next.revision < s.revision)
                {
                    return Err(invalid(
                        "endpoint boot changed or snapshot revision regressed; reconnect required",
                    ));
                }
                let next = Arc::new(next);
                let revision_changed = snapshot
                    .as_ref()
                    .is_none_or(|s| s.revision != next.revision);
                *snapshot = Some(next.clone());
                emit(ClientEvent::Snapshot(next.clone()))?;
                if revision_changed
                    && let Some(current) = surface
                        .as_ref()
                        .filter(|current| current.projection_revision == next.revision)
                {
                    emit(ClientEvent::Surface(current.clone()))?;
                }
            }
            ServerMessage::EndpointControl { .. } => {} // Unknown optional named controls are ignored.
            ServerMessage::PaneSurface(next) => {
                let s = snapshot
                    .as_ref()
                    .ok_or_else(|| invalid("surface before snapshot"))?;
                if next.boot_id != s.boot_id
                    || surface
                        .as_ref()
                        .is_some_and(|old| next.surface_revision <= old.surface_revision)
                {
                    return Err(invalid("invalid surface identity/revision"));
                }
                next.frame.validate()?;
                if let Some(popup) = &next.popup {
                    popup.frame.validate()?;
                }
                let next = Arc::new(next);
                if next.projection_revision == s.revision {
                    emit(ClientEvent::Surface(next.clone()))?;
                }
                *surface = Some(next);
            }
            ServerMessage::PaneSurfacePatch(patch) => {
                let current = surface
                    .as_mut()
                    .ok_or_else(|| invalid("patch before baseline"))?;
                Arc::make_mut(current).apply_patch(patch)?;
                if snapshot
                    .as_ref()
                    .is_some_and(|s| s.revision == current.projection_revision)
                {
                    emit(ClientEvent::Surface(current.clone()))?;
                }
            }
            ServerMessage::ClientShellEndpointResponseChunk {
                boot_id,
                request_id,
                final_chunk,
                data,
            } => {
                if snapshot.as_ref().is_none_or(|s| s.boot_id != boot_id) {
                    return Err(invalid("response boot mismatch"));
                }
                let total = pending.as_ref().map_or(0, |p| p.bytes.len());
                if data.len() > MAX_RESPONSE_BYTES.saturating_sub(total) {
                    return Err(invalid("response limit exceeded"));
                }
                let p = pending
                    .as_mut()
                    .filter(|p| p.id == request_id)
                    .ok_or_else(|| invalid("unsolicited response"))?;
                p.bytes.extend(data);
                if final_chunk {
                    let p = pending
                        .take()
                        .ok_or_else(|| invalid("unsolicited response"))?;
                    let response: Value = serde_json::from_slice(&p.bytes)?;
                    if response.get("id").and_then(Value::as_str) != Some(&request_id) {
                        return Err(invalid("response ID mismatch"));
                    }
                    emit(ClientEvent::Response {
                        request_id,
                        response,
                    })?;
                }
            }
            ServerMessage::ServerShutdown { reason } => {
                return Err(invalid(reason.unwrap_or_else(|| "server shutdown".into())));
            }
            other => emit(ClientEvent::Message(other))?,
        }
        Ok(())
    }
}
