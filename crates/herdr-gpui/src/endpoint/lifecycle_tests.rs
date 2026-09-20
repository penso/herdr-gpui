//! Exercise GUI lifecycle transitions through real, isolated client transports.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::controls::Command;
use gpui::AppContext;
use herdr_client::{
    ClientEvent,
    protocol::{endpoint::*, *},
};
use std::{
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::atomic::AtomicU64,
};

struct Server {
    stream: UnixStream,
    path: PathBuf,
}

// Keep the tested entity out of the render tree: a terminal canvas would enqueue
// unrelated native resize requests while these tests advance the lifecycle.
struct Fixture(gpui::Entity<HerdrWindow>);
impl gpui::Render for Fixture {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        gpui::div()
    }
}

impl Server {
    fn receive(&mut self) -> ClientMessage {
        read_message(&mut self.stream, MAX_FRAME_SIZE).unwrap()
    }

    fn respond(&mut self, request: &serde_json::Value) {
        let id = request["id"].as_str().unwrap();
        write_message(
            &mut self.stream,
            &ServerMessage::ClientShellEndpointResponseChunk {
                boot_id: snapshot().boot_id,
                request_id: id.into(),
                final_chunk: true,
                data: serde_json::to_vec(&serde_json::json!({"id": id, "result": {}})).unwrap(),
            },
            MAX_GRAPHICS_FRAME_SIZE,
        )
        .unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn snapshot() -> ClientShellSnapshot {
    serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap()
}

fn surface(snapshot: &ClientShellSnapshot) -> Arc<PaneSurfaceFrame> {
    Arc::new(PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: 1,
        frame: FrameData {
            width: 80,
            height: 24,
            cells: vec![],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    })
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "client worker did not finish in time"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn connected_endpoint(id: &str) -> (Endpoint, Server) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "hg-{}-{}.sock",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut endpoint = Endpoint::new(
        id.into(),
        id.into(),
        ConnectTarget::Socket(path.clone()),
        true,
    );
    endpoint.connect(ConnectOptions::default(), true);
    let mut accepted = None;
    wait_until(|| {
        accepted = listener.accept().ok();
        accepted.is_some()
    });
    let mut server = Server {
        stream: accepted.unwrap().0,
        path,
    };
    server.stream.set_nonblocking(false).unwrap();
    server
        .stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    server
        .stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    assert!(matches!(
        server.receive(),
        ClientMessage::EndpointControl { .. }
    ));
    let mut welcome: EndpointServerWelcome = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json"
    ))
    .unwrap();
    welcome.capabilities.extend([
        "surface_interest".into(),
        "presentation_effects_fence".into(),
    ]);
    welcome.methods.extend(
        [
            "client_shell.surface.set",
            "workspace.create",
            "tab.create",
            "pane.split",
            "tab.focus",
            "pane.focus",
            "workspace.focus",
            "pane.focus_direction",
            "pane.zoom",
            "pane.close",
            "tab.close",
            "command.invoke",
        ]
        .map(str::to_owned),
    );
    for (kind, data) in [
        (
            ENDPOINT_WELCOME_KIND,
            serde_json::to_string(&welcome).unwrap(),
        ),
        (
            ENDPOINT_SNAPSHOT_KIND,
            serde_json::to_string(&snapshot()).unwrap(),
        ),
    ] {
        write_message(
            &mut server.stream,
            &ServerMessage::EndpointControl {
                kind: kind.into(),
                data,
            },
            MAX_GRAPHICS_FRAME_SIZE,
        )
        .unwrap();
    }
    wait_until(|| {
        endpoint.poll(Instant::now());
        endpoint.connection.handle.is_some() && endpoint.live.snapshot.is_some()
    });
    assert!(endpoint.live.supports_surface);
    let frame = surface(endpoint.live.snapshot.as_ref().unwrap());
    endpoint
        .connection
        .inbox
        .lock()
        .unwrap()
        .apply(ClientEvent::Surface(frame));
    endpoint.poll(Instant::now());
    (endpoint, server)
}

#[gpui::test]
fn saved_selection_waits_for_snapshot_without_overwriting_preference(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut remote, _server) = connected_endpoint("ssh:saved");
    let ready = remote.live.clone();
    remote.live.snapshot = None;
    remote.initial_surface = false;
    view.update(cx, |view, cx| {
        view.catalog.desired = Some("saved".into());
        view.catalog.initialized = true;
        view.catalog.restore_pending = true;
        // The catalog and then its connection can arrive long after startup.
        view.restore_selection(cx);
        assert!(view.catalog.restore_pending);
        view.endpoints.push(remote);
        for _ in 0..10 {
            view.restore_selection(cx);
            assert_eq!(view.selected_endpoint, 0);
            assert!(view.activation_deadline.is_none());
        }
        view.endpoints[1].live = ready;
        view.restore_selection(cx);
        assert_eq!(view.selected_endpoint, 1);
        assert!(!view.catalog.restore_pending);
        assert!(view.catalog.queued_write.is_none());
        // Automatic fallback is not a user choice and must not cause a loop.
        view.switch_endpoint(LOCAL, cx);
        view.restore_selection(cx);
        assert_eq!(view.selected_endpoint, 0);
        assert_eq!(view.catalog.desired.as_deref(), Some("saved"));
        assert!(view.catalog.queued_write.is_none());
        // An explicit Local click cancels even a not-yet-ready restore.
        view.catalog.restore_pending = true;
        view.select_endpoint(LOCAL, cx);
        view.restore_selection(cx);
        assert_eq!(view.catalog.desired, None);
        assert_eq!(view.selected_endpoint, 0);
    });
}

#[gpui::test]
fn every_focus_changing_command_fences_immediate_input_until_ack_and_surface(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for (command, method) in [
        (Command::SplitRight, "pane.split"),
        (Command::SplitDown, "pane.split"),
        (Command::Tab, "tab.create"),
        (Command::Workspace, "workspace.create"),
        (Command::NextTab, "tab.focus"),
        (Command::PreviousTab, "tab.focus"),
        (Command::TabNumber(1), "tab.focus"),
        (Command::FocusLeft, "pane.focus_direction"),
        (Command::FocusRight, "pane.focus_direction"),
        (Command::FocusUp, "pane.focus_direction"),
        (Command::FocusDown, "pane.focus_direction"),
        (Command::NextPane, "pane.focus"),
        (Command::PreviousPane, "pane.focus"),
        (Command::Zoom, "pane.zoom"),
        (Command::ClosePane, "pane.close"),
        (Command::CloseTab, "tab.close"),
        (Command::WorkspacePicker, "workspace.focus"),
        (Command::Palette, "command.invoke"),
    ] {
        let (endpoint, mut server) = connected_endpoint("ssh:fixture");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints.truncate(1);
                view.endpoints.push(endpoint);
                view.selected_endpoint = 1;
                view.options = ConnectOptions::default();
                view.reset_selected(cx);
                view.activation_deadline = None;
                assert!(view.input_ready());
                view.command(command, window, cx);
                let key = |key: &str| gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                    is_held: false,
                };
                match command {
                    Command::ClosePane | Command::CloseTab => {
                        view.close_confirmation_key(&key("tab"), window, cx);
                        view.close_confirmation_key(&key("enter"), window, cx);
                    }
                    Command::WorkspacePicker => view.palette_key(&key("enter"), window, cx),
                    Command::Palette => {
                        // The configured entry follows all native entries except Palette.
                        for _ in 0..crate::controls::COMMANDS.len() - 1 {
                            view.palette_key(&key("down"), window, cx);
                        }
                        view.palette_key(&key("enter"), window, cx);
                    }
                    _ => {}
                }
                assert!(!view.input_ready(), "{method} must fence immediately");
                assert!(view.activation_deadline.is_some());
                view.send(
                    ClientPaneInputEvent::TextCommit("must not reach old pane".into()),
                    cx,
                );
                let boot = view.live.snapshot.as_ref().unwrap().boot_id.clone();
                // An ordered marker exposes any input that incorrectly escaped.
                view.endpoints[view.selected_endpoint]
                    .connection
                    .handle
                    .as_ref()
                    .unwrap()
                    .set_focus(&boot, false)
                    .unwrap();
            })
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing command");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], method);
        // herdr-client serializes API requests behind their predecessor's reply.
        server.respond(&request);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing surface barrier");
        };
        let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(barrier["method"], "client_shell.surface.set");
        assert!(matches!(
            server.receive(),
            ClientMessage::ClientShellFocus { focused: false }
        ));
        view.update(cx, |view, cx| {
            let mut next = snapshot();
            next.revision += 1;
            next.focused_pane_id = Some("new-pane".into());
            {
                let mut state = view.endpoints[view.selected_endpoint]
                    .connection
                    .inbox
                    .lock()
                    .unwrap();
                // A fresh frame alone must not open input before the ordered ack.
                state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
                state.apply(ClientEvent::Surface(surface(&next)));
            }
            view.poll_endpoints(cx);
            assert!(!view.input_ready());
            {
                let mut state = view.endpoints[view.selected_endpoint]
                    .connection
                    .inbox
                    .lock()
                    .unwrap();
                state.apply(ClientEvent::Response {
                    request_id: barrier["id"].as_str().unwrap().into(),
                    response: serde_json::json!({"result": {
                        "type": "client_shell_surface_set", "active": true,
                        "projection_revision": next.revision + 1
                    }}),
                });
            }
            view.poll_endpoints(cx);
            assert!(
                !view.input_ready(),
                "ack newer than frame still fences input"
            );
            next.revision += 1;
            {
                let mut state = view.endpoints[view.selected_endpoint]
                    .connection
                    .inbox
                    .lock()
                    .unwrap();
                state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
                state.apply(ClientEvent::Surface(surface(&next)));
            }
            view.poll_endpoints(cx);
            assert!(view.input_ready());
            assert!(view.activation_deadline.is_none());
            view.send(ClientPaneInputEvent::TextCommit("new pane only".into()), cx);
        });
        let ClientMessage::ClientShellPaneInput { pane_id, .. } = server.receive() else {
            panic!("missing input after fence");
        };
        assert_eq!(pane_id, "new-pane");
    }
}

#[gpui::test]
fn retiring_release_source_unblocks_destination_without_waiting_for_timeout(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for change in ["remove", "disable", "retarget", "disconnect"] {
        let (mut source, mut source_server) = connected_endpoint("ssh:source");
        let (mut target, mut target_server) = connected_endpoint("ssh:target");
        let profile = |id: &str| SavedHost {
            id: id.into(),
            label: id.into(),
            target: id.into(),
            session: "default".into(),
            enabled: true,
        };
        // These already-connected test transports stand in for SSH profiles;
        // catalog reconciliation must not try opening actual SSH connections.
        source.connection.target = ConnectTarget::Ssh {
            target: "source".into(),
            session: "default".into(),
        };
        target.connection.target = ConnectTarget::Ssh {
            target: "target".into(),
            session: "default".into(),
        };
        target.initial_surface = false;
        let drained = source.connection.drained.clone();
        let source_handle = source.connection.handle.clone().unwrap();
        view.update(cx, |view, cx| {
            view.endpoints.truncate(1);
            view.endpoints[0].detached = true;
            view.endpoints.extend([source, target]);
            view.selected_endpoint = 1;
            view.reset_selected(cx);
            view.select_endpoint("ssh:target", cx);
            assert_eq!(view.pending_releases.len(), 1);
            view.poll_endpoints(cx);
            assert!(!view.endpoints[2].initial_surface);
        });
        // The release went onto the old transport but is never acknowledged.
        assert!(matches!(
            source_server.receive(),
            ClientMessage::ClientShellFocus { focused: false }
        ));
        assert!(matches!(
            source_server.receive(),
            ClientMessage::ClientShellEndpointRequest { .. }
        ));
        view.update(cx, |view, cx| {
            let mut source_profile = profile("source");
            match change {
                "remove" => view.reconcile_catalog(vec![profile("target")], cx),
                "disable" => {
                    source_profile.enabled = false;
                    view.reconcile_catalog(vec![source_profile, profile("target")], cx);
                }
                "retarget" => {
                    source_profile.session = "new-session".into();
                    view.reconcile_catalog(vec![source_profile, profile("target")], cx);
                }
                _ => {}
            }
            for endpoint in &mut view.endpoints {
                endpoint.retry_at = Instant::now() + Duration::from_secs(120);
            }
        });
        if change == "disconnect" {
            source_server
                .stream
                .shutdown(std::net::Shutdown::Both)
                .unwrap();
        }
        wait_until(|| drained.load(Ordering::Acquire));
        assert!(source_handle.is_disconnected());
        view.update(cx, |view, cx| {
            assert!(!view.pending_releases.is_empty());
            view.poll_endpoints(cx);
            assert!(view.pending_releases.is_empty(), "{change}");
            assert_eq!(view.endpoints[view.selected_endpoint].id, "ssh:target");
            assert!(view.endpoints[view.selected_endpoint].initial_surface);
            assert!(view.activation_deadline.unwrap() > Instant::now());
        });
        assert!(matches!(
            target_server.receive(),
            ClientMessage::ClientShellResize { .. }
        ));
        let ClientMessage::ClientShellEndpointRequest { request, .. } = target_server.receive()
        else {
            panic!("destination not activated");
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&request).unwrap()["method"],
            "client_shell.surface.set"
        );
    }
}

#[test]
fn retry_backoff_resets_only_after_sixty_seconds_of_healthy_connection() {
    let (mut endpoint, _server) = connected_endpoint(LOCAL);
    endpoint.attempts = 8;
    let now = Instant::now();
    endpoint.online_since = None;
    endpoint.poll(now);
    endpoint.poll(now + Duration::from_secs(59));
    assert_eq!(endpoint.retry_delay(), Duration::from_secs(120));
    endpoint.poll(now + Duration::from_secs(60));
    assert_eq!(endpoint.attempts, 0);
    assert_eq!(endpoint.retry_delay(), Duration::from_millis(500));
    endpoint.connection.handle.as_ref().unwrap().disconnect();
    endpoint.poll(now + Duration::from_secs(61));
    assert_eq!(
        endpoint.retry_at,
        now + Duration::from_secs(61) + Duration::from_millis(500)
    );
    assert!(endpoint.online_since.is_none());
}

#[test]
fn brief_success_preserves_backoff_and_disconnect_restarts_stability_window() {
    let (mut endpoint, _server) = connected_endpoint(LOCAL);
    endpoint.attempts = 8;
    let now = Instant::now();
    endpoint.online_since = None;
    endpoint.poll(now);
    endpoint.poll(now + Duration::from_secs(59));
    endpoint.connection.handle.as_ref().unwrap().disconnect();
    endpoint.poll(now + Duration::from_secs(59));
    assert_eq!(endpoint.attempts, 8);
    assert!(endpoint.online_since.is_none());
    assert_eq!(endpoint.retry_at, now + Duration::from_secs(59 + 120));
    endpoint.connect(ConnectOptions::default(), false);
    assert_eq!(
        endpoint.attempts, 9,
        "automatic retry must preserve failed attempts"
    );
    assert!(endpoint.online_since.is_none());
}

#[gpui::test]
fn changed_target_and_manual_reconnect_reset_retry_history(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    view.update(cx, |view, cx| {
        let host = SavedHost {
            id: "test".into(),
            label: "Test".into(),
            target: "unused".into(),
            session: "default".into(),
            enabled: true,
        };
        view.reconcile_catalog(vec![host.clone()], cx);
        view.endpoints[1].attempts = 8;
        view.reconcile_catalog(vec![host.clone()], cx);
        assert_eq!(
            view.endpoints[1].attempts, 8,
            "unchanged catalog preserves backoff"
        );
        view.reconcile_catalog(
            vec![SavedHost {
                session: "changed".into(),
                ..host
            }],
            cx,
        );
        assert_eq!(view.endpoints[1].attempts, 0);
        assert!(view.endpoints[1].online_since.is_none());
        view.endpoints[0].attempts = 8;
        view.reconnect(cx); // Explicit isolated missing socket, never SSH/discovery.
        assert_eq!(
            view.endpoints[0].attempts, 1,
            "manual reconnect starts a fresh first attempt"
        );
        assert_eq!(view.endpoints[0].retry_delay(), Duration::from_secs(1));
    });
}
