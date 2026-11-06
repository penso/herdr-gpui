use super::*;
use crate::{
    handle::{Command, PendingRequest},
    session::{Barrier, MAX_INTERLEAVED_INPUT_BYTES},
};
use std::io::Cursor;

mod attempted;

fn pane_surface() -> PaneSurfaceFrame {
    let mut surface = baseline();
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    surface.panes.push(PaneSurfacePane {
        pane_id: "w1:p1".into(),
        content_revision: 1,
        rect,
        inner_rect: rect,
        scrollbar_rect: None,
        scroll: None,
        focused: true,
        mouse_reporting: true,
        sgr_pixel_mouse: false,
        alternate_screen_active: false,
        pixel_width: 0,
        pixel_height: 0,
    });
    surface
}

fn settled() -> Session {
    let mut session = ready_session();
    session
        .handle_message(ServerMessage::PaneSurface(pane_surface()), |_| Ok(()))
        .unwrap();
    session
}

fn command(message: ClientMessage) -> Command {
    let input_pane = match &message {
        ClientMessage::ClientShellPaneInput { pane_id, .. } => Some(pane_id.clone()),
        _ => None,
    };
    Command {
        boot_id: "boot-v1".into(),
        bytes: encode_message(&message, MAX_FRAME_SIZE).unwrap(),
        input_pane,
        written: Default::default(),
        request: None,
        image: None,
    }
}

fn pane_input(events: Vec<ClientPaneInputEvent>) -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: "w1:p1".into(),
        events,
    }
}

fn key() -> ClientPaneInputEvent {
    ClientPaneInputEvent::Key {
        code: ClientKeyCode::Char('p'),
        modifiers: 0,
        kind: ClientKeyKind::Press,
        repeat_count: 1,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    }
}

fn wheel() -> ClientPaneInputEvent {
    ClientPaneInputEvent::Mouse {
        kind: ClientMouseKind::ScrollDown,
        position: ClientMousePosition::Cell { column: 0, row: 0 },
        geometry: None,
        modifiers: 0,
        lines: 1,
    }
}

fn partial(message: ServerMessage) -> FrameReader {
    let bytes = encode_message(&message, MAX_GRAPHICS_FRAME_SIZE).unwrap();
    let mut reader = FrameReader::new();
    let mut input = Cursor::new(&bytes[..bytes.len() - 1]);
    while reader.bytes.len() < bytes.len() - 1 {
        assert!(reader.poll(&mut input).unwrap().is_none());
    }
    reader
}

fn next_surface() -> PaneSurfaceFrame {
    let mut surface = pane_surface();
    surface.surface_revision = 2;
    surface
}

#[test]
fn partial_permission_uses_target_provenance_and_latest_focused_snapshot() {
    let mut session = settled();
    let reader = partial(ServerMessage::PaneSurface(next_surface()));
    let mut input = command(pane_input(vec![key()]));
    assert!(session.permits_partial_input(&input, &reader));
    input.input_pane = None;
    assert!(!session.permits_partial_input(&input, &reader));
    input.input_pane = Some("w1:p2".into());
    assert!(!session.permits_partial_input(&input, &reader));
    input.input_pane = Some("w1:p1".into());
    input.boot_id = "old-boot".into();
    assert!(!session.permits_partial_input(&input, &reader));
    input.boot_id = "boot-v1".into();
    input.request = Some(PendingRequest {
        id: "request".into(),
        method: Method::PaneFocus,
    });
    assert!(!session.permits_partial_input(&input, &reader));
    input.request = None;
    let (_, receiver) = bounded(1);
    input.image = Some(clipboard::ImageSlot {
        receiver,
        lease: Arc::new(clipboard::ImageLease {
            busy: Arc::new(AtomicBool::new(false)),
            claimed: AtomicBool::new(false),
            cancelled: Arc::new(AtomicBool::new(false)),
            finished: Arc::new(AtomicBool::new(false)),
            reserved_at: Instant::now(),
            waker: queue::channel(1).unwrap().0.waker(),
        }),
        writer: Default::default(),
    });
    assert!(!session.permits_partial_input(&input, &reader));
    input.image = None;
    session.pending = Some(Pending {
        id: "request".into(),
        method: Method::PaneFocus,
        bytes: vec![],
        started: Instant::now(),
    });
    assert!(!session.permits_partial_input(&input, &reader));
    session.pending = None;
    Arc::make_mut(session.snapshot.as_mut().unwrap()).focused_pane_id = Some("w1:p2".into());
    assert!(!session.permits_partial_input(&input, &reader));
    Arc::make_mut(session.snapshot.as_mut().unwrap()).focused_pane_id = Some("w1:p1".into());
    Arc::make_mut(session.snapshot.as_mut().unwrap()).panes[0].tab_id = "another-tab".into();
    assert!(!session.permits_partial_input(&input, &reader));
    Arc::make_mut(session.snapshot.as_mut().unwrap()).panes[0].tab_id = "w1:t1".into();
    Arc::make_mut(session.surface.as_mut().unwrap())
        .panes
        .clear();
    assert!(!session.permits_partial_input(&input, &reader));
}

#[test]
fn generic_input_and_large_semantic_frames_keep_the_partial_barrier() {
    let session = settled();
    let reader = partial(ServerMessage::PaneSurface(next_surface()));
    let generic = command(ClientMessage::ClientShellFocus { focused: true });
    assert!(!session.permits_partial_input(&generic, &reader));
    let boundary = (MAX_INTERLEAVED_INPUT_BYTES - 100..MAX_INTERLEAVED_INPUT_BYTES)
        .map(|len| {
            command(pane_input(vec![ClientPaneInputEvent::TextCommit(
                "x".repeat(len),
            )]))
        })
        .find(|command| command.bytes.len() == MAX_INTERLEAVED_INPUT_BYTES)
        .unwrap();
    assert!(session.permits_partial_input(&boundary, &reader));
    let oversized = command(pane_input(vec![ClientPaneInputEvent::Paste(
        "x".repeat(MAX_INTERLEAVED_INPUT_BYTES),
    )]));
    assert!(!session.permits_partial_input(&oversized, &reader));
}

#[test]
fn raw_surface_and_patch_identity_guards_delegate_to_complete_current_headers() {
    let mut session = settled();
    let input = command(pane_input(vec![wheel()]));
    let mut surface = next_surface();
    for (boot, projection, revision) in [("new-boot", 7, 2), ("boot-v1", 8, 2), ("boot-v1", 7, 1)] {
        surface.boot_id = boot.into();
        surface.projection_revision = projection;
        surface.surface_revision = revision;
        assert!(!session.permits_partial_input(
            &input,
            &partial(ServerMessage::PaneSurface(surface.clone()))
        ));
    }
    let patch = PaneSurfacePatch {
        boot_id: "boot-v1".into(),
        projection_revision: 7,
        base_surface_revision: 1,
        surface_revision: 2,
        rows: vec![],
        panes: vec![],
        cursor: None,
    };
    assert!(session.permits_partial_input(
        &input,
        &partial(ServerMessage::PaneSurfacePatch(patch.clone()))
    ));
    Arc::make_mut(session.snapshot.as_mut().unwrap()).revision = 8;
    let mut future_patch = patch;
    future_patch.projection_revision = 8;
    assert!(!session.permits_partial_input(
        &input,
        &partial(ServerMessage::PaneSurfacePatch(future_patch))
    ));
    Arc::make_mut(session.snapshot.as_mut().unwrap()).revision = 7;
    let unknown = partial(ServerMessage::EndpointControl {
        kind: ENDPOINT_SNAPSHOT_KIND.into(),
        data: SNAPSHOT.into(),
    });
    assert!(!session.permits_partial_input(&input, &unknown));
    let mut short = FrameReader::new();
    short.bytes = vec![1, 0];
    short.started = Some(Instant::now());
    assert!(!session.permits_partial_input(&input, &short));
    session.surface = None;
    assert!(
        !session
            .permits_partial_input(&input, &partial(ServerMessage::PaneSurface(next_surface())))
    );
}

#[test]
fn a_retained_popup_keeps_underlying_pane_input_fenced() {
    let mut session = settled();
    let reader = partial(ServerMessage::PaneSurface(next_surface()));
    let input = command(pane_input(vec![key()]));
    let surface = Arc::make_mut(session.surface.as_mut().unwrap());
    surface.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: "popup-public".into(),
        title: "Public popup".into(),
        width: None,
        height: None,
        frame: surface.frame.clone(),
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 0,
        pixel_height: 0,
    }));
    assert!(!session.permits_partial_input(&input, &reader));
}

#[test]
fn an_outstanding_pipelined_activation_blocks_partial_input_like_a_request() {
    for refused in [false, true] {
        let mut session = settled();
        let reader = partial(ServerMessage::PaneSurface(next_surface()));
        let input = command(pane_input(vec![key()]));
        assert!(session.permits_partial_input(&input, &reader));
        // Focus answered first; its activation is still unanswered or awaits resend.
        session.barrier = Some(Barrier {
            pending: Pending {
                id: "activation".into(),
                method: Method::ClientShellSurfaceSet,
                bytes: Vec::new(),
                started: Instant::now(),
            },
            request: Vec::new(),
            held: None,
            refused,
        });
        assert!(session.pending.is_none());
        assert!(!session.permits_partial_input(&input, &reader));
        session.barrier = None;
        assert!(session.permits_partial_input(&input, &reader));
    }
}

#[cfg(unix)]
mod socket {
    use super::*;
    use std::path::PathBuf;

    mod backpressure;
    mod batch_bound;
    mod geometry;

    fn large_surface() -> PaneSurfaceFrame {
        let mut surface = pane_surface();
        surface.frame.width = 128;
        surface.frame.height = 64;
        surface.frame.cells = vec![surface.frame.cells[0].clone(); 128 * 64];
        let rect = SurfaceRect {
            x: 0,
            y: 0,
            width: 128,
            height: 64,
        };
        surface.panes[0].rect = rect;
        surface.panes[0].inner_rect = rect;
        surface
    }

    fn peer() -> (Client, Stream, Stream) {
        let (stream, mut server) = Stream::pair().unwrap();
        let observer = stream.try_clone().unwrap();
        let client = connect_with_connector(
            ConnectTarget::Socket(PathBuf::from("/unused-public-fixture.sock")),
            ConnectOptions::default(),
            true,
            move |_, _| Ok::<_, io::Error>(stream),
        )
        .unwrap();
        handshake(&mut server);
        event(&client);
        event(&client);
        send(&mut server, ServerMessage::PaneSurface(large_surface()));
        assert!(matches!(event(&client), ClientEvent::Surface(_)));
        (client, server, observer)
    }

    fn consumed(observer: &Stream) {
        let due = Instant::now() + Duration::from_secs(3);
        while rustix::io::ioctl_fionread(observer).unwrap() != 0 {
            assert!(
                Instant::now() < due,
                "worker did not consume public fragment"
            );
            thread::yield_now();
        }
    }

    fn fragment(server: &mut Stream, observer: &Stream, message: ServerMessage) -> Vec<u8> {
        let bytes = encode_message(&message, MAX_GRAPHICS_FRAME_SIZE).unwrap();
        assert!(bytes.len() > 1024);
        server.write_all(&bytes[..1024]).unwrap();
        consumed(observer);
        bytes[1024..].to_vec()
    }

    fn quiet(server: &mut Stream) {
        server
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let error = server.read(&mut [0]).unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        ));
        server
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
    }

    fn close(client: Client, mut server: Stream, observer: Stream) {
        client.handle.disconnect();
        drop(observer);
        assert_eq!(server.read(&mut [0]).unwrap(), 0);
    }

    #[test]
    fn focused_key_and_wheel_arrive_before_current_full_or_patch_body_is_released() {
        for patch in [false, true] {
            let (client, mut server, observer) = peer();
            let message = if patch {
                ServerMessage::PaneSurfacePatch(PaneSurfacePatch {
                    boot_id: "boot-v1".into(),
                    projection_revision: 7,
                    base_surface_revision: 1,
                    surface_revision: 2,
                    rows: vec![
                        PaneSurfacePatchRow {
                            x: 0,
                            y: 0,
                            cells: vec![large_surface().frame.cells[0].clone(); 128]
                        };
                        64
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(y, mut row)| {
                        row.y = y as u16;
                        row
                    })
                    .collect(),
                    panes: large_surface().panes,
                    cursor: None,
                })
            } else {
                let mut surface = large_surface();
                surface.surface_revision = 2;
                ServerMessage::PaneSurface(surface)
            };
            let tail = fragment(&mut server, &observer, message);
            let started = Instant::now();
            client
                .handle
                .send_input("boot-v1", "w1:p1", [key()])
                .unwrap();
            client
                .handle
                .send_input("boot-v1", "w1:p1", [wheel()])
                .unwrap();
            assert_eq!(receive(&mut server), pane_input(vec![key()]));
            assert_eq!(receive(&mut server), pane_input(vec![wheel()]));
            eprintln!(
                "public {} partial: key+wheel reached peer in {:?}",
                if patch { "patch" } else { "surface" },
                started.elapsed()
            );
            assert!(
                client.events.try_recv().is_err(),
                "partial surface was published"
            );
            server.write_all(&tail).unwrap();
            assert!(
                matches!(event(&client), ClientEvent::Surface(surface) if surface.surface_revision == 2)
            );
            close(client, server, observer);
        }
    }

    #[test]
    fn generic_fifo_head_prevents_targeted_input_overtaking() {
        let (client, mut server, observer) = peer();
        let mut surface = large_surface();
        surface.surface_revision = 2;
        let tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
        client.handle.set_focus("boot-v1", false).unwrap();
        client
            .handle
            .send_input("boot-v1", "w1:p1", [key()])
            .unwrap();
        quiet(&mut server);
        server.write_all(&tail).unwrap();
        assert!(matches!(event(&client), ClientEvent::Surface(_)));
        assert!(matches!(
            receive(&mut server),
            ClientMessage::ClientShellFocus { focused: false }
        ));
        assert_eq!(receive(&mut server), pane_input(vec![key()]));
        close(client, server, observer);
    }

    #[test]
    fn in_flight_api_retains_input_barrier_until_partial_frame_finishes() {
        let (client, mut server, observer) = peer();
        client.handle.focus_pane("boot-v1", "w1:p1").unwrap();
        assert!(matches!(
            receive(&mut server),
            ClientMessage::ClientShellEndpointRequest { .. }
        ));
        let mut surface = large_surface();
        surface.surface_revision = 2;
        let tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
        client
            .handle
            .send_input("boot-v1", "w1:p1", [key()])
            .unwrap();
        quiet(&mut server);
        server.write_all(&tail).unwrap();
        assert!(matches!(event(&client), ClientEvent::Surface(_)));
        assert_eq!(receive(&mut server), pane_input(vec![key()]));
        close(client, server, observer);
    }

    #[test]
    fn cancellation_drops_fenced_fifo_head_and_input_without_replay() {
        let (client, mut server, observer) = peer();
        let mut surface = large_surface();
        surface.surface_revision = 2;
        let _tail = fragment(&mut server, &observer, ServerMessage::PaneSurface(surface));
        client.handle.set_focus("boot-v1", false).unwrap();
        client
            .handle
            .send_input("boot-v1", "w1:p1", [key()])
            .unwrap();
        quiet(&mut server);
        close(client, server, observer);
    }

    #[test]
    fn malformed_complete_body_disconnects_and_prevents_later_writes() {
        let (client, mut server, observer) = peer();
        let mut malformed = large_surface();
        malformed.surface_revision = 2;
        malformed.frame.cells.pop();
        let tail = fragment(
            &mut server,
            &observer,
            ServerMessage::PaneSurface(malformed),
        );
        client
            .handle
            .send_input("boot-v1", "w1:p1", [key()])
            .unwrap();
        assert_eq!(receive(&mut server), pane_input(vec![key()]));
        server.write_all(&tail).unwrap();
        // Invalid bytes are already readable before this new command exists.
        // It may be rejected on enqueue or retained until validation fails;
        // neither path may write it or replay it on another connection.
        let _ = client.handle.send_input("boot-v1", "w1:p1", [wheel()]);
        assert!(
            matches!(event(&client), ClientEvent::Disconnected { reason, .. } if reason.contains("cell count"))
        );
        assert!(
            client
                .handle
                .send_input("boot-v1", "w1:p1", [wheel()])
                .is_err()
        );
        drop(observer);
        assert_eq!(server.read(&mut [0]).unwrap(), 0);
    }
}
