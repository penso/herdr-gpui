//! Connection-lifetime state and the worker loop that drives it: handshake and
//! codec negotiation, boot-fenced snapshot/surface projection, the API request
//! lease with its pipelined focus activation, and health checks. Nothing here
//! reconnects; only an activation the daemon refused unprocessed is resent.

mod barrier;
pub(crate) use barrier::Barrier;

use crate::{
    Error, Result, clipboard,
    event::{ClientEvent, deliver},
    frame::FrameReader,
    handle::Command,
    limits::{
        COMMAND_TIMEOUT, IDLE_WAIT, LIVENESS_TIMEOUT, MAX_RESPONSE_BYTES, PING_TIMEOUT, POLL,
        QUIET_PING, SLOW_REQUEST, TIMEOUT,
    },
    method::Method,
    options::ConnectOptions,
    protocol::{endpoint::*, *},
    queue::CommandReceiver,
    surface_images::ImageStore,
    transport::Stream,
};
use crossbeam_channel::{Sender, TryRecvError};
use serde_json::Value;
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub(crate) struct Pending {
    pub(crate) id: String,
    pub(crate) method: Method,
    pub(crate) bytes: Vec<u8>,
    pub(crate) started: Instant,
}

// Keep ordinary blocking writes small while the inbound surface is unfinished.
pub(super) const MAX_INTERLEAVED_INPUT_BYTES: usize = 4 * 1024;

/// One bounded write of `command`'s input, resuming where the last one stopped.
/// Returns whether the whole frame is written.
fn write_input(stream: &mut Stream, command: &mut Command) -> Result<bool> {
    stream.set_write_timeout(Some(POLL))?;
    let finished = command.written.write_some(stream, &command.bytes)?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    Ok(finished)
}

fn supports_surface_interest(welcome: &EndpointServerWelcome) -> bool {
    ["surface_interest", "presentation_effects_fence"]
        .iter()
        .all(|cap| welcome.capabilities.iter().any(|c| c == cap))
        && Method::ClientShellSurfaceSet.advertised_in(&welcome.methods)
}

/// Optional surface encodings this connection may receive. The hello requests
/// all of them because it precedes the welcome, as upstream's own client does;
/// only the ones the welcome advertises are accepted afterwards.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SurfaceEncodings {
    pub(crate) reuse: bool,
    pub(crate) delta: bool,
    pub(crate) scroll: bool,
}

impl From<&EndpointServerWelcome> for SurfaceEncodings {
    fn from(welcome: &EndpointServerWelcome) -> Self {
        let advertised = |capability| welcome.capabilities.iter().any(|c| c == capability);
        Self {
            reuse: advertised(surface_reuse::CAPABILITY),
            delta: advertised(surface_delta::CAPABILITY),
            scroll: advertised(surface_scroll::CAPABILITY),
        }
    }
}

pub(crate) struct Health {
    pub(crate) received: Instant,
    /// The outstanding ping, and the instant it fails the link unanswered.
    pub(crate) ping: Option<(Instant, Instant)>,
}
impl Health {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            received: now,
            ping: None,
        }
    }
    pub(crate) fn received(&mut self, now: Instant) {
        self.received = now;
        self.ping = None;
    }
    /// Ask the link to prove itself within `timeout`, sooner than a quiet
    /// link's routine ping. Returns whether a new ping must be sent; one
    /// already outstanding only has its deadline brought forward.
    pub(crate) fn probe(&mut self, now: Instant, timeout: Duration) -> bool {
        let deadline = now + timeout;
        match &mut self.ping {
            Some((_, due)) => {
                *due = (*due).min(deadline);
                false
            }
            None => {
                self.ping = Some((now, deadline));
                true
            }
        }
    }
    /// When `tick` next has something to do.
    pub(crate) fn deadline(&self) -> Instant {
        self.ping.map_or(self.received + QUIET_PING, |(_, due)| due)
    }
    pub(crate) fn tick(&mut self, now: Instant) -> Result<bool> {
        if now < self.deadline() {
            return Ok(false);
        }
        if self.ping.is_some() {
            tracing::warn!(category = "health_check", "client timeout");
            return Err(Error::HealthTimeout);
        }
        self.ping = Some((now, now + PING_TIMEOUT));
        Ok(true)
    }
}

pub(crate) struct Session {
    pub(crate) started: Instant,
    pub(crate) surface_active: bool,
    pub(crate) remote: bool,
    pub(crate) health: Option<Health>,
    pub(crate) welcome: Option<EndpointServerWelcome>,
    pub(crate) encodings: SurfaceEncodings,
    pub(crate) snapshot: Option<Arc<ClientShellSnapshot>>,
    pub(crate) surface: Option<Arc<PaneSurfaceFrame>>,
    pub(crate) images: ImageStore,
    pub(crate) pending: Option<Pending>,
    pub(crate) barrier: Option<Barrier>,
    pub(crate) pipeline_barriers: bool,
}

impl Session {
    pub(crate) fn new(surface_active: bool, remote: bool) -> Self {
        Self {
            started: Instant::now(),
            surface_active,
            remote,
            health: None,
            welcome: None,
            encodings: SurfaceEncodings::default(),
            snapshot: None,
            surface: None,
            images: ImageStore::default(),
            pending: None,
            barrier: None,
            pipeline_barriers: false,
        }
    }

    fn handshake_deadline(&self) -> Option<Instant> {
        self.snapshot.is_none().then(|| self.started + TIMEOUT)
    }

    /// A pipelined activation keeps its own deadline beside the focus request.
    fn request_deadline(&self) -> Option<Instant> {
        self.pending
            .iter()
            .chain(self.barrier.iter().map(|b| &b.pending))
            .map(|p| p.started + COMMAND_TIMEOUT)
            .min()
    }

    /// The earliest instant `check_timeouts` fails the connection.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        [self.handshake_deadline(), self.request_deadline()]
            .into_iter()
            .flatten()
            .min()
    }

    pub(crate) fn check_timeouts(&self) -> Result<()> {
        let now = Instant::now();
        if self.handshake_deadline().is_some_and(|due| now >= due) {
            tracing::warn!(category = "initial_snapshot", "client timeout");
            return Err(Error::HandshakeTimeout);
        }
        if self.request_deadline().is_some_and(|due| now >= due) {
            tracing::warn!(category = "request", "client timeout; request not replayed");
            return Err(Error::RequestTimeout);
        }
        Ok(())
    }

    pub(crate) fn rejection_reason(&self, command: &Command) -> Option<Error> {
        if self
            .snapshot
            .as_ref()
            .is_none_or(|s| s.boot_id != command.boot_id)
        {
            Some(Error::CommandBoot)
        } else if let Some(request) = &command.request
            && self
                .welcome
                .as_ref()
                .is_none_or(|w| !request.method.advertised_in(&w.methods))
        {
            Some(Error::UnsupportedMethod)
        } else if let Some(request) = &command.request
            && request.method == Method::ClientShellSurfaceSet
            && self
                .welcome
                .as_ref()
                .is_none_or(|w| !supports_surface_interest(w))
        {
            Some(Error::UnsupportedSurfaceInterest)
        } else {
            None
        }
    }

    /// Whether room in the socket lets the batch write this attempted input: it
    /// is on the wire, no inbound frame fences it, or it is still permitted. A
    /// fenced attempt must wait for reads, or writability would spin the worker.
    pub(crate) fn input_writable(&self, command: &Command, reader: &FrameReader) -> bool {
        command.started() || reader.started.is_none() || self.permits_partial_input(command, reader)
    }

    pub(crate) fn permits_partial_input(&self, command: &Command, reader: &FrameReader) -> bool {
        if cfg!(windows)
            || self.pending.is_some()
            || self.barrier.is_some()
            || command.request.is_some()
            || command.image.is_some()
            || command.bytes.len() > MAX_INTERLEAVED_INPUT_BYTES
        {
            return false;
        }
        let (Some(snapshot), Some(surface), Some(pane_id)) = (
            self.snapshot.as_deref(),
            self.surface.as_deref(),
            command.input_pane.as_ref(),
        ) else {
            return false;
        };
        if command.boot_id != snapshot.boot_id
            || surface.popup.is_some()
            || snapshot.focused_pane_id.as_deref() != Some(pane_id.as_str())
            || !reader.has_current_surface_prefix(snapshot, surface)
        {
            return false;
        }
        let (Some(tab_id), Some(workspace_id)) = (
            snapshot.focused_tab_id.as_deref(),
            snapshot.focused_workspace_id.as_deref(),
        ) else {
            return false;
        };
        // Use the captured destination, never state inferred from an unfinished body.
        snapshot.panes.iter().any(|pane| {
            pane.pane_id == *pane_id && pane.tab_id == tab_id && pane.workspace_id == workspace_id
        }) && surface.panes.iter().any(|pane| pane.pane_id == *pane_id)
    }
}

/// The worker's own waits. Tests shorten or lengthen them to exercise a
/// deadline, or to prove that a wake rather than the cap ended a wait.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pace {
    pub(crate) idle_wait: Duration,
    pub(crate) liveness: Duration,
    pub(crate) partial_frame: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Self {
            idle_wait: IDLE_WAIT,
            liveness: LIVENESS_TIMEOUT,
            partial_frame: TIMEOUT,
        }
    }
}

/// How long the worker may block before the earliest of `deadlines`, or `cap`
/// if none is nearer. Queued commands, `stop`, liveness requests, and image
/// publication and cancellation wake it themselves.
pub(crate) fn wait_timeout(
    now: Instant,
    cap: Duration,
    deadlines: impl IntoIterator<Item = Option<Instant>>,
) -> Duration {
    deadlines
        .into_iter()
        .flatten()
        .fold(now + cap, Instant::min)
        .saturating_duration_since(now)
}

/// What the caller's handle tells the worker between commands, and the pace
/// of the worker's waits.
#[derive(Clone, Copy)]
pub(crate) struct Signals<'a> {
    /// Ends the connection without flushing or replaying anything.
    pub(crate) stop: &'a AtomicBool,
    /// Asks an SSH link for an immediate health probe.
    pub(crate) liveness: &'a AtomicBool,
    pub(crate) pace: Pace,
}

pub(crate) fn run_connection(
    mut stream: Stream,
    options: ConnectOptions,
    surface_active: bool,
    remote: bool,
    commands: CommandReceiver,
    tx: &Sender<ClientEvent>,
    signals: Signals<'_>,
) -> Result<()> {
    let Signals {
        stop,
        liveness,
        pace,
    } = signals;
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
        surface_reuse: true,
        surface_delta: true,
        surface_scroll: true,
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
    let mut reader = FrameReader::with_timeout(pace.partial_frame);
    let mut session = Session::new(surface_active, remote);
    let mut queued: Option<Command> = None;
    while !stop.load(Ordering::Acquire) {
        session.check_timeouts()?;
        reader.check_timeout()?;
        let partial_image = queued
            .as_ref()
            .and_then(|c| c.image.as_ref())
            .is_some_and(|image| image.writer.offset > 0);
        // A started image or attempted input: no probe may block behind it.
        let partial_out = queued
            .as_ref()
            .is_some_and(|c| c.started() || c.input_attempted());
        if let Some(command) = queued.as_ref().filter(|c| c.input_attempted())
            && command
                .written
                .deadline()
                .is_some_and(|due| Instant::now() >= due)
        {
            return Err(Error::InputWriteTimeout);
        }
        if let Some(image) = queued.as_ref().and_then(|c| c.image.as_ref()) {
            image.writer.check_timeout()?;
            if partial_image && image.lease.cancelled.load(Ordering::Acquire) {
                return Err(clipboard::Error::Cancelled.into());
            }
        }
        // A probe is a frame too: never insert one into a partial outbound frame.
        // Preparation does not suppress probes; actual writes have their own deadline.
        // A liveness request waits for the same boundary rather than being lost.
        if !partial_out && let Some(health) = &mut session.health {
            let now = Instant::now();
            let probe = liveness.swap(false, Ordering::AcqRel) && health.probe(now, pace.liveness);
            if health.tick(now)? || probe {
                write_message(
                    &mut stream,
                    &ClientMessage::EndpointControl {
                        kind: "endpoint.health.ping.v1".into(),
                        data: String::new(),
                    },
                    MAX_FRAME_SIZE,
                )?;
            }
        }
        // Bound the batch so continuous input cannot starve reads.
        for _ in 0..16 {
            // Never splice the resend into, or block it behind, a started or
            // attempted outbound frame.
            if reader.started.is_none()
                && !queued
                    .as_ref()
                    .is_some_and(|c| c.started() || c.input_attempted())
                && let Some(barrier) = session.take_refused_barrier()
            {
                if stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                stream.write_all(&barrier.request)?;
                session.pending = Some(Pending {
                    started: Instant::now(),
                    ..barrier.pending
                });
                continue;
            }
            let Some(mut command) = queued.take().or_else(|| commands.try_recv().ok()) else {
                break;
            };
            if stop.load(Ordering::Acquire) {
                return Ok(());
            }
            if command.image.is_none() && command.started() {
                if !write_input(&mut stream, &mut command)? {
                    queued = Some(command);
                    break;
                }
                continue;
            }
            if reader.started.is_some()
                && !command.started()
                && (!session.permits_partial_input(&command, &reader)
                    || commands.wait(&stream, false, Duration::ZERO)?)
            {
                // Retain FIFO and drain readable bytes before permitting another write.
                queued = Some(command);
                break;
            }
            if let Some(reason) = session.rejection_reason(&command) {
                tracing::debug!(category = "session_policy", "command rejected");
                deliver(
                    tx,
                    ClientEvent::CommandRejected {
                        request_id: command.request.map(|request| request.id),
                        reason,
                    },
                    stop,
                )?;
                continue;
            }
            // The daemon has one API command lease per connection. Keep FIFO order,
            // including input behind a waiting request, without draining the bound.
            // Only a surface activation behind synchronous focus is exempt.
            if command.request.is_some()
                && (session.pending.is_some() || session.barrier.is_some())
                && !session.pipelines(&command)
            {
                queued = Some(command);
                break;
            }
            if let Some(image) = &mut command.image {
                if image.lease.cancelled.load(Ordering::Acquire) {
                    if image.writer.offset > 0 {
                        return Err(clipboard::Error::Cancelled.into());
                    }
                    continue;
                }
                if command.bytes.is_empty() {
                    match image.receiver.try_recv() {
                        Ok(bytes) => command.bytes = bytes,
                        Err(TryRecvError::Empty) => {
                            if image.lease.reserved_at.elapsed() >= COMMAND_TIMEOUT {
                                image.lease.cancelled.store(true, Ordering::Release);
                                deliver(
                                    tx,
                                    ClientEvent::CommandRejected {
                                        request_id: None,
                                        reason: clipboard::Error::PreparationTimeout.into(),
                                    },
                                    stop,
                                )?;
                                continue;
                            }
                            queued = Some(command);
                            break;
                        }
                        Err(TryRecvError::Disconnected) => continue,
                    }
                }
                stream.set_write_timeout(Some(POLL))?;
                let finished = image.writer.poll(&mut stream, &command.bytes)?;
                stream.set_write_timeout(Some(Duration::from_secs(1)))?;
                if !finished {
                    queued = Some(command);
                }
                // Even a successful bounded write yields to the inbound reader.
                break;
            }
            // Input beside a partial inbound frame must not block on a peer that
            // is still sending: write what the socket takes and keep the rest.
            if reader.started.is_some() || command.input_attempted() {
                if !write_input(&mut stream, &mut command)? {
                    queued = Some(command);
                    break;
                }
                continue;
            }
            stream.write_all(&command.bytes)?;
            if let Some(request) = command.request {
                tracing::trace!(category = "api", "request sent");
                let pending = Pending {
                    id: request.id,
                    method: request.method,
                    bytes: Vec::new(),
                    started: Instant::now(),
                };
                if session.pending.is_some() {
                    session.barrier = Some(Barrier {
                        pending,
                        request: command.bytes,
                        held: None,
                        refused: false,
                    });
                } else {
                    session.pending = Some(pending);
                }
            }
        }
        let image = queued
            .as_ref()
            .and_then(|c| Some((c.image.as_ref()?, !c.bytes.is_empty())));
        let partial_image = image.is_some_and(|(image, _)| image.writer.offset > 0);
        // An inbound frame fences an image not yet begun: until the frame ends,
        // the batch can neither write it nor expire it, so neither its socket
        // room nor its deadline may end the wait.
        let image = image.filter(|_| reader.started.is_none() || partial_image);
        let input = queued.as_ref().filter(|c| c.input_attempted());
        let partial_out = partial_image || input.is_some();
        let input_ready = input.is_some_and(|c| session.input_writable(c, &reader));
        // A published image or ready input waits for room in the socket; an
        // image still being prepared is woken on publication.
        let writable = image.is_some_and(|(_, published)| published) || input_ready;
        // Commands left behind by the batch bound must not wait, nor a liveness
        // request a partial image deferred; anything else fenced by a request,
        // an image, or a partial frame waits for what unfences it. A fenced
        // command is always `queued`, so input permitted during a partial frame
        // and cut off by the bound does not wait for the frame's tail.
        // Nor may a refused activation that an image or frame deferred.
        let timeout = if (queued.is_none() && !commands.is_empty())
            || (!partial_out && session.health.is_some() && liveness.load(Ordering::Acquire))
            || (!partial_out && reader.started.is_none() && session.resend_ready())
        {
            Duration::ZERO
        } else {
            wait_timeout(
                Instant::now(),
                pace.idle_wait,
                [
                    session.deadline(),
                    reader.deadline(),
                    // A partial outbound frame defers health checks, so their
                    // deadline cannot end the wait either; the frame's own does.
                    session
                        .health
                        .as_ref()
                        .filter(|_| !partial_out)
                        .map(Health::deadline),
                    image.and_then(|(image, published)| image.deadline(published)),
                    input.and_then(|c| c.written.deadline()),
                ],
            )
        };
        if !commands.wait(&stream, writable, timeout)? {
            continue;
        }
        let read = reader.bytes.len();
        let message = match reader.poll_batch(&mut stream) {
            // Daemons before the endpoint protocol cannot decode the hello and
            // close without a word. SSH discovery vets the remote binary first,
            // so only a local close this early is read as an outdated daemon.
            Err(Error::SocketClosed) if session.welcome.is_none() && !remote => {
                return Err(Error::ClosedBeforeWelcome);
            }
            result => result?,
        };
        let Some(message) = message else {
            // A large frame can take longer than a health deadline to arrive;
            // its bytes prove the link alive as well as a whole message would.
            if reader.bytes.len() > read
                && let Some(health) = &mut session.health
            {
                health.received(Instant::now());
            }
            continue;
        };
        session.handle_message(message, |event| deliver(tx, event, stop))?;
    }
    // No queued commands are flushed on cancellation and nothing is replayed.
    Ok(())
}

impl Session {
    pub(crate) fn handle_message(
        &mut self,
        message: ServerMessage,
        mut emit: impl FnMut(ClientEvent) -> Result<()>,
    ) -> Result<()> {
        if let Some(health) = &mut self.health {
            health.received(Instant::now());
        }
        let Self {
            surface_active,
            remote,
            health,
            welcome,
            encodings,
            snapshot,
            surface,
            images,
            pending,
            barrier,
            pipeline_barriers,
            ..
        } = self;
        if welcome.is_none() {
            let (kind, data) = match message {
                ServerMessage::EndpointControl { kind, data } => (kind, data),
                // Only a daemon from before the endpoint protocol answers the
                // hello with its legacy welcome.
                ServerMessage::Welcome { .. } => return Err(Error::LegacyDaemon),
                _ => return Err(Error::ExpectedWelcome),
            };
            if kind != ENDPOINT_WELCOME_KIND {
                return Err(Error::WelcomeKind);
            }
            let w: EndpointServerWelcome = serde_json::from_str(&data)?;
            // A rejection carries the daemon's own generation, so a refused
            // generation still says which side is older.
            if w.generation != ENDPOINT_PROTOCOL_GENERATION {
                return Err(Error::EndpointGeneration {
                    generation: w.generation,
                    server_version: w.server_version,
                });
            }
            if let Some(error) = &w.error {
                return Err(Error::WelcomeRejected {
                    code: error.code.clone(),
                    message: error.message.clone(),
                });
            }
            if w.snapshot_codec != SNAPSHOT_CODEC_V1
                || w.surface_codec != SURFACE_CODEC_V1
                || w.input_codec != INPUT_CODEC_V1
                || w.blob_codec != BLOB_CODEC_V1
            {
                return Err(Error::IncompatibleCodecs);
            }
            if (*remote || !*surface_active) && !supports_surface_interest(&w) {
                return Err(Error::MissingSurfaceInterest {
                    server_version: w.server_version,
                });
            }
            if *remote {
                if !w.capabilities.iter().any(|c| c == "health_check") {
                    return Err(Error::MissingHealthCheck {
                        server_version: w.server_version,
                    });
                }
                *health = Some(Health::new(Instant::now()));
            }
            *encodings = SurfaceEncodings::from(&w);
            emit(ClientEvent::Connected(w.clone()))?;
            tracing::info!(?encodings, "endpoint handshake accepted");
            *pipeline_barriers = barrier::verified_ordering(&w.server_version);
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
                    return Err(Error::SnapshotIdentity);
                }
                let next = Arc::new(next);
                if snapshot.is_none() {
                    tracing::debug!("initial snapshot ready");
                }
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
                    if images.show() {
                        emit(ClientEvent::SurfaceImages(images.published()))?;
                    }
                    emit(ClientEvent::Surface(current.clone()))?;
                }
            }
            // Optional encodings expand into the same atomic surface and patch
            // paths, so revision fencing and validation stay in one place.
            ServerMessage::EndpointControl { kind, data }
                if kind == surface_scroll::MESSAGE_KIND =>
            {
                if !encodings.scroll {
                    return Err(Error::SurfaceEncodingNotNegotiated);
                }
                let scroll = surface_scroll::decode(&data)?;
                let current = surface.as_mut().ok_or(Error::PatchBeforeBaseline)?;
                Arc::make_mut(current).apply_scroll_patch(scroll)?;
                emit_patched(snapshot.as_deref(), current, images, &mut emit)?;
            }
            ServerMessage::EndpointControl { kind, data }
                if kind == surface_delta::MESSAGE_KIND =>
            {
                if !encodings.delta {
                    return Err(Error::SurfaceEncodingNotNegotiated);
                }
                let delta = surface_delta::decode(&data)?;
                let base = surface
                    .as_deref()
                    .ok_or(Error::EncodedSurfaceBeforeBaseline)?;
                let next = delta.reconstruct(base)?;
                accept_surface(next, snapshot.as_deref(), surface, images, &mut emit)?;
            }
            ServerMessage::EndpointControl { kind, data }
                if kind == surface_reuse::MESSAGE_KIND =>
            {
                if !encodings.reuse {
                    return Err(Error::SurfaceEncodingNotNegotiated);
                }
                let reuse = surface_reuse::decode(&data)?;
                let base = surface
                    .as_deref()
                    .ok_or(Error::EncodedSurfaceBeforeBaseline)?;
                let next = reuse.reconstruct(base)?;
                accept_surface(next, snapshot.as_deref(), surface, images, &mut emit)?;
            }
            ServerMessage::EndpointControl { .. } => {} // Unknown optional named controls are ignored.
            ServerMessage::PaneSurface(next) => {
                accept_surface(next, snapshot.as_deref(), surface, images, &mut emit)?;
            }
            ServerMessage::PaneSurfacePatch(patch) => {
                let current = surface.as_mut().ok_or(Error::PatchBeforeBaseline)?;
                Arc::make_mut(current).apply_patch(patch)?;
                emit_patched(snapshot.as_deref(), current, images, &mut emit)?;
            }
            ServerMessage::ClientShellEndpointResponseChunk {
                boot_id,
                request_id,
                final_chunk,
                data,
            } => {
                if snapshot.as_ref().is_none_or(|s| s.boot_id != boot_id) {
                    return Err(Error::ResponseBoot);
                }
                let total = pending
                    .iter()
                    .chain(barrier.iter().map(|b| &b.pending))
                    .map(|p| p.bytes.len())
                    .sum::<usize>();
                if data.len() > MAX_RESPONSE_BYTES.saturating_sub(total) {
                    return Err(Error::ResponseLimit);
                }
                let to_barrier = barrier.as_ref().is_some_and(|b| b.awaits(&request_id));
                let p = if to_barrier {
                    barrier.as_mut().map(|b| &mut b.pending)
                } else {
                    pending.as_mut().filter(|p| p.id == request_id)
                }
                .ok_or(Error::UnsolicitedResponse)?;
                p.bytes.extend(data);
                if final_chunk {
                    let p = if to_barrier {
                        let p = &mut barrier.as_mut().ok_or(Error::UnsolicitedResponse)?.pending;
                        Pending {
                            id: p.id.clone(),
                            method: p.method,
                            bytes: std::mem::take(&mut p.bytes),
                            started: p.started,
                        }
                    } else {
                        pending.take().ok_or(Error::UnsolicitedResponse)?
                    };
                    let response: Value = serde_json::from_slice(&p.bytes)?;
                    if response.get("id").and_then(Value::as_str) != Some(&request_id) {
                        return Err(Error::ResponseId);
                    }
                    let elapsed = p.started.elapsed();
                    let elapsed_ms = elapsed.as_millis() as u64;
                    let api_error = response.get("error").is_some_and(|error| !error.is_null());
                    if api_error {
                        tracing::warn!(elapsed_ms, "API request failed");
                    } else if elapsed > SLOW_REQUEST {
                        tracing::warn!(elapsed_ms, api_error, "slow API request completed");
                    } else {
                        tracing::trace!(elapsed_ms, api_error, "API request completed");
                    }
                    if let Some(b) = barrier.as_mut().filter(|_| to_barrier) {
                        if barrier::endpoint_busy(&response) {
                            tracing::debug!(
                                category = "api",
                                "surface activation resent after focus"
                            );
                            *pipeline_barriers = false;
                            b.refused = true;
                            return Ok(());
                        }
                        if pending.is_some() {
                            // Its bytes stay counted until delivery, so the two
                            // responses share one assembly bound.
                            b.pending.bytes = p.bytes;
                            b.held = Some(response);
                            return Ok(());
                        }
                        *barrier = None;
                    }
                    emit(ClientEvent::Response {
                        request_id,
                        response,
                    })?;
                    if pending.is_none()
                        && let Some(held) = barrier.as_mut().and_then(|b| b.held.take())
                    {
                        let id = barrier.take().map(|b| b.pending.id).unwrap_or_default();
                        emit(ClientEvent::Response {
                            request_id: id,
                            response: held,
                        })?;
                    }
                }
            }
            ServerMessage::ServerShutdown { reason } => {
                return Err(Error::ServerShutdown(
                    reason.unwrap_or_else(|| "server shutdown".into()),
                ));
            }
            other => emit(ClientEvent::Message(other))?,
        }
        Ok(())
    }
}

/// Fences a complete surface on boot and revision, validates it, and retains it
/// as the baseline whether or not its projection is current.
fn accept_surface(
    mut next: PaneSurfaceFrame,
    snapshot: Option<&ClientShellSnapshot>,
    surface: &mut Option<Arc<PaneSurfaceFrame>>,
    images: &mut ImageStore,
    emit: &mut impl FnMut(ClientEvent) -> Result<()>,
) -> Result<()> {
    let s = snapshot.ok_or(Error::SurfaceBeforeSnapshot)?;
    if next.boot_id != s.boot_id
        || surface
            .as_ref()
            .is_some_and(|old| next.surface_revision <= old.surface_revision)
    {
        return Err(Error::SurfaceIdentity);
    }
    next.frame.validate()?;
    if let Some(popup) = &next.popup {
        popup.frame.validate()?;
    }
    let mut changed = images.receive(&mut next);
    let next = Arc::new(next);
    let shown = next.projection_revision == s.revision;
    if shown {
        changed |= images.show();
    }
    // Pixels precede the surface that places them.
    if changed {
        emit(ClientEvent::SurfaceImages(images.published()))?;
    }
    if shown {
        emit(ClientEvent::Surface(next.clone()))?;
    }
    *surface = Some(next);
    Ok(())
}

fn emit_patched(
    snapshot: Option<&ClientShellSnapshot>,
    current: &Arc<PaneSurfaceFrame>,
    images: &mut ImageStore,
    emit: &mut impl FnMut(ClientEvent) -> Result<()>,
) -> Result<()> {
    if snapshot.is_some_and(|s| s.revision == current.projection_revision) {
        // A patch keeps the scene, but emitting it releases what an older
        // surface on screen still held.
        if images.show() {
            emit(ClientEvent::SurfaceImages(images.published()))?;
        }
        emit(ClientEvent::Surface(current.clone()))?;
    }
    Ok(())
}
