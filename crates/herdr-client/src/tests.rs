use super::*;
use std::os::unix::net::UnixListener;

const SNAPSHOT: &str =
    include_str!("../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json");
const WELCOME: &str = include_str!("../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json");

fn send(stream: &mut UnixStream, message: ServerMessage) {
    write_message(stream, &message, MAX_GRAPHICS_FRAME_SIZE).unwrap();
}
fn receive(stream: &mut UnixStream) -> ClientMessage {
    read_message(stream, MAX_FRAME_SIZE).unwrap()
}
fn event(client: &Client) -> ClientEvent {
    client.events.recv_timeout(Duration::from_secs(3)).unwrap()
}

fn handshake(stream: &mut UnixStream) {
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
    assert!(!hello.surface_reuse && !hello.surface_delta && !hello.direct_graphics);
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

fn test_client() -> (Client, UnixStream, thread::JoinHandle<io::Result<()>>) {
    test_client_mode(true, false)
}

fn test_client_mode(
    active: bool,
    remote: bool,
) -> (Client, UnixStream, thread::JoinHandle<io::Result<()>>) {
    let (stream, server) = UnixStream::pair().unwrap();
    let (commands, rx) = bounded(COMMAND_CAPACITY);
    let (tx, events) = bounded(EVENT_CAPACITY);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || {
        run_connection(
            stream,
            ConnectOptions::default(),
            active,
            remote,
            rx,
            &tx,
            &worker_stop,
        )
    });
    (
        Client {
            handle: ClientHandle {
                inner: Arc::new(HandleInner {
                    commands,
                    stop,
                    next_request: AtomicU64::new(1),
                }),
            },
            events,
        },
        server,
        worker,
    )
}

#[test]
fn handshake_rejects_every_incompatible_core_selection() {
    for field in [
        "generation",
        "snapshot_codec",
        "surface_codec",
        "input_codec",
        "blob_codec",
        "error",
    ] {
        let (client, mut server, worker) = test_client();
        receive(&mut server);
        let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
        welcome[field] = match field {
            "generation" => json!(2),
            "error" => json!({"code": "no_common_core", "message": "unsupported"}),
            _ => json!("future.codec"),
        };
        send(
            &mut server,
            ServerMessage::EndpointControl {
                kind: ENDPOINT_WELCOME_KIND.into(),
                data: welcome.to_string(),
            },
        );
        assert!(worker.join().unwrap().is_err(), "{field}");
        assert!(client.events.try_recv().is_err());
    }
}

#[test]
fn full_popup_surface_is_delivered_and_invalid_cells_fail_closed() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    let mut surface = baseline();
    surface.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: "popup-1".into(),
        title: "Popup".into(),
        width: Some(ClientShellPopupSize::Percent(80)),
        height: Some(ClientShellPopupSize::Cells(1)),
        frame: surface.frame.clone(),
        mouse_reporting: true,
        sgr_pixel_mouse: false,
        pixel_width: 8,
        pixel_height: 16,
    }));
    send(&mut server, ServerMessage::PaneSurface(surface.clone()));
    assert!(matches!(event(&client), ClientEvent::Surface(s) if *s == surface));
    surface.surface_revision += 1;
    surface.popup.as_mut().unwrap().frame.cells.clear();
    send(&mut server, ServerMessage::PaneSurface(surface));
    assert!(
        worker
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("cell count")
    );
}

#[test]
fn requests_wait_for_final_response_and_preserve_fifo() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    let first = client.handle.focus_pane("boot-v1", "w1:p1").unwrap();
    let second = client.handle.focus_pane("boot-v1", "w1:p2").unwrap();
    client.handle.set_focus("boot-v1", false).unwrap();
    assert!(
        matches!(receive(&mut server), ClientMessage::ClientShellEndpointRequest { request, .. }
        if serde_json::from_str::<Value>(&request).unwrap()["id"] == first)
    );
    send(
        &mut server,
        ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: "boot-v1".into(),
            request_id: first.clone(),
            final_chunk: false,
            data: format!("{{\"id\":\"{first}\",").into_bytes(),
        },
    );
    server
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let mut byte = [0];
    assert!(matches!(
        server.read(&mut byte).unwrap_err().kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ));
    send(
        &mut server,
        ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: "boot-v1".into(),
            request_id: first,
            final_chunk: true,
            data: b"\"result\":{}}".to_vec(),
        },
    );
    assert!(matches!(event(&client), ClientEvent::Response { .. }));
    assert!(
        matches!(receive(&mut server), ClientMessage::ClientShellEndpointRequest { request, .. }
        if serde_json::from_str::<Value>(&request).unwrap()["id"] == second)
    );
    assert_eq!(
        receive(&mut server),
        ClientMessage::ClientShellFocus { focused: false }
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn future_surface_and_patch_wait_for_matching_snapshot() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    let mut future = baseline();
    future.projection_revision = 9;
    send(&mut server, ServerMessage::PaneSurface(future));
    send(
        &mut server,
        ServerMessage::PaneSurfacePatch(PaneSurfacePatch {
            boot_id: "boot-v1".into(),
            projection_revision: 9,
            base_surface_revision: 1,
            surface_revision: 2,
            rows: vec![],
            panes: vec![],
            cursor: None,
        }),
    );
    let mut snapshot: Value = serde_json::from_str(SNAPSHOT).unwrap();
    for revision in [8, 9] {
        snapshot["revision"] = revision.into();
        send(
            &mut server,
            ServerMessage::EndpointControl {
                kind: ENDPOINT_SNAPSHOT_KIND.into(),
                data: snapshot.to_string(),
            },
        );
        assert!(matches!(event(&client), ClientEvent::Snapshot(s) if s.revision == revision));
    }
    assert!(matches!(event(&client), ClientEvent::Surface(s)
        if s.projection_revision == 9 && s.surface_revision == 2));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn boot_change_in_partial_frame_prevents_queued_input() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    let mut snapshot: Value = serde_json::from_str(SNAPSHOT).unwrap();
    snapshot["boot_id"] = "replacement".into();
    let bytes = encode_message(
        &ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: snapshot.to_string(),
        },
        MAX_FRAME_SIZE,
    )
    .unwrap();
    server.write_all(&bytes[..5]).unwrap();
    thread::sleep(Duration::from_millis(100));
    client.handle.set_focus("boot-v1", true).unwrap();
    server.write_all(&bytes[5..]).unwrap();
    assert!(
        worker
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("boot changed")
    );
    let mut byte = [0];
    assert_eq!(server.read(&mut byte).unwrap(), 0);
}

#[test]
fn snapshot_surface_patch_navigation_input_resize_and_response() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    assert!(matches!(event(&client), ClientEvent::Connected(_)));
    let ClientEvent::Snapshot(s) = event(&client) else {
        panic!("snapshot missing")
    };
    assert_eq!(s.panes[0].pane_id, "w1:p1");
    send(&mut server, ServerMessage::PaneSurface(baseline()));
    let ClientEvent::Surface(first) = event(&client) else {
        panic!("surface missing")
    };
    send(
        &mut server,
        ServerMessage::PaneSurfacePatch(PaneSurfacePatch {
            boot_id: s.boot_id.clone(),
            projection_revision: 7,
            base_surface_revision: 1,
            surface_revision: 2,
            rows: vec![PaneSurfacePatchRow {
                x: 0,
                y: 0,
                cells: vec![CellData {
                    symbol: "y".into(),
                    ..first.frame.cells[0].clone()
                }],
            }],
            panes: vec![],
            cursor: None,
        }),
    );
    let ClientEvent::Surface(second) = event(&client) else {
        panic!("patched surface missing")
    };
    assert_eq!(first.frame.cells[0].symbol, "x"); // Published Arcs are immutable.
    assert_eq!(second.frame.cells[0].symbol, "y");
    assert_eq!(second.surface_revision, 2);
    client
        .handle
        .send_input(
            &s.boot_id,
            "w1:p1",
            std::iter::once(ClientPaneInputEvent::TextCommit("hello".into())),
        )
        .unwrap();
    client
        .handle
        .resize(
            &s.boot_id,
            ConnectOptions {
                surface_size: ClientSurfaceSize {
                    cols: 100,
                    rows: 30,
                },
                ..ConnectOptions::default()
            },
        )
        .unwrap();
    let id = client.handle.focus_pane(&s.boot_id, "w1:p1").unwrap();
    assert!(
        matches!(receive(&mut server), ClientMessage::ClientShellPaneInput { pane_id, events } if pane_id == "w1:p1" && events == vec![ClientPaneInputEvent::TextCommit("hello".into())])
    );
    assert!(matches!(
        receive(&mut server),
        ClientMessage::ClientShellResize {
            surface_size: ClientSurfaceSize {
                cols: 100,
                rows: 30
            },
            ..
        }
    ));
    let ClientMessage::ClientShellEndpointRequest { boot_id, request } = receive(&mut server)
    else {
        panic!("request missing")
    };
    assert_eq!(boot_id, s.boot_id);
    assert_eq!(
        serde_json::from_str::<Value>(&request).unwrap(),
        json!({"id": id, "method": "pane.focus", "params": {"pane_id": "w1:p1"}})
    );
    let response = json!({"id": id, "result": {"type": "pane_info", "pane": {"pane_id": "w1:p1", "focused": true}}}).to_string();
    let mid = response.len() / 2;
    for (final_chunk, data) in [
        (false, &response.as_bytes()[..mid]),
        (true, &response.as_bytes()[mid..]),
    ] {
        send(
            &mut server,
            ServerMessage::ClientShellEndpointResponseChunk {
                boot_id: s.boot_id.clone(),
                request_id: id.clone(),
                final_chunk,
                data: data.to_vec(),
            },
        );
    }
    assert!(
        matches!(event(&client), ClientEvent::Response { request_id, response } if request_id == id && response["result"]["pane"]["focused"] == true)
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn stale_boot_and_unsupported_commands_never_reach_socket() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    client
        .handle
        .send_input(
            "old-boot",
            "w1:p1",
            vec![ClientPaneInputEvent::Paste("bad".into())],
        )
        .unwrap();
    assert!(matches!(
        event(&client),
        ClientEvent::CommandRejected {
            request_id: None,
            ..
        }
    ));
    let unsupported = client
        .handle
        .request("boot-v1", "not.advertised", json!({}))
        .unwrap();
    assert!(
        matches!(event(&client), ClientEvent::CommandRejected { request_id: Some(id), .. } if id == unsupported)
    );
    client.handle.set_focus("boot-v1", true).unwrap();
    assert_eq!(
        receive(&mut server),
        ClientMessage::ClientShellFocus { focused: true }
    );
    let mut snapshot: Value = serde_json::from_str(SNAPSHOT).unwrap();
    snapshot["boot_id"] = "replacement-boot".into();
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: snapshot.to_string(),
        },
    );
    assert!(
        worker
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("boot changed")
    );
}

#[test]
fn fragmented_frames_survive_timeout_between_every_byte() {
    let (mut client, mut server) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(1)))
        .unwrap();
    let expected = ServerMessage::TerminalBell { count: 300 };
    let bytes = encode_message(&expected, MAX_FRAME_SIZE).unwrap();
    let mut reader = FrameReader::new();
    let mut message = None;
    for byte in bytes {
        assert!(reader.poll(&mut client).unwrap().is_none()); // timeout, preserving state
        server.write_all(&[byte]).unwrap();
        if let Some(next) = reader.poll(&mut client).unwrap() {
            message = Some(next);
        }
    }
    assert_eq!(message, Some(expected));
}

#[test]
fn malformed_handshake_and_patch_fail_closed() {
    let (client, mut server, worker) = test_client();
    receive(&mut server);
    send(
        &mut server,
        ServerMessage::Welcome {
            version: 22,
            encoding: RenderEncoding::SemanticFrame,
            error: None,
        },
    );
    assert!(worker.join().unwrap().is_err());
    drop(client);
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    send(
        &mut server,
        ServerMessage::PaneSurfacePatch(PaneSurfacePatch {
            boot_id: "boot-v1".into(),
            projection_revision: 7,
            base_surface_revision: 1,
            surface_revision: 2,
            rows: vec![],
            panes: vec![],
            cursor: None,
        }),
    );
    assert!(
        worker
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("patch before baseline")
    );
}

#[test]
fn bounded_command_queue_and_outbound_limit_are_explicit() {
    let (commands, _rx) = bounded(1);
    let handle = ClientHandle {
        inner: Arc::new(HandleInner {
            commands,
            stop: Arc::new(AtomicBool::new(false)),
            next_request: AtomicU64::new(1),
        }),
    };
    assert!(matches!(
        handle.set_focus("", true),
        Err(SendError::Invalid(_))
    ));
    handle.set_focus("boot", true).unwrap();
    assert_eq!(handle.set_focus("boot", false), Err(SendError::Full));
    assert!(matches!(
        handle.send_input(
            "boot",
            "p",
            vec![ClientPaneInputEvent::Paste("x".repeat(MAX_FRAME_SIZE))]
        ),
        Err(SendError::Invalid(_))
    ));
    handle.disconnect();
    assert_eq!(
        handle.set_focus("boot", false),
        Err(SendError::Disconnected)
    );
}

#[test]
fn cancellation_interrupts_full_event_queue_and_idle_read() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    for _ in 0..EVENT_CAPACITY + 2 {
        send(&mut server, ServerMessage::TerminalBell { count: 1 });
    }
    client.handle.disconnect();
    // Cancellation during a bounded send returns Interrupted; idle cancellation returns Ok.
    let result = worker.join().unwrap();
    assert!(result.is_ok() || result.unwrap_err().kind() == io::ErrorKind::Interrupted);

    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    drop(client.handle); // Last handle drop also cancels.
    worker.join().unwrap().unwrap();
}

#[test]
fn public_connect_delivers_shutdown_and_socket_failure() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // Deep worktree paths can exceed the Unix socket address limit on macOS.
    let path = std::env::temp_dir().join(format!(
        "test-{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    let client = connect(
        ConnectTarget::Socket(path.clone()),
        ConnectOptions::default(),
    )
    .unwrap();
    let (mut server, _) = listener.accept().unwrap();
    std::fs::remove_file(&path).unwrap();
    handshake(&mut server);
    event(&client);
    event(&client);
    send(
        &mut server,
        ServerMessage::ServerShutdown {
            reason: Some("test shutdown".into()),
        },
    );
    assert!(
        matches!(event(&client), ClientEvent::Disconnected { reason } if reason == "test shutdown")
    );
    let missing = connect(ConnectTarget::Socket(path), ConnectOptions::default()).unwrap();
    assert!(matches!(event(&missing), ClientEvent::Disconnected { .. }));
}

#[test]
fn inactive_hello_and_surface_interest_use_upstream_contract() {
    let (client, mut server, worker) = test_client_mode(false, true);
    let ClientMessage::EndpointControl { data, .. } = receive(&mut server) else {
        panic!("hello")
    };
    let hello: EndpointClientHello = serde_json::from_str(&data).unwrap();
    assert!(!hello.surface_active);
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(["client_shell.surface.set"]);
    welcome["capabilities"] = json!([
        "surface_interest",
        "presentation_effects_fence",
        "health_check"
    ]);
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: welcome.to_string(),
        },
    );
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
    assert!(matches!(event(&client), ClientEvent::Connected(_)));
    assert!(matches!(event(&client), ClientEvent::Snapshot(_)));
    let id = client.handle.set_surface_active("boot-v1", true).unwrap();
    let ClientMessage::ClientShellEndpointRequest { boot_id, request } = receive(&mut server)
    else {
        panic!("request")
    };
    assert_eq!(boot_id, "boot-v1");
    assert_eq!(
        serde_json::from_str::<Value>(&request).unwrap(),
        json!({"id":id,"method":"client_shell.surface.set","params":{"active":true}})
    );
    send(
        &mut server,
        ServerMessage::ClientShellEndpointResponseChunk {
            boot_id,
            request_id: id.clone(),
            final_chunk: true,
            data: json!({"id":id,"result":{"active":true}})
                .to_string()
                .into_bytes(),
        },
    );
    assert!(matches!(event(&client), ClientEvent::Response { request_id, .. } if request_id == id));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn inactive_and_remote_require_negotiated_capabilities() {
    for missing in [
        "surface_interest",
        "presentation_effects_fence",
        "health_check",
        "method",
    ] {
        let (client, mut server, worker) = test_client_mode(false, true);
        receive(&mut server);
        let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
        welcome["capabilities"] = json!(
            [
                "surface_interest",
                "presentation_effects_fence",
                "health_check"
            ]
            .into_iter()
            .filter(|c| *c != missing)
            .collect::<Vec<_>>()
        );
        welcome["methods"] = if missing == "method" {
            json!([])
        } else {
            json!(["client_shell.surface.set"])
        };
        send(
            &mut server,
            ServerMessage::EndpointControl {
                kind: ENDPOINT_WELCOME_KIND.into(),
                data: welcome.to_string(),
            },
        );
        assert!(worker.join().unwrap().is_err(), "{missing}");
        assert!(client.events.try_recv().is_err());
    }
    let (client, mut server, worker) = test_client();
    receive(&mut server);
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(["client_shell.surface.set"]);
    welcome["capabilities"] = json!([]);
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: welcome.to_string(),
        },
    );
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
    event(&client);
    event(&client);
    let id = client.handle.set_surface_active("boot-v1", false).unwrap();
    assert!(
        matches!(event(&client), ClientEvent::CommandRejected { request_id: Some(rejected), .. } if rejected == id)
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn health_probes_quiet_hosts_and_any_complete_message_satisfies_probe() {
    let now = Instant::now();
    let mut health = Health {
        received: now,
        ping: None,
    };
    assert!(!health.tick(now).unwrap());
    assert!(health.tick(now + Duration::from_secs(5)).unwrap());
    assert!(!health.tick(now + Duration::from_secs(14)).unwrap());
    assert!(health.tick(now + Duration::from_secs(15)).is_err());
    health.received(now + Duration::from_secs(15));
    assert!(!health.tick(now + Duration::from_secs(16)).unwrap());
    assert!(health.tick(now + Duration::from_secs(20)).unwrap());
}

#[test]
fn session_negotiates_remote_health_without_extending_snapshot_deadline() {
    for (surface_active, remote) in [(true, false), (false, false), (true, true), (false, true)] {
        for health_supported in [false, true] {
            let mut session = Session::new(surface_active, remote);
            let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
            welcome["methods"] = json!(["client_shell.surface.set"]);
            welcome["capabilities"] = json!(["surface_interest", "presentation_effects_fence"]);
            if health_supported {
                welcome["capabilities"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("health_check"));
            }
            let mut events = Vec::new();
            let result = session.handle_message(
                ServerMessage::EndpointControl {
                    kind: ENDPOINT_WELCOME_KIND.into(),
                    data: welcome.to_string(),
                },
                |event| {
                    events.push(event);
                    Ok(())
                },
            );
            if remote && !health_supported {
                assert_eq!(
                    result.unwrap_err().to_string(),
                    "SSH endpoint lacks health_check capability"
                );
                assert!(events.is_empty());
                assert!(session.welcome.is_none());
                continue;
            }
            result.unwrap();
            assert!(matches!(events.as_slice(), [ClientEvent::Connected(_)]));
            assert_eq!(session.health.is_some(), remote);
            if let Some(health) = &mut session.health {
                health.ping = Some(Instant::now());
            }
            session.started = Instant::now() - TIMEOUT - POLL;
            session
                .handle_message(
                    ServerMessage::EndpointControl {
                        kind: "endpoint.health.pong.v1".into(),
                        data: String::new(),
                    },
                    |_| panic!("health controls must not emit UI events"),
                )
                .unwrap();
            assert!(session.health.as_ref().is_none_or(|h| h.ping.is_none()));
            assert_eq!(
                session.check_timeouts().unwrap_err().to_string(),
                "handshake/snapshot timed out"
            );
        }
    }
}

#[test]
fn ssh_health_uses_named_ping_and_ignores_pong_as_an_optional_control() {
    let (client, mut server, worker) = test_client_mode(false, true);
    server
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    receive(&mut server);
    let mut welcome: Value = serde_json::from_str(WELCOME).unwrap();
    welcome["methods"] = json!(["client_shell.surface.set"]);
    welcome["capabilities"] = json!([
        "surface_interest",
        "presentation_effects_fence",
        "health_check"
    ]);
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_WELCOME_KIND.into(),
            data: welcome.to_string(),
        },
    );
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
    event(&client);
    event(&client);
    assert!(
        matches!(receive(&mut server), ClientMessage::EndpointControl { kind, data } if kind == "endpoint.health.ping.v1" && data.is_empty())
    );
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: "endpoint.health.pong.v1".into(),
            data: String::new(),
        },
    );
    send(
        &mut server,
        ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: SNAPSHOT.into(),
        },
    );
    assert!(matches!(event(&client), ClientEvent::Snapshot(_)));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn geometry_and_frame_reader_limits() {
    for (cols, rows, cell_width_px) in [(0, 24, 0), (4097, 1, 0), (1001, 1000, 0), (80, 24, 4097)] {
        assert!(
            validate_options(ConnectOptions {
                surface_size: ClientSurfaceSize { cols, rows },
                cell_width_px,
                cell_height_px: 0,
            })
            .is_err()
        );
    }
    let (mut client, mut server) = UnixStream::pair().unwrap();
    server
        .write_all(&((MAX_GRAPHICS_FRAME_SIZE + 1) as u32).to_le_bytes())
        .unwrap();
    assert!(FrameReader::new().poll(&mut client).is_err());
    let mut reader = FrameReader::new();
    reader.started = Some(Instant::now() - TIMEOUT - POLL);
    assert!(
        reader
            .poll(&mut client)
            .unwrap_err()
            .to_string()
            .contains("timed out")
    );
}

#[test]
fn response_boot_id_correlation_and_assembly_limits() {
    for case in ["boot", "id", "limit"] {
        let (client, mut server, worker) = test_client();
        handshake(&mut server);
        event(&client);
        event(&client);
        let id = client.handle.focus_pane("boot-v1", "w1:p1").unwrap();
        receive(&mut server);
        let data = match case {
            "limit" => vec![b' '; MAX_RESPONSE_BYTES + 1],
            "id" => br#"{"id":"wrong","result":{}}"#.to_vec(),
            _ => vec![],
        };
        send(
            &mut server,
            ServerMessage::ClientShellEndpointResponseChunk {
                boot_id: if case == "boot" { "stale" } else { "boot-v1" }.into(),
                request_id: id,
                final_chunk: true,
                data,
            },
        );
        let error = worker.join().unwrap().unwrap_err().to_string();
        assert!(
            error.contains(match case {
                "boot" => "boot mismatch",
                "id" => "ID mismatch",
                _ => "limit exceeded",
            }),
            "{error}"
        );
    }
}

#[test]
fn connect_options_equality_and_send_error_display() {
    let options = ConnectOptions::default();
    assert_eq!(options, ConnectOptions::default());
    for changed in [
        ConnectOptions {
            surface_size: ClientSurfaceSize { cols: 81, rows: 24 },
            ..options
        },
        ConnectOptions {
            surface_size: ClientSurfaceSize { cols: 80, rows: 25 },
            ..options
        },
        ConnectOptions {
            cell_width_px: 8,
            ..options
        },
        ConnectOptions {
            cell_height_px: 16,
            ..options
        },
    ] {
        assert_ne!(options, changed);
    }
    assert_eq!(SendError::Full.to_string(), "client command queue is full");
    assert_eq!(
        SendError::Disconnected.to_string(),
        "client is disconnected"
    );
    assert_eq!(
        SendError::Invalid("snapshot boot ID required".into()).to_string(),
        "invalid client command: snapshot boot ID required"
    );
}

#[test]
fn frame_reader_accepts_read_trait_objects_and_preserves_partial_state() {
    struct Fragmented {
        bytes: io::Cursor<Vec<u8>>,
        pause: bool,
        error: io::ErrorKind,
    }
    impl Read for Fragmented {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.pause = !self.pause;
            if self.pause {
                return Err(self.error.into());
            }
            let len = buf.len().min(1);
            self.bytes.read(&mut buf[..len])
        }
    }
    let expected = ServerMessage::TerminalBell { count: 300 };
    let bytes = encode_message(&expected, MAX_FRAME_SIZE).unwrap();
    for error in [
        io::ErrorKind::WouldBlock,
        io::ErrorKind::TimedOut,
        io::ErrorKind::Interrupted,
    ] {
        let mut input = Fragmented {
            bytes: io::Cursor::new(bytes.repeat(2)),
            pause: false,
            error,
        };
        let input: &mut dyn Read = &mut input;
        let mut reader = FrameReader::new();
        for _ in 0..2 {
            for index in 0..bytes.len() {
                assert!(reader.poll(input).unwrap().is_none());
                let message = reader.poll(input).unwrap();
                if index + 1 == bytes.len() {
                    assert_eq!(message, Some(expected.clone()));
                    assert!(reader.started.is_none());
                    assert!(reader.bytes.is_empty());
                    assert_eq!(reader.target, 4);
                } else {
                    assert!(message.is_none());
                }
            }
        }
        assert!(reader.poll(input).unwrap().is_none());
        assert_eq!(
            reader.poll(input).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
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

#[test]
fn session_response_slot_correlates_chunks_and_clears_only_on_completion() {
    let mut session = ready_session();
    let chunk =
        |id: &str, final_chunk, data: &[u8]| ServerMessage::ClientShellEndpointResponseChunk {
            boot_id: "boot-v1".into(),
            request_id: id.into(),
            final_chunk,
            data: data.into(),
        };
    let no_event = |_| -> io::Result<()> { panic!("unexpected event") };
    assert_eq!(
        session
            .handle_message(chunk("one", true, b"{}"), no_event)
            .unwrap_err()
            .to_string(),
        "unsolicited response"
    );
    session.pending = Some(Pending {
        id: "one".into(),
        bytes: Vec::new(),
        started: Instant::now(),
    });
    session
        .handle_message(chunk("one", false, br#"{"id":"one","result":"#), no_event)
        .unwrap();
    let partial = session.pending.as_ref().unwrap().bytes.clone();
    assert_eq!(
        session
            .handle_message(chunk("other", true, b"null}"), no_event)
            .unwrap_err()
            .to_string(),
        "unsolicited response"
    );
    assert_eq!(session.pending.as_ref().unwrap().bytes, partial);
    let mut events = Vec::new();
    session
        .handle_message(chunk("one", true, b"null}"), |event| {
            events.push(event);
            Ok(())
        })
        .unwrap();
    assert!(session.pending.is_none());
    assert!(
        matches!(events.as_slice(), [ClientEvent::Response { request_id, response }]
        if request_id == "one" && *response == json!({"id": "one", "result": null}))
    );
    assert_eq!(
        session
            .handle_message(chunk("one", true, b"{}"), no_event)
            .unwrap_err()
            .to_string(),
        "unsolicited response"
    );
}

#[test]
fn session_response_limit_counts_previous_chunks_and_allows_exact_limit() {
    for overflow in [false, true] {
        let mut session = ready_session();
        let response = br#"{"id":"one"}"#;
        session.pending = Some(Pending {
            id: "one".into(),
            bytes: vec![b' '; MAX_RESPONSE_BYTES - response.len()],
            started: Instant::now(),
        });
        let mut data = response.to_vec();
        if overflow {
            data.push(b' ');
        }
        let mut events = Vec::new();
        let result = session.handle_message(
            ServerMessage::ClientShellEndpointResponseChunk {
                boot_id: "boot-v1".into(),
                request_id: "one".into(),
                final_chunk: true,
                data,
            },
            |event| {
                events.push(event);
                Ok(())
            },
        );
        if overflow {
            assert_eq!(result.unwrap_err().to_string(), "response limit exceeded");
            assert!(events.is_empty());
            assert_eq!(
                session.pending.unwrap().bytes.len(),
                MAX_RESPONSE_BYTES - response.len()
            );
        } else {
            result.unwrap();
            assert!(session.pending.is_none());
            assert!(matches!(events.as_slice(), [ClientEvent::Response { .. }]));
        }
    }
}

#[test]
fn session_deadlines_and_snapshot_revision_fence() {
    let mut session = Session::new(true, false);
    session.started = Instant::now() - TIMEOUT - POLL;
    assert_eq!(
        session.check_timeouts().unwrap_err().to_string(),
        "handshake/snapshot timed out"
    );
    session
        .handle_message(
            ServerMessage::EndpointControl {
                kind: ENDPOINT_WELCOME_KIND.into(),
                data: WELCOME.into(),
            },
            |_| Ok(()),
        )
        .unwrap();
    assert!(session.check_timeouts().is_err()); // Welcome alone is not ready.

    let mut session = ready_session();
    session.started = Instant::now() - TIMEOUT - POLL;
    session.check_timeouts().unwrap();
    session.pending = Some(Pending {
        id: "one".into(),
        bytes: Vec::new(),
        started: Instant::now() - COMMAND_TIMEOUT - POLL,
    });
    assert_eq!(
        session.check_timeouts().unwrap_err().to_string(),
        "endpoint request timed out; not replayed"
    );

    let mut snapshot: Value = serde_json::from_str(SNAPSHOT).unwrap();
    snapshot["revision"] = json!(6);
    let error = session
        .handle_message(
            ServerMessage::EndpointControl {
                kind: ENDPOINT_SNAPSHOT_KIND.into(),
                data: snapshot.to_string(),
            },
            |_| panic!("regressed snapshot must not be published"),
        )
        .unwrap_err();
    assert!(error.to_string().contains("snapshot revision regressed"));
    assert_eq!(session.snapshot.unwrap().revision, 7);
}

#[test]
fn cancellation_does_not_flush_commands_behind_pending_request() {
    let (client, mut server, worker) = test_client();
    handshake(&mut server);
    event(&client);
    event(&client);
    let first = client.handle.focus_pane("boot-v1", "w1:p1").unwrap();
    client.handle.focus_pane("boot-v1", "w1:p2").unwrap();
    client.handle.set_focus("boot-v1", false).unwrap();
    assert!(
        matches!(receive(&mut server), ClientMessage::ClientShellEndpointRequest { request, .. }
        if serde_json::from_str::<Value>(&request).unwrap()["id"] == first)
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
    assert_eq!(server.read(&mut [0]).unwrap(), 0);
    assert!(client.events.try_recv().is_err());
}
