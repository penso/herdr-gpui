use super::*;
#[cfg(unix)]
use crate::transport::Listener;
use crate::{
    Error, Result,
    frame::FrameReader,
    handle::HandleInner,
    limits::{
        COMMAND_CAPACITY, COMMAND_TIMEOUT, EVENT_CAPACITY, MAX_RESPONSE_BYTES, POLL, TIMEOUT,
    },
    method::Method,
    options::validate_options,
    protocol::{endpoint::*, *},
    session::{Health, Pace, Pending, Session, Signals, SurfaceEncodings, run_connection},
    transport::Stream,
};
use crossbeam_channel::bounded;
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize},
    },
    thread,
    time::{Duration, Instant},
};

mod commands;
mod error_kinds;
mod errors;
mod focus_barrier;
mod framing;
mod handshake;
mod health;
mod host_theme_order;
mod host_themes;
mod idle_wait;
mod input_pane;
mod liveness;
#[cfg(unix)]
mod partial_input;
#[cfg(windows)]
mod partial_input_windows;
mod session_state;
mod surface_encoding;
mod surfaces;
mod version_mismatch;
const SNAPSHOT: &str =
    include_str!("../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json");
const WELCOME: &str = include_str!("../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json");

mod clipboard_tests;

fn send(stream: &mut Stream, message: ServerMessage) {
    write_message(stream, &message, MAX_GRAPHICS_FRAME_SIZE).unwrap();
}
fn receive(stream: &mut Stream) -> ClientMessage {
    read_message(stream, MAX_FRAME_SIZE).unwrap()
}
fn event(client: &Client) -> ClientEvent {
    client.events.recv_timeout(Duration::from_secs(3)).unwrap()
}

fn handshake(stream: &mut Stream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let ClientMessage::EndpointControl { kind, data } = receive(stream) else {
        panic!("not stable hello")
    };
    assert_eq!(kind, ENDPOINT_HELLO_KIND);
    let hello: EndpointClientHello = serde_json::from_str(&data).unwrap();
    assert_eq!(hello.generation, 1);
    assert!(hello.surface_active);
    assert!(hello.surface_reuse && hello.surface_delta && hello.surface_scroll);
    assert!(!hello.direct_graphics);
    send(
        stream,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: WELCOME.into(),
        },
    );
    send(
        stream,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
}

/// The handshake of an SSH link, whose welcome must offer health checks.
fn ssh_handshake(stream: &mut Stream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    receive(stream);
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(["client_shell.surface.set"]);
    welcome["capabilities"] = json!([
        "surface_interest",
        "presentation_effects_fence",
        "health_check"
    ]);
    send(
        stream,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: welcome.to_string(),
        },
    );
    send(
        stream,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
}

fn baseline() -> PaneSurfaceFrame {
    PaneSurfaceFrame {
        boot_id: "boot-v1".into(),
        projection_revision: 7,
        surface_revision: 1,
        frame: FrameData {
            width: 1,
            height: 1,
            cells: vec![CellData {
                symbol: "x".into(),
                fg: 0,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: None,
            }],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![],
        splits: vec![],
        popup: None,
        graphics: SurfaceGraphicsScene::default(),
    }
}

fn test_client() -> (Client, Stream, thread::JoinHandle<Result<()>>) {
    test_client_mode(true, false)
}

fn test_client_mode(
    active: bool,
    remote: bool,
) -> (Client, Stream, thread::JoinHandle<Result<()>>) {
    let (client, server, worker, _) = test_client_paced(active, remote, Pace::default());
    (client, server, worker)
}

/// Also returns the worker's wait count, odd while it is waiting.
fn test_client_paced(
    active: bool,
    remote: bool,
    pace: Pace,
) -> (
    Client,
    Stream,
    thread::JoinHandle<Result<()>>,
    Arc<AtomicUsize>,
) {
    let (stream, server) = Stream::pair().unwrap();
    let (commands, rx) = queue::channel(COMMAND_CAPACITY).unwrap();
    let waits = rx.waits.clone();
    let (tx, events) = bounded(EVENT_CAPACITY);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let liveness = Arc::new(AtomicBool::new(false));
    let worker_liveness = liveness.clone();
    let worker = thread::spawn(move || {
        run_connection(
            stream,
            ConnectOptions::default(),
            active,
            remote,
            rx,
            &tx,
            Signals {
                stop: &worker_stop,
                liveness: &worker_liveness,
                pace,
            },
        )
    });
    (
        Client {
            handle: ClientHandle {
                inner: Arc::new(HandleInner {
                    commands,
                    stop,
                    next_request: AtomicU64::new(1),
                    image_busy: Arc::new(AtomicBool::new(false)),
                    last_queued_theme: Default::default(),
                    liveness,
                }),
            },
            events,
        },
        server,
        worker,
        waits,
    )
}

fn ready_session() -> Session {
    let mut session = Session::new(true, false);
    let mut events = Vec::new();
    for (kind, data) in [
        (ENDPOINT_WELCOME_KIND, WELCOME),
        (ENDPOINT_SNAPSHOT_KIND, SNAPSHOT),
    ] {
        session
            .handle_message(
                ServerMessage::EndpointControl {
                    kind: kind.into(),
                    data: data.into(),
                },
                |event| {
                    events.push(event);
                    Ok(())
                },
            )
            .unwrap();
    }
    assert!(matches!(
        events.as_slice(),
        [ClientEvent::Connected(_), ClientEvent::Snapshot(_)]
    ));
    session
}
