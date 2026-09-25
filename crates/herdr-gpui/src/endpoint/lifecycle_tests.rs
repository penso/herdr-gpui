//! Exercise GUI lifecycle transitions through real, isolated client transports.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::controls::Command;
use gpui_kit::{
    AppContext, ClipboardItem, Image, ImageFormat, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, point, px, size,
};
use herdr_client::{
    ClientEvent, Method,
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
struct Fixture(gpui_kit::Entity<HerdrWindow>);
impl gpui_kit::Render for Fixture {
    fn render(
        &mut self,
        _: &mut gpui_kit::Window,
        _: &mut Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        gpui_kit::div()
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

/// Poll until the inbox has been projected into `live`, then return.
///
/// `ConnectionBridge::take_update` reads the inbox under `try_lock` so the UI
/// thread never blocks on the socket worker: a poll that races the worker
/// projects nothing and the next one delivers it. A test that polls once and
/// asserts is therefore reading whatever `live` happened to hold, which is a
/// revision behind whenever the worker held the lock. Drive polls until the
/// expected state lands instead of assuming a single poll suffices.
fn project_until(
    view: &mut HerdrWindow,
    cx: &mut Context<HerdrWindow>,
    what: &str,
    ready: impl Fn(&HerdrWindow) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        view.poll_endpoints(cx);
        if ready(view) {
            return;
        }
        assert!(Instant::now() < deadline, "{what} was never projected");
        std::thread::sleep(Duration::from_millis(1));
    }
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
            Method::ClientShellSurfaceSet,
            Method::WorkspaceCreate,
            Method::TabCreate,
            Method::PaneSplit,
            Method::LayoutSetSplitRatio,
            Method::TabFocus,
            Method::PaneFocus,
            Method::WorkspaceFocus,
            Method::PaneFocusDirection,
            Method::PaneZoom,
            Method::PaneClear,
            Method::PaneClose,
            Method::TabClose,
            Method::CommandInvoke,
            Method::WorkspaceClose,
            Method::WorktreeCreate,
            Method::WorktreeOpen,
            Method::WorktreeRemove,
        ]
        .map(|method| method.as_str().to_owned()),
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

fn prepare_mouse(view: &mut HerdrWindow, endpoint: Endpoint) {
    view.endpoints.truncate(1);
    view.endpoints.push(endpoint);
    view.selected_endpoint = 1;
    view.options = ConnectOptions::default();
    view.reset_selected();
    view.activation_deadline = None;
    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    let mut inactive = snapshot.panes[0].clone();
    inactive.pane_id = "w1:p2".into();
    inactive.focused = false;
    snapshot.panes.push(inactive);
    Arc::make_mut(view.live.surface.as_mut().unwrap()).panes = [0, 40]
        .into_iter()
        .map(|x| PaneSurfacePane {
            pane_id: if x == 0 { "w1:p1" } else { "w1:p2" }.into(),
            content_revision: 1,
            rect: SurfaceRect {
                x,
                y: 0,
                width: 40,
                height: 24,
            },
            inner_rect: SurfaceRect {
                x: x + 1,
                y: 1,
                width: 38,
                height: 22,
            },
            scrollbar_rect: None,
            scroll: None,
            focused: x == 0,
            mouse_reporting: true,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 760,
            pixel_height: 880,
        })
        .collect();
    view.cell_width = 10.;
    view.bounds = gpui_kit::Bounds::new(
        point(px(100.), px(50.)),
        size(px(800.), px(24. * view.config.terminal.line_height())),
    );
    assert!(view.input_ready());
}

fn mouse_position(view: &HerdrWindow, column: f32, row: f32) -> gpui_kit::Point<gpui_kit::Pixels> {
    view.bounds.origin
        + point(
            px(column * 10.),
            px(row * view.config.terminal.line_height()),
        )
}

fn mouse_event(kind: ClientMouseKind, column: u16, row: u16) -> ClientPaneInputEvent {
    ClientPaneInputEvent::Mouse {
        kind,
        position: ClientMousePosition::Cell { column, row },
        geometry: None,
        modifiers: 0,
        lines: 1,
    }
}

fn clipboard_image(bytes: &[u8]) -> ClipboardItem {
    ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, bytes.to_vec()))
}

fn prepare_remote_image(view: &mut HerdrWindow, mut endpoint: Endpoint) {
    // Only change the classification after connecting the isolated socket harness.
    endpoint.connection.target = ConnectTarget::Ssh {
        target: "unused-image-test.invalid".into(),
        session: "default".into(),
    };
    prepare_mouse(view, endpoint);
    assert!(view.accepts_remote_images());
}

fn image_popup(view: &mut HerdrWindow, id: &str) {
    let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
    surface.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: id.into(),
        title: String::new(),
        width: None,
        height: None,
        frame: surface.frame.clone(),
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 800,
        pixel_height: 480,
    }));
}

fn wait_image_finished(view: &gpui_kit::Entity<HerdrWindow>, cx: &mut gpui_kit::VisualTestContext) {
    // GPUI tasks publish the frame; only the socket worker can finish its FIFO slot.
    wait_until(|| {
        view.update(cx, |view, _| {
            view.cancel_stale_image();
            view.pending_images.is_empty()
        })
    });
}

/// Navigation leaves its acknowledged activation recorded. Cmd-V must still
/// paste afterwards, and only a navigation still in flight may drop it.
#[gpui_kit::test]
fn text_paste_survives_a_settled_navigation(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("image");
    let paste = gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke::parse("cmd-v").unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    let settled = |view: &HerdrWindow| crate::state::SurfaceActivation {
        request: "activate-1".into(),
        boot: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
        revision: Some(view.live.surface.as_ref().unwrap().projection_revision),
        failed: false,
        focus: None,
        active: true,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint);
            view.live.activation = Some(settled(view));
            assert!(view.live.surface_ready() && !view.live.activation_pending());
            cx.write_to_clipboard(ClipboardItem::new_string("after navigation".into()));
            view.key_down(&paste, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::Paste("after navigation".into())],
        }
    );
    wait_image_finished(&view, cx);
    // Linux sends Cmd-V text synchronously from GPUI's clipboard, so there is
    // no background read for a navigation to overtake.
    if cfg!(target_os = "linux") {
        return;
    }

    // A navigation starting while the clipboard is read cancels that paste.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("stale target".into()));
            view.key_down(&paste, window, cx);
            view.live.activation.as_mut().unwrap().revision = None;
            assert!(view.live.activation_pending());
        });
    });
    cx.run_until_parked();
    wait_image_finished(&view, cx);
    view.update(cx, |view, cx| {
        view.live.activation = Some(settled(view));
        view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit("sentinel".into())],
        }
    );
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui_kit::test]
fn connected_image_paste_captures_pane_before_immediate_text_and_enter(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:image");
    let enter = gpui_kit::KeyDownEvent {
        keystroke: gpui_kit::Keystroke::parse("enter").unwrap(),
        is_held: false,
        prefer_character_input: false,
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_remote_image(view, endpoint);
            assert!(view.paste_terminal_clipboard(clipboard_image(&[0, 1, 255]), false, cx));
            assert_eq!(view.pending_images.len(), 1);
            // Focus can move while preparation runs; the image retains its original pane.
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_pane_id =
                Some("w1:p2".into());
            view.send(ClientPaneInputEvent::TextCommit("after image".into()), cx);
            view.key_down(&enter, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        ClientMessage::ClipboardImage {
            target: ClientClipboardImageTarget::Pane("w1:p1".into()),
            extension: "png".into(),
            data: vec![0, 1, 255],
        }
    );
    for event in [
        ClientPaneInputEvent::TextCommit("after image".into()),
        crate::terminal::key_input(&enter, true).unwrap(),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p2".into(),
                events: vec![event],
            }
        );
    }
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui_kit::test]
fn connected_image_paste_popup_never_reaches_underlying_pane(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:image");
    view.update(cx, |view, cx| {
        prepare_remote_image(view, endpoint);
        image_popup(view, "image-popup");
        assert!(view.paste_terminal_clipboard(clipboard_image(&[42]), false, cx));
        view.send(ClientPaneInputEvent::TextCommit("popup only".into()), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        ClientMessage::ClipboardImage {
            target: ClientClipboardImageTarget::Popup("image-popup".into()),
            extension: "png".into(),
            data: vec![42],
        }
    );
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPopupInput {
            terminal_id: "image-popup".into(),
            events: vec![ClientPaneInputEvent::TextCommit("popup only".into())],
        }
    );
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui_kit::test]
fn connected_image_paste_image_only_preserves_text(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for remote in [false, true] {
        let (endpoint, mut server) = connected_endpoint("image");
        view.update(cx, |view, cx| {
            if remote {
                prepare_remote_image(view, endpoint);
            } else {
                prepare_mouse(view, endpoint);
                assert!(!view.accepts_remote_images());
            }
            let text = ClipboardItem::new_string("ordinary text".into());
            assert!(!view.paste_terminal_clipboard(text.clone(), true, cx));
            assert!(view.pending_images.is_empty());
            assert!(view.paste_terminal_clipboard(text, false, cx));
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
        cx.run_until_parked();
        for event in [
            ClientPaneInputEvent::Paste("ordinary text".into()),
            ClientPaneInputEvent::TextCommit("sentinel".into()),
        ] {
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![event],
                },
                "remote={remote}"
            );
        }
        view.read_with(cx, |view, _| {
            assert!(view.pending_images.is_empty());
            assert!(view.local_error.is_none());
        });
    }
}

#[gpui_kit::test]
fn connected_image_paste_local_bridges_clipboard_image_but_not_paths(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("local image.png");
    std::fs::write(&path, [1, 2, 3]).unwrap();
    let text = format!("'{}'", path.display());
    let (endpoint, mut server) = connected_endpoint("image");
    view.update(cx, |view, cx| {
        prepare_mouse(view, endpoint);
        assert!(view.accepts_clipboard_images());
        assert!(!view.accepts_remote_images());
        assert!(view.paste_terminal_clipboard(clipboard_image(&[42]), false, cx));
        assert_eq!(view.pending_images.len(), 1);
        // A local pane reads the original file; only remote panes need its bytes.
        assert!(view.paste_terminal_clipboard(ClipboardItem::new_string(text.clone()), false, cx));
        assert_eq!(view.pending_images.len(), 1);
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        ClientMessage::ClipboardImage {
            target: ClientClipboardImageTarget::Pane("w1:p1".into()),
            extension: "png".into(),
            data: vec![42],
        }
    );
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::Paste(text)],
        }
    );
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui_kit::test]
fn connected_image_paste_missing_path_falls_back_in_reserved_order(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let directory = tempfile::tempdir().unwrap();
    let text = format!(
        "'{}'\r\n",
        directory.path().join("missing image.png").display()
    );
    let (endpoint, mut server) = connected_endpoint("ssh:image");
    view.update(cx, |view, cx| {
        prepare_remote_image(view, endpoint);
        assert!(view.paste_terminal_clipboard(ClipboardItem::new_string(text.clone()), false, cx));
        assert_eq!(view.pending_images.len(), 1);
        view.send(
            ClientPaneInputEvent::TextCommit("after fallback".into()),
            cx,
        );
    });
    cx.run_until_parked();
    for event in [
        ClientPaneInputEvent::Paste(text),
        ClientPaneInputEvent::TextCommit("after fallback".into()),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![event],
            }
        );
    }
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui_kit::test]
fn connected_image_paste_cancels_stale_preparation_without_blocking_fifo(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for change in [
        "epoch",
        "generation",
        "boot",
        "menu",
        "pane",
        "popup",
        "endpoint",
        "local",
    ] {
        for cancel_before_prepare in [false, true] {
            let (endpoint, mut server) = connected_endpoint("ssh:image");
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    prepare_remote_image(view, endpoint);
                    if change == "popup" {
                        image_popup(view, "original-popup");
                    }
                    assert!(view.paste_terminal_clipboard(clipboard_image(&[42]), false, cx));
                    assert_eq!(view.pending_images.len(), 1);
                    match change {
                        "epoch" => view.selection_epoch += 1,
                        "generation" => view.endpoints[1].generation += 1,
                        "boot" => {
                            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).boot_id =
                                "replacement".into();
                            Arc::make_mut(view.live.surface.as_mut().unwrap()).boot_id =
                                "replacement".into();
                        }
                        "menu" => view.open_keybinds(window, cx),
                        "pane" => Arc::make_mut(view.live.surface.as_mut().unwrap())
                            .panes
                            .retain(|pane| pane.pane_id != "w1:p1"),
                        "popup" => image_popup(view, "replacement-popup"),
                        "endpoint" => view.endpoints[1].id = "ssh:replacement".into(),
                        "local" => {
                            view.endpoints[1].connection.target =
                                ConnectTarget::Socket(server.path.clone());
                        }
                        _ => unreachable!(),
                    }
                    if cancel_before_prepare {
                        view.cancel_stale_image();
                        // Cancellation retains the task until its background work exits.
                        assert_eq!(view.pending_images.len(), 1);
                    }
                    view.endpoints[1]
                        .connection
                        .handle
                        .as_ref()
                        .unwrap()
                        .set_focus(&snapshot().boot_id, false)
                        .unwrap();
                });
            });
            cx.run_until_parked();
            // A bounded FIFO sentinel catches both stray images and stuck cancelled slots.
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellFocus { focused: false },
                "{change}, cancel_before_prepare={cancel_before_prepare}"
            );
            wait_image_finished(&view, cx);
            view.read_with(cx, |view, _| {
                assert!(view.local_error.is_none(), "{change}")
            });
        }
    }
}

#[gpui_kit::test]
fn connected_image_paste_busy_guard_releases_after_completion(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:image");
    view.update(cx, |view, _| prepare_remote_image(view, endpoint));
    for byte in [1, 2] {
        view.update(cx, |view, cx| {
            assert!(view.pending_images.is_empty());
            assert!(view.paste_terminal_clipboard(clipboard_image(&[byte]), false, cx));
            assert!(view.paste_terminal_clipboard(clipboard_image(&[99]), false, cx));
            assert_eq!(
                view.local_error.as_deref(),
                Some(
                    format!(
                        "Image not sent: {}",
                        herdr_client::Error::ClipboardImageBusy
                    )
                    .as_str()
                )
            );
            view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
        });
        cx.run_until_parked();
        assert_eq!(
            server.receive(),
            ClientMessage::ClipboardImage {
                target: ClientClipboardImageTarget::Pane("w1:p1".into()),
                extension: "png".into(),
                data: vec![byte],
            }
        );
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![ClientPaneInputEvent::TextCommit("sentinel".into())],
            }
        );
        wait_image_finished(&view, cx);
    }
}

#[gpui_kit::test]
fn connected_image_paste_reconnect_cancels_old_task_and_keeps_single_preparation(
    cx: &mut gpui_kit::TestAppContext,
) {
    use std::io::Read as _;

    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut old_server) = connected_endpoint("ssh:image");
    let (replacement, mut server) = connected_endpoint("ssh:image");
    let directory = tempfile::tempdir().unwrap();
    view.update(cx, |view, cx| {
        prepare_remote_image(view, endpoint);
        assert!(view.paste_terminal_clipboard(clipboard_image(&[1]), false, cx));
        view.send(
            ClientPaneInputEvent::TextCommit("must not replay".into()),
            cx,
        );
        // Exercise reconnect without ever launching SSH or discovering a personal daemon.
        view.endpoints[1].connection.target =
            ConnectTarget::Socket(directory.path().join("missing.sock"));
        view.reconnect();
        assert_eq!(view.pending_images.len(), 1);
        prepare_remote_image(view, replacement);
        assert!(view.paste_terminal_clipboard(clipboard_image(&[2]), false, cx));
        assert!(view.local_error.is_some());
        view.send(
            ClientPaneInputEvent::TextCommit("replacement sentinel".into()),
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(old_server.stream.read(&mut [0]).unwrap(), 0);
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit(
                "replacement sentinel".into()
            )],
        }
    );
    wait_image_finished(&view, cx);
    view.update(cx, |view, cx| {
        assert!(view.paste_terminal_clipboard(clipboard_image(&[3]), false, cx));
    });
    cx.run_until_parked();
    assert_eq!(
        server.receive(),
        ClientMessage::ClipboardImage {
            target: ClientClipboardImageTarget::Pane("w1:p1".into()),
            extension: "png".into(),
            data: vec![3],
        }
    );
    wait_image_finished(&view, cx);
}

#[gpui_kit::test]
fn connected_image_paste_key_down_ctrl_v_and_cmd_v(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for remote in [false, true] {
        for key in ["ctrl-v", "cmd-v"] {
            for image in [false, true] {
                let (endpoint, mut server) = connected_endpoint("image");
                let event = gpui_kit::KeyDownEvent {
                    keystroke: gpui_kit::Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                };
                cx.update(|window, cx| {
                    view.update(cx, |view, cx| {
                        if remote {
                            prepare_remote_image(view, endpoint);
                        } else {
                            prepare_mouse(view, endpoint);
                        }
                        let item = if image {
                            clipboard_image(&[42])
                        } else {
                            ClipboardItem::new_string("clipboard text".into())
                        };
                        cx.write_to_clipboard(item.clone());
                        view.key_down(&event, window, cx);
                        assert_eq!(cx.read_from_clipboard(), Some(item));
                        // Remote clipboard reads reserve FIFO order even for text-only
                        // Ctrl-V. Local Ctrl-V stays a key for agents that read the
                        // clipboard themselves; local Cmd-V bridges images.
                        let native_paste = key == "cmd-v" && (image || !cfg!(target_os = "linux"));
                        assert_eq!(
                            view.pending_images.len(),
                            usize::from(native_paste || (remote && key == "ctrl-v"))
                        );
                        view.send(ClientPaneInputEvent::TextCommit("key sentinel".into()), cx);
                    });
                });
                cx.run_until_parked();
                if image && (remote || key == "cmd-v") {
                    assert_eq!(
                        server.receive(),
                        ClientMessage::ClipboardImage {
                            target: ClientClipboardImageTarget::Pane("w1:p1".into()),
                            extension: "png".into(),
                            data: vec![42],
                        }
                    );
                } else if key == "ctrl-v" || !image {
                    assert_eq!(
                        server.receive(),
                        ClientMessage::ClientShellPaneInput {
                            pane_id: "w1:p1".into(),
                            events: vec![if key == "ctrl-v" {
                                crate::terminal::key_input(&event, true).unwrap()
                            } else {
                                ClientPaneInputEvent::Paste("clipboard text".into())
                            }],
                        }
                    );
                }
                assert_eq!(
                    server.receive(),
                    ClientMessage::ClientShellPaneInput {
                        pane_id: "w1:p1".into(),
                        events: vec![ClientPaneInputEvent::TextCommit("key sentinel".into())],
                    },
                    "remote={remote}, key={key}, image={image}"
                );
                wait_image_finished(&view, cx);
                view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
            }
        }
    }
}

#[gpui_kit::test]
fn connected_image_paste_native_text_reservations_preserve_fifo(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:image");
    view.update(cx, |view, cx| {
        prepare_remote_image(view, endpoint);
        for text in ["first paste", "second paste"] {
            cx.write_to_clipboard(ClipboardItem::new_string(text.into()));
            view.paste_native_clipboard(false, None, cx);
        }
        assert_eq!(view.pending_images.len(), 2);
        assert!(view.local_error.is_none());
        view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
    });
    cx.run_until_parked();
    for event in [
        ClientPaneInputEvent::Paste("first paste".into()),
        ClientPaneInputEvent::Paste("second paste".into()),
        ClientPaneInputEvent::TextCommit("sentinel".into()),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![event],
            }
        );
    }
    wait_image_finished(&view, cx);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui_kit::test]
fn connected_image_paste_native_text_during_blocked_image_and_second_image_busy(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:image");
    view.update(cx, |view, cx| {
        prepare_remote_image(view, endpoint);
        let handle = view.endpoints[1].connection.handle.as_ref().unwrap();
        // The second API request holds the FIFO behind the first request's reply.
        for _ in 0..2 {
            handle
                .request(
                    &snapshot().boot_id,
                    Method::TabCreate,
                    serde_json::json!({}),
                )
                .unwrap();
        }
        assert!(view.paste_terminal_clipboard(clipboard_image(&[42]), false, cx));
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing first API request");
    };
    let first: serde_json::Value = serde_json::from_str(&request).unwrap();
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(view.pending_images.len(), 1);
        for text in ["first paste", "second paste"] {
            cx.write_to_clipboard(ClipboardItem::new_string(text.into()));
            view.paste_native_clipboard(false, None, cx);
        }
        assert_eq!(view.pending_images.len(), 3);
        assert!(view.local_error.is_none());
        cx.write_to_clipboard(clipboard_image(&[99]));
        view.paste_native_clipboard(false, None, cx);
        assert_eq!(view.pending_images.len(), 4);
        view.send(ClientPaneInputEvent::TextCommit("sentinel".into()), cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.local_error,
            Some(format!(
                "Image not sent: {}",
                herdr_client::Error::ClipboardImageBusy
            ))
        );
    });
    server.respond(&first);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing second API request");
    };
    server.respond(&serde_json::from_str(&request).unwrap());
    assert_eq!(
        server.receive(),
        ClientMessage::ClipboardImage {
            target: ClientClipboardImageTarget::Pane("w1:p1".into()),
            extension: "png".into(),
            data: vec![42],
        }
    );
    for event in [
        ClientPaneInputEvent::Paste("first paste".into()),
        ClientPaneInputEvent::Paste("second paste".into()),
        ClientPaneInputEvent::TextCommit("sentinel".into()),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![event],
            }
        );
    }
    wait_image_finished(&view, cx);
}

#[gpui_kit::test]
fn connected_image_paste_native_preparations_stay_bounded_across_reset(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for reset_all in [false, true] {
        let (endpoint, mut server) = connected_endpoint("ssh:image");
        view.update(cx, |view, cx| {
            prepare_remote_image(view, endpoint);
            let surface = view.live.surface.clone();
            for index in 0..4 {
                if index == 3 {
                    view.reset_selected();
                    view.activation_deadline = None;
                    view.live.surface = surface.clone();
                }
                cx.write_to_clipboard(ClipboardItem::new_string(format!("paste {index}")));
                view.paste_native_clipboard(false, None, cx);
                assert_eq!(view.pending_images.len(), index + 1);
            }
            if reset_all {
                view.reset_selected();
                view.live.surface = surface;
            }
            view.cancel_stale_image();
            assert_eq!(view.pending_images.len(), 4);
            // Even cancelled tasks count until their background work returns.
            view.activation_deadline = None;
            cx.write_to_clipboard(ClipboardItem::new_string("overflow".into()));
            view.paste_native_clipboard(false, None, cx);
            assert_eq!(view.pending_images.len(), 4);
            assert_eq!(
                view.local_error,
                Some(format!(
                    "Image not sent: {}",
                    herdr_client::Error::ClipboardImageBusy
                ))
            );
            view.endpoints[1]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&snapshot().boot_id, false)
                .unwrap();
        });
        cx.run_until_parked();
        if !reset_all {
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![ClientPaneInputEvent::Paste("paste 3".into())],
                }
            );
        }
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellFocus { focused: false }
        );
        wait_image_finished(&view, cx);
    }
}

#[gpui_kit::test]
fn connected_image_paste_queued_upload_remains_cancellable_after_preparation(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for change in ["epoch", "popup", "local"] {
        let (endpoint, mut server) = connected_endpoint("ssh:image");
        let requests = view.update(cx, |view, cx| {
            prepare_remote_image(view, endpoint);
            if change == "popup" {
                image_popup(view, "original-popup");
            }
            let handle = view.endpoints[1].connection.handle.as_ref().unwrap();
            // Images bypass the API lease, so a second API request must block the FIFO.
            let requests = [0, 1].map(|_| {
                handle
                    .request(
                        &snapshot().boot_id,
                        Method::TabCreate,
                        serde_json::json!({}),
                    )
                    .unwrap()
            });
            assert!(view.paste_terminal_clipboard(clipboard_image(&[42]), false, cx));
            view.endpoints[1]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&snapshot().boot_id, false)
                .unwrap();
            requests
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing first API request");
        };
        let first: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(first["id"], requests[0]);
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            view.cancel_stale_image();
            assert!(
                !view.pending_images.is_empty(),
                "publication must retain cancellation"
            );
            assert!(view.local_error.is_none());
            // No preparation is running now, but the unsent frame still occupies the guard.
            assert!(view.paste_terminal_clipboard(clipboard_image(&[99]), false, cx));
            assert_eq!(
                view.local_error,
                Some(format!(
                    "Image not sent: {}",
                    herdr_client::Error::ClipboardImageBusy
                ))
            );
            match change {
                "epoch" => view.selection_epoch += 1,
                "popup" => image_popup(view, "replacement-popup"),
                "local" => {
                    view.endpoints[1].connection.target =
                        ConnectTarget::Socket(server.path.clone());
                }
                _ => unreachable!(),
            }
            view.cancel_stale_image();
            assert!(
                !view.pending_images.is_empty(),
                "cancelling does not finish the worker slot"
            );
        });
        server.respond(&first);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing second API request");
        };
        let second: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(second["id"], requests[1]);
        server.respond(&second);
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellFocus { focused: false },
            "queued image escaped after {change} changed"
        );
        wait_image_finished(&view, cx);
    }
}

#[gpui_kit::test]
fn connected_image_paste_snapshot_surface_gap_preserves_existing_target(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for removed in [false, true] {
        let (endpoint, mut server) = connected_endpoint("ssh:image");
        let requests = view.update(cx, |view, cx| {
            prepare_remote_image(view, endpoint);
            let handle = view.endpoints[1].connection.handle.as_ref().unwrap();
            let requests = [0, 1].map(|_| {
                handle
                    .request(
                        &snapshot().boot_id,
                        Method::TabCreate,
                        serde_json::json!({}),
                    )
                    .unwrap()
            });
            assert!(view.paste_terminal_clipboard(clipboard_image(&[42]), false, cx));
            view.send(
                ClientPaneInputEvent::TextCommit("after surface gap".into()),
                cx,
            );
            // A newer snapshot invalidates old cells before the replacement surface arrives.
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.revision += 1;
            if removed {
                snapshot.panes.retain(|pane| pane.pane_id != "w1:p1");
                snapshot.focused_pane_id = Some("w1:p2".into());
            }
            view.live.surface = None;
            assert!(!view.input_ready());
            view.cancel_stale_image();
            assert_eq!(view.pending_images.len(), 1);
            requests
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing first API request");
        };
        let first: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(first["id"], requests[0]);
        cx.run_until_parked();
        view.update(cx, |view, _| {
            view.cancel_stale_image();
            if !removed {
                assert!(
                    !view.pending_images.is_empty(),
                    "missing cells must not cancel an existing pane"
                );
            }
            assert!(view.local_error.is_none());
        });
        server.respond(&first);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing second API request");
        };
        let second: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(second["id"], requests[1]);
        server.respond(&second);
        if !removed {
            assert_eq!(
                server.receive(),
                ClientMessage::ClipboardImage {
                    target: ClientClipboardImageTarget::Pane("w1:p1".into()),
                    extension: "png".into(),
                    data: vec![42],
                }
            );
        }
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![ClientPaneInputEvent::TextCommit("after surface gap".into())],
            },
            "snapshot removed pane={removed}"
        );
        wait_image_finished(&view, cx);
        view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
    }
}

#[gpui_kit::test]
fn connected_mouse_focused_pane_preserves_drag_target_and_immediate_text(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for (button, wire_button) in [
        (MouseButton::Left, ClientMouseButton::Left),
        (MouseButton::Middle, ClientMouseButton::Middle),
        (MouseButton::Right, ClientMouseButton::Right),
    ] {
        let (endpoint, mut server) = connected_endpoint("ssh:mouse");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint);
                assert!(view.terminal_mouse_down(
                    &MouseDownEvent {
                        position: mouse_position(view, 3.5, 4.5),
                        button,
                        ..Default::default()
                    },
                    window,
                    cx
                ));
                assert!(view.input_ready());
                assert!(view.focus.is_focused(window));
                // Crossing another pane and leaving the canvas must stay on the pressed pane.
                assert!(view.terminal_mouse_move(
                    &MouseMoveEvent {
                        position: mouse_position(view, 45.5, 6.5),
                        pressed_button: Some(button),
                        ..Default::default()
                    },
                    cx
                ));
                assert!(view.terminal_mouse_up(
                    &MouseUpEvent {
                        position: mouse_position(view, 90., 30.),
                        button,
                        ..Default::default()
                    },
                    cx
                ));
                assert!(view.terminal_mouse.is_none());
                assert!(view.input_ready());
                assert!(view.live.activation.is_none());
                assert!(view.activation_deadline.is_none());
                view.send(ClientPaneInputEvent::TextCommit("immediate".into()), cx);
            });
        });
        for event in [
            mouse_event(ClientMouseKind::Down(wire_button), 2, 3),
            mouse_event(ClientMouseKind::Drag(wire_button), 37, 5),
            mouse_event(ClientMouseKind::Up(wire_button), 37, 21),
            ClientPaneInputEvent::TextCommit("immediate".into()),
        ] {
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![event],
                }
            );
        }
    }
}

#[gpui_kit::test]
fn connected_mouse_inactive_pane_receives_first_click_before_focus_fence(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:mouse");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint);
            let position = mouse_position(view, 43.5, 4.5);
            assert!(view.terminal_mouse_down(
                &MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    ..Default::default()
                },
                window,
                cx
            ));
            assert!(view.input_ready());
            assert!(view.live.activation.is_none());
            assert!(view.mouse_focus_pending());
            view.send(
                ClientPaneInputEvent::TextCommit("must not reach old pane during press".into()),
                cx,
            );
            assert!(view.terminal_mouse_up(
                &MouseUpEvent {
                    position,
                    button: MouseButton::Left,
                    ..Default::default()
                },
                cx
            ));
            assert!(!view.input_ready());
            assert!(!view.mouse_focus_pending());
            assert!(view.live.activation.is_some());
            assert!(view.activation_deadline.is_some());
            view.send(
                ClientPaneInputEvent::TextCommit("must stay fenced".into()),
                cx,
            );
            view.endpoints[1]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&snapshot().boot_id, false)
                .unwrap();
        });
    });
    for kind in [
        ClientMouseKind::Down(ClientMouseButton::Left),
        ClientMouseKind::Up(ClientMouseButton::Left),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p2".into(),
                events: vec![mouse_event(kind, 2, 3)],
            }
        );
    }
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing focus after the complete first click");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "pane.focus");
    assert_eq!(request["params"], serde_json::json!({"pane_id": "w1:p2"}));
    server.respond(&request);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing ordered surface fence");
    };
    let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
    assert_eq!(barrier["params"]["active"], true);
    // The FIFO sentinel catches text incorrectly sent to the previously focused pane.
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    );
}

#[gpui_kit::test]
fn connected_mouse_popup_uses_popup_relative_pixel_coordinates_and_blocks_covered_panes(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:mouse");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint);
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.popup = Some(Box::new(ClientShellPopupSurface {
                terminal_id: "popup-mouse".into(),
                title: String::new(),
                width: None,
                height: None,
                frame: FrameData {
                    width: 20,
                    height: 10,
                    ..surface.frame.clone()
                },
                mouse_reporting: true,
                sgr_pixel_mouse: true,
                pixel_width: 400,
                pixel_height: 400,
            }));
            let outside = mouse_position(view, 3.5, 4.5);
            assert!(!view.terminal_mouse_down(
                &MouseDownEvent {
                    position: outside,
                    button: MouseButton::Left,
                    ..Default::default()
                },
                window,
                cx
            ));
            assert!(!view.terminal_mouse_up(
                &MouseUpEvent {
                    position: outside,
                    button: MouseButton::Left,
                    ..Default::default()
                },
                cx
            ));
            let modifiers = gpui_kit::Modifiers {
                control: true,
                alt: true,
                platform: true,
                ..Default::default()
            };
            // A 20x10 popup in an 80x24 surface starts at column 30, row 7.
            assert!(view.terminal_mouse_down(
                &MouseDownEvent {
                    position: mouse_position(view, 32.5, 10.5),
                    button: MouseButton::Left,
                    modifiers,
                    ..Default::default()
                },
                window,
                cx
            ));
            assert!(view.terminal_mouse_move(
                &MouseMoveEvent {
                    position: mouse_position(view, 34.5, 12.5),
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                },
                cx
            ));
            assert!(view.terminal_mouse_up(
                &MouseUpEvent {
                    position: mouse_position(view, 35.5, 13.5),
                    button: MouseButton::Left,
                    modifiers,
                    ..Default::default()
                },
                cx
            ));
            assert!(view.input_ready());
            assert!(view.live.activation.is_none());
            assert!(view.activation_deadline.is_none());
            view.send(ClientPaneInputEvent::TextCommit("popup text".into()), cx);
        });
    });
    for (kind, column, row, x, y) in [
        (
            ClientMouseKind::Down(ClientMouseButton::Left),
            2,
            3,
            50,
            140,
        ),
        (
            ClientMouseKind::Drag(ClientMouseButton::Left),
            4,
            5,
            90,
            220,
        ),
        (ClientMouseKind::Up(ClientMouseButton::Left), 5, 6, 110, 260),
    ] {
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPopupInput {
                terminal_id: "popup-mouse".into(),
                events: vec![ClientPaneInputEvent::Mouse {
                    kind,
                    position: ClientMousePosition::Pixels { x, y, column, row },
                    geometry: Some(ClientMouseGeometry {
                        cols: 20,
                        rows: 10,
                        width_px: 400,
                        height_px: 400
                    }),
                    modifiers: 14,
                    lines: 1,
                }],
            }
        );
    }
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPopupInput {
            terminal_id: "popup-mouse".into(),
            events: vec![ClientPaneInputEvent::TextCommit("popup text".into())],
        }
    );
}

#[gpui_kit::test]
fn connected_mouse_cancels_stale_gestures_before_drag_or_release(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for change in [
        "epoch",
        "generation",
        "boot",
        "menu",
        "geometry",
        "reporting",
    ] {
        for move_first in [false, true] {
            let (endpoint, mut server) = connected_endpoint("ssh:mouse");
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    prepare_mouse(view, endpoint);
                    let position = mouse_position(view, 3.5, 4.5);
                    assert!(view.terminal_mouse_down(
                        &MouseDownEvent {
                            position,
                            button: MouseButton::Left,
                            ..Default::default()
                        },
                        window,
                        cx
                    ));
                    assert!(view.terminal_mouse_move(
                        &MouseMoveEvent {
                            position: mouse_position(view, 5.5, 6.5),
                            pressed_button: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        cx
                    ));
                    match change {
                        "epoch" => view.selection_epoch += 1,
                        "generation" => view.selected_generation += 1,
                        "boot" => {
                            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).boot_id =
                                "replacement".into();
                            Arc::make_mut(view.live.surface.as_mut().unwrap()).boot_id =
                                "replacement".into();
                        }
                        "menu" => view.open_keybinds(window, cx),
                        "geometry" => {
                            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0]
                                .inner_rect
                                .width -= 1
                        }
                        "reporting" => {
                            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0]
                                .mouse_reporting = false
                        }
                        _ => unreachable!(),
                    }
                    assert!(
                        view.input_ready(),
                        "isolate gesture cancellation from input readiness"
                    );
                    if move_first {
                        view.terminal_mouse_move(
                            &MouseMoveEvent {
                                position,
                                pressed_button: Some(MouseButton::Left),
                                ..Default::default()
                            },
                            cx,
                        );
                        assert!(view.terminal_mouse.is_none(), "{change}");
                    }
                    view.terminal_mouse_up(
                        &MouseUpEvent {
                            position,
                            button: MouseButton::Left,
                            ..Default::default()
                        },
                        cx,
                    );
                    assert!(view.terminal_mouse.is_none(), "{change}");
                    view.cancel_terminal_mouse(cx);
                    view.endpoints[1]
                        .connection
                        .handle
                        .as_ref()
                        .unwrap()
                        .set_focus(&snapshot().boot_id, false)
                        .unwrap();
                });
            });
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![mouse_event(
                        ClientMouseKind::Down(ClientMouseButton::Left),
                        2,
                        3
                    )],
                }
            );
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![mouse_event(
                        ClientMouseKind::Drag(ClientMouseButton::Left),
                        4,
                        5
                    )],
                }
            );
            if matches!(change, "menu" | "geometry" | "reporting") {
                // Cleanup uses the last sent drag, not the rejected move/release position.
                assert_eq!(
                    server.receive(),
                    ClientMessage::ClientShellPaneInput {
                        pane_id: "w1:p1".into(),
                        events: vec![mouse_event(
                            ClientMouseKind::Up(ClientMouseButton::Left),
                            4,
                            5
                        )],
                    }
                );
            }
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellFocus { focused: false },
                "{change}, move_first={move_first}"
            );
        }
    }
}

#[gpui_kit::test]
fn connected_mouse_external_drag_cleans_up_once_without_forwarding_synthetic_input(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for pressed in [false, true] {
        for move_first in [false, true] {
            let (endpoint, mut server) = connected_endpoint("ssh:mouse");
            let position = cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    prepare_mouse(view, endpoint);
                    let position = mouse_position(view, 3.5, 4.5);
                    if pressed {
                        assert!(view.terminal_mouse_down(
                            &MouseDownEvent {
                                position,
                                button: MouseButton::Left,
                                ..Default::default()
                            },
                            window,
                            cx
                        ));
                    }
                    mouse_position(view, 7.5, 8.5)
                })
            });
            // Use GPUI's real external-drag state; the nonrendering Fixture keeps resize out.
            cx.simulate_event(gpui_kit::FileDropEvent::Entered {
                position,
                paths: gpui_kit::ExternalPaths::default(),
            });
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    assert!(cx.has_active_drag());
                    if move_first {
                        assert!(!view.terminal_mouse_move(
                            &MouseMoveEvent {
                                position,
                                pressed_button: Some(MouseButton::Left),
                                ..Default::default()
                            },
                            cx
                        ));
                    }
                    assert!(!view.terminal_mouse_up(
                        &MouseUpEvent {
                            position,
                            button: MouseButton::Left,
                            ..Default::default()
                        },
                        cx
                    ));
                    assert!(view.terminal_mouse.is_none());
                    assert!(view.terminal_mouse_down(
                        &MouseDownEvent {
                            position,
                            button: MouseButton::Left,
                            ..Default::default()
                        },
                        window,
                        cx
                    ));
                    assert!(view.terminal_mouse.is_none());
                    view.terminal_mouse_hover(
                        &MouseMoveEvent {
                            position,
                            ..Default::default()
                        },
                        cx,
                    );
                    view.cancel_terminal_mouse(cx);
                    view.endpoints[1]
                        .connection
                        .handle
                        .as_ref()
                        .unwrap()
                        .set_focus(&snapshot().boot_id, false)
                        .unwrap();
                });
            });
            cx.simulate_event(gpui_kit::FileDropEvent::Exited);
            if pressed {
                assert_eq!(
                    server.receive(),
                    ClientMessage::ClientShellPaneInput {
                        pane_id: "w1:p1".into(),
                        events: vec![mouse_event(
                            ClientMouseKind::Down(ClientMouseButton::Left),
                            2,
                            3
                        )],
                    }
                );
                assert_eq!(
                    server.receive(),
                    ClientMessage::ClientShellPaneInput {
                        pane_id: "w1:p1".into(),
                        events: vec![mouse_event(
                            ClientMouseKind::Up(ClientMouseButton::Left),
                            2,
                            3
                        )],
                    }
                );
            }
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellFocus { focused: false }
            );
        }
    }
}

#[gpui_kit::test]
fn connected_mouse_deactivation_releases_last_sent_position_once_without_focusing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for (offset, pane_id) in [(0., "w1:p1"), (40., "w1:p2")] {
        let (endpoint, mut server) = connected_endpoint("ssh:mouse");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                prepare_mouse(view, endpoint);
                view.active = true;
                assert!(view.terminal_mouse_down(
                    &MouseDownEvent {
                        position: mouse_position(view, offset + 3.5, 4.5),
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx
                ));
                assert!(view.terminal_mouse_move(
                    &MouseMoveEvent {
                        position: mouse_position(view, offset + 5.5, 6.5),
                        pressed_button: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    cx
                ));
                view.active = false;
                view.cancel_terminal_mouse(cx);
                view.cancel_terminal_mouse(cx);
                assert!(view.terminal_mouse.is_none());
                assert!(!view.mouse_focus_pending());
                assert!(!view.terminal_mouse_up(
                    &MouseUpEvent {
                        position: mouse_position(view, offset + 7.5, 8.5),
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    cx
                ));
                assert!(view.live.activation.is_none());
                assert!(view.activation_deadline.is_none());
                view.active = true;
                view.send(
                    ClientPaneInputEvent::TextCommit("after cancellation".into()),
                    cx,
                );
            });
        });
        for event in [
            mouse_event(ClientMouseKind::Down(ClientMouseButton::Left), 2, 3),
            mouse_event(ClientMouseKind::Drag(ClientMouseButton::Left), 4, 5),
            mouse_event(ClientMouseKind::Up(ClientMouseButton::Left), 4, 5),
        ] {
            assert_eq!(
                server.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: pane_id.into(),
                    events: vec![event],
                }
            );
        }
        assert_eq!(
            server.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w1:p1".into(),
                events: vec![ClientPaneInputEvent::TextCommit(
                    "after cancellation".into()
                )],
            }
        );
    }
}

#[gpui_kit::test]
fn connected_mouse_hover_is_separate_from_capture_and_obeys_input_guards(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:mouse");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint);
            let event = MouseMoveEvent {
                position: mouse_position(view, 43.5, 4.5),
                ..Default::default()
            };
            assert!(!view.terminal_mouse_move(&event, cx));
            view.terminal_mouse_hover(&event, cx);
            view.terminal_mouse_hover(
                &MouseMoveEvent {
                    pressed_button: Some(MouseButton::Left),
                    ..event.clone()
                },
                cx,
            );
            view.terminal_mouse_hover(
                &MouseMoveEvent {
                    modifiers: gpui_kit::Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    ..event.clone()
                },
                cx,
            );
            view.terminal_mouse_hover(
                &MouseMoveEvent {
                    position: mouse_position(view, 90., 30.),
                    ..event.clone()
                },
                cx,
            );
            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[1].mouse_reporting = false;
            view.terminal_mouse_hover(&event, cx);
            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[1].mouse_reporting = true;
            view.open_keybinds(window, cx);
            view.terminal_mouse_hover(&event, cx);
            view.menu.reset();
            assert!(view.terminal_mouse.is_none());
            assert!(view.live.activation.is_none());
            assert!(view.activation_deadline.is_none());
            view.send(
                ClientPaneInputEvent::TextCommit("hover does not focus".into()),
                cx,
            );
        });
    });
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p2".into(),
            events: vec![mouse_event(ClientMouseKind::Moved, 2, 3)],
        }
    );
    assert_eq!(
        server.receive(),
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![ClientPaneInputEvent::TextCommit(
                "hover does not focus".into()
            )],
        }
    );
}

#[gpui_kit::test]
fn startup_focus_waits_for_the_first_surface_without_flapping_on_later_updates(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, mut server) = connected_endpoint(LOCAL);
    endpoint.connection.inbox.lock().unwrap().surface = None;
    endpoint.live.surface = None;
    let inbox = endpoint.connection.inbox.clone();
    view.update(cx, |view, _| {
        view.endpoints = vec![endpoint];
        view.options = ConnectOptions::default();
        view.reset_selected();
        view.active = true;
        assert!(!view.input_ready());
        view.report_focus();
        assert_eq!(view.sent_focus, Some(false));
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    ));

    inbox
        .lock()
        .unwrap()
        .apply(ClientEvent::Surface(surface(&snapshot())));
    view.update(cx, |view, cx| {
        project_until(view, cx, "startup surface ready", HerdrWindow::input_ready);
        view.report_focus();
        assert_eq!(view.sent_focus, Some(true));
        // New snapshot/surface pairs can arrive separately during normal activity.
        view.live.surface = None;
        view.report_focus();
        assert_eq!(view.sent_focus, Some(true));
        view.active = false;
        view.report_focus();
        assert_eq!(view.sent_focus, Some(false));
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: true }
    ));
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    ));
}

#[gpui_kit::test]
fn workspace_menu_keeps_immediate_and_deferred_navigation(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for deferred in [false, true] {
        let (endpoint, mut server) = connected_endpoint("local");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.selected_endpoint = 0;
                view.endpoints = vec![endpoint];
                view.options = ConnectOptions::default();
                view.reset_selected();
                if deferred {
                    view.live.surface = None;
                }
                assert!(view.navigate_endpoint("local", NavigationTarget::Workspace("w1"), cx));
                view.open_workspace_menu("w1", Default::default(), window, cx);
                assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
                if deferred {
                    assert!(view.pending_navigation.is_some());
                    view.live = view.endpoints[0].live.clone();
                    view.poll_endpoints(cx);
                }
                assert!(view.pending_navigation.is_none());
                assert!(!view.input_ready());
                assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
                assert!(!view.focus.is_focused(window));
                // New input is still blocked while the menu owns focus.
                assert!(!view.navigate(NavigationTarget::Workspace("other"), cx));
            });
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing workspace focus request");
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], "workspace.focus");
        assert_eq!(request["params"], serde_json::json!({"workspace_id": "w1"}));
        server.respond(&request);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing surface barrier");
        };
        let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
        view.update(cx, |view, cx| {
            let mut next = snapshot();
            next.revision += 1;
            next.focused_workspace_id = Some("w1".into());
            for workspace in &mut next.workspaces {
                workspace.focused = workspace.workspace_id == "w1";
            }
            {
                let mut state = view.endpoints[0].connection.inbox.lock().unwrap();
                state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
                state.apply(ClientEvent::Surface(surface(&next)));
                state.apply(ClientEvent::Response {
                    request_id: barrier["id"].as_str().unwrap().into(),
                    response: serde_json::json!({"result": {
                        "type": "client_shell_surface_set", "active": true,
                        "projection_revision": next.revision
                    }}),
                });
            }
            project_until(view, cx, "workspace selection", HerdrWindow::input_ready);
            assert_eq!(view.menu.page, Some(crate::menu::Page::Workspace));
            let selected = view.live.snapshot.as_ref().unwrap();
            assert_eq!(selected.focused_workspace_id.as_deref(), Some("w1"));
            assert!(
                selected
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.workspace_id == "w1" && workspace.focused)
            );
        });
    }
}

#[gpui_kit::test]
fn toast_navigation_queues_typed_targets_and_fences_input(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for (tab, pane, method, params) in [
        (
            None,
            None,
            "workspace.focus",
            serde_json::json!({"workspace_id":"w1"}),
        ),
        (
            Some("w1:t1"),
            None,
            "tab.focus",
            serde_json::json!({"tab_id":"w1:t1"}),
        ),
        (
            Some("w1:t1"),
            Some("w1:p1"),
            "pane.focus",
            serde_json::json!({"pane_id":"w1:p1"}),
        ),
    ] {
        let (mut endpoint, mut server) = connected_endpoint("ssh:toast");
        let mut wire = crate::notifications::tests::notification("Navigate");
        wire.workspace_id = Some("w1".into());
        wire.tab_id = tab.map(str::to_owned);
        wire.pane_id = pane.map(str::to_owned);
        endpoint
            .toasts
            .receive([crate::notifications::Notice::new(wire, Instant::now())
                .with_snapshot(endpoint.live.snapshot.as_deref())
                .preview()]);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints.truncate(1);
                view.endpoints.push(endpoint);
                view.selected_endpoint = 1;
                view.options = ConnectOptions::default();
                view.reset_selected();
                window.focus(&view.focus, cx);
                view.marked = "composition".into();
                assert!(view.input_ready());
                view.tick_toasts(false, Instant::now());
                view.command(Command::OpenNotificationTarget, window, cx);
                assert!(view.endpoints[1].toasts.entries.is_empty());
                assert!(view.marked.is_empty());
                assert!(view.focus.is_focused(window));
                assert!(!view.input_ready());
            })
        });
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("expected semantic focus request")
        };
        let request: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], method);
        assert_eq!(request["params"], params);
    }
}

#[gpui_kit::test]
fn wire_completion_waits_for_evidence_then_command_uses_original_pane(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, mut server) = connected_endpoint("ssh:completion");
    let mut projection = snapshot();
    projection.revision += 1;
    projection.agents[0].agent_status = AgentStatus::Working;
    write_message(
        &mut server.stream,
        &ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: serde_json::to_string(&projection).unwrap(),
        },
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    let mut event = crate::notifications::tests::notification("wire completion");
    event.kind = SemanticNotificationKind::Finished;
    event.workspace_id = None;
    event.pane_id = Some("w1:p1".into());
    write_message(
        &mut server.stream,
        &ServerMessage::SemanticNotification(event),
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    wait_until(|| {
        endpoint.poll(Instant::now());
        !endpoint.toasts.entries.is_empty()
    });
    let now = Instant::now();
    view.update(cx, |view, _| {
        view.endpoints.push(endpoint);
        view.config.notifications.enabled = true;
        view.config.notifications.delay_seconds = 0;
        view.tick_toasts(false, now);
        assert!(!view.endpoints[1].toasts.entries[0].1.visible);
    });
    projection.revision += 1;
    projection.agents[0].agent_status = AgentStatus::Done;
    write_message(
        &mut server.stream,
        &ServerMessage::EndpointControl {
            kind: ENDPOINT_SNAPSHOT_KIND.into(),
            data: serde_json::to_string(&projection).unwrap(),
        },
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            wait_until(|| {
                view.endpoints[1].poll(Instant::now());
                view.endpoints[1]
                    .live
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.revision == projection.revision)
            });
            view.tick_toasts(false, now + Duration::from_millis(50));
            assert!(view.endpoints[1].toasts.entries[0].1.visible);
            view.endpoints[1]
                .connection
                .inbox
                .lock()
                .unwrap()
                .apply(ClientEvent::Surface(surface(&projection)));
            wait_until(|| {
                view.endpoints[1].poll(Instant::now());
                view.endpoints[1].live.surface.is_some()
            });
            // Select the already-active fixture surface without issuing unrelated activation requests.
            view.selected_endpoint = 1;
            view.reset_selected();
            view.command(Command::OpenNotificationTarget, window, cx);
            assert!(view.endpoints[1].toasts.entries.is_empty());
            assert!(!view.input_ready());
        })
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("expected completion target focus");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "pane.focus");
    assert_eq!(request["params"]["pane_id"], "w1:p1");
}

#[gpui_kit::test]
fn toast_click_uses_origin_and_close_never_navigates(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) =
        crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
    let (mut remote, _server) = connected_endpoint("ssh:toast");
    remote.initial_surface = false;
    let mut wire = crate::notifications::tests::notification("Remote target");
    wire.workspace_id = Some("w1".into());
    let notice = crate::notifications::Notice::new(wire, Instant::now())
        .with_snapshot(remote.live.snapshot.as_deref())
        .preview();
    remote.toasts.receive([notice.clone(), notice]);
    cx.simulate_resize(size(px(1000.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // The same IDs on Local must not win over the notification's origin.
            view.endpoints[0].live.snapshot = remote.live.snapshot.clone();
            view.endpoints[0].detached = true;
            view.endpoints.push(remote);
            window.focus(&view.focus, cx);
            view.marked = "composition".into();
        });
        window.draw(cx).clear(cx);
    });
    // Closing the kit card is what its close button does.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let card = view.toast_cards[0].clone();
            view.toast_closed(&card, cx);
        })
    });
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert_eq!(view.selected_endpoint, 0);
        assert!(view.pending_navigation.is_none());
        assert_eq!(view.marked, "composition");
        assert!(view.focus.is_focused(window));
        assert_eq!(view.endpoints[1].toasts.entries.len(), 1);
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let card = cx.debug_bounds("toast-ssh:toast-1").unwrap();
    cx.simulate_click(card.center(), Default::default());
    view.update(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(view.pending_toast, Some(1));
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Workspace("w1".into()))
        );
        assert_eq!(view.endpoints[1].toasts.entries.len(), 1);
        // A newer inbox boot is rejected even before it reaches the UI projection.
        let inbox = view.endpoints[1].connection.inbox.clone();
        {
            let mut state = inbox.lock().unwrap();
            Arc::make_mut(state.snapshot.as_mut().unwrap()).boot_id = "replacement".into();
        }
        assert_eq!(view.toast_target(1, 1), std::task::Poll::Ready(None));
        view.endpoints[1].stop();
        assert_eq!(view.toast_target(1, 1), std::task::Poll::Ready(None));
    });
}

#[gpui_kit::test]
fn toast_rendered_clicks_reject_replaced_removed_and_disabled_origins(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (view, cx) =
        crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    for change in 0..4 {
        let (mut remote, _server) = connected_endpoint("ssh:toast");
        let mut wire = crate::notifications::tests::notification("old render");
        wire.workspace_id = Some("w1".into());
        remote
            .toasts
            .receive([crate::notifications::Notice::new(wire, Instant::now())
                .with_snapshot(remote.live.snapshot.as_deref())
                .preview()]);
        cx.update(|window, cx| {
            view.update(cx, |view, _| {
                view.endpoints.truncate(1);
                view.endpoints.push(remote);
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("toast-ssh:toast-0").is_some());
        let (generation, inbox) = view.read_with(cx, |view, _| {
            (
                view.endpoints[1].generation,
                view.endpoints[1].connection.inbox.clone(),
            )
        });
        view.update(cx, |view, _| match change {
            0 => view.endpoints[1].generation += 1,
            1 => {
                view.endpoints[1].connection.inbox =
                    Arc::new(Mutex::new(view.endpoints[1].live.clone()))
            }
            2 => {
                view.endpoints.pop();
            }
            _ => view.endpoints[1].enabled = false,
        });
        // Invoke the captured callback identity directly: simulate_click redraws
        // first and would correctly capture the replacement generation instead.
        view.update(cx, |view, cx| {
            view.click_toast("ssh:toast", generation, &inbox, 0, cx)
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.selected_endpoint, 0);
            assert!(view.pending_navigation.is_none());
            assert!(view.pending_toast.is_none());
        });
    }
}

#[gpui_kit::test]
fn newer_same_endpoint_navigation_cannot_replay_a_pending_toast(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, mut server) = connected_endpoint("ssh:toast");
    let inbox = endpoint.connection.inbox.clone();
    let mut wire = crate::notifications::tests::notification("older intent");
    wire.workspace_id = Some("w1".into());
    wire.pane_id = Some("w1:p1".into());
    endpoint
        .toasts
        .receive([crate::notifications::Notice::new(wire, Instant::now())
            .with_snapshot(endpoint.live.snapshot.as_deref())
            .preview()]);
    view.update(cx, |view, cx| {
        view.endpoints[0].detached = true;
        view.endpoints.push(endpoint);
        view.selected_endpoint = 1;
        view.options = ConnectOptions::default();
        view.reset_selected();
        view.tick_toasts(false, Instant::now());
        let held = inbox.lock().unwrap();
        view.click_toast("ssh:toast", view.endpoints[1].generation, &inbox, 0, cx);
        assert_eq!(view.pending_toast, Some(0));
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Pane("w1:p1".into()))
        );
        drop(held);
        assert!(view.navigation_ready());
        view.navigate_endpoint("ssh:toast", NavigationTarget::Pane("new-pane"), cx);
        assert!(view.pending_toast.is_none());
        assert!(view.pending_navigation.is_none());
        assert!(!view.input_ready());
    });
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing newer navigation");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "pane.focus");
    assert_eq!(request["params"]["pane_id"], "new-pane");
    server.respond(&request);
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing newer navigation barrier");
    };
    let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
    let mut next = snapshot();
    next.revision += 1;
    let mut pane = next.panes[0].clone();
    pane.pane_id = "new-pane".into();
    next.panes[0].focused = false;
    next.panes.push(pane);
    next.focused_pane_id = Some("new-pane".into());
    write_message(&mut server.stream, &ServerMessage::ClientShellEndpointResponseChunk {
        boot_id: next.boot_id.clone(),
        request_id: barrier["id"].as_str().unwrap().into(),
        final_chunk: true,
        data: serde_json::to_vec(&serde_json::json!({"id": barrier["id"], "result": {
            "type": "client_shell_surface_set", "active": true, "projection_revision": next.revision
        }})).unwrap(),
    }, MAX_GRAPHICS_FRAME_SIZE).unwrap();
    wait_until(|| {
        inbox
            .lock()
            .unwrap()
            .activation
            .as_ref()
            .and_then(|a| a.revision)
            == Some(next.revision)
    });
    {
        let mut state = inbox.lock().unwrap();
        state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
        state.apply(ClientEvent::Surface(surface(&next)));
    }
    view.update(cx, |view, cx| {
        project_until(view, cx, "newer navigation completed", |view| {
            view.live
                .snapshot
                .as_ref()
                .is_some_and(|s| s.revision == next.revision)
        });
        view.poll_endpoints(cx);
        assert!(view.pending_navigation.is_none());
        assert!(view.input_ready());
        assert_eq!(
            view.live
                .snapshot
                .as_ref()
                .unwrap()
                .focused_pane_id
                .as_deref(),
            Some("new-pane")
        );
        // FIFO input proves no old pane.focus was queued after the completed barrier.
        view.send(
            ClientPaneInputEvent::TextCommit("new intent only".into()),
            cx,
        );
    });
    let ClientMessage::ClientShellPaneInput { pane_id, .. } = server.receive() else {
        panic!("old navigation replayed instead of input to the newer target");
    };
    assert_eq!(pane_id, "new-pane");
}

#[gpui_kit::test]
fn toast_handoff_retains_busy_validation_and_revalidates_before_queueing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for (deleted, busy_click, already_active) in [
        (false, false, false),
        (false, true, false),
        (true, false, false),
        (true, true, false),
        (false, true, true),
        (true, true, true),
    ] {
        let (mut endpoint, mut server) = connected_endpoint("ssh:toast");
        endpoint.initial_surface = already_active;
        let mut wire = crate::notifications::tests::notification("handoff");
        wire.workspace_id = Some("w1".into());
        endpoint
            .toasts
            .receive([crate::notifications::Notice::new(wire, Instant::now())
                .with_snapshot(endpoint.live.snapshot.as_deref())
                .preview()]);
        view.update(cx, |view, cx| {
            view.selected_endpoint = 0;
            view.endpoints.truncate(1);
            view.endpoints[0].detached = true;
            view.endpoints.push(endpoint);
            view.options = ConnectOptions::default();
            if already_active {
                view.selected_endpoint = 1;
                view.reset_selected();
                assert!(view.input_ready());
            }
            let inbox = view.endpoints[1].connection.inbox.clone();
            view.tick_toasts(false, Instant::now());
            let held = busy_click.then(|| inbox.lock().unwrap());
            view.click_toast("ssh:toast", view.endpoints[1].generation, &inbox, 0, cx);
            drop(held);
            assert_eq!(view.pending_toast, Some(0));
            assert!(!view.input_ready());
            assert_eq!(view.endpoints[1].toasts.entries.len(), 1);
            // Complete the handoff with a coherent current projection.
            {
                let mut state = inbox.lock().unwrap();
                if deleted {
                    Arc::make_mut(state.snapshot.as_mut().unwrap())
                        .workspaces
                        .clear();
                }
                state.surface = Some(surface(state.snapshot.as_ref().unwrap()));
                state.activation = None;
                state.dirty = true;
                view.endpoints[1].initial_surface = true;
                // Project the completed handoff, then deterministically hold
                // the inbox across polling and attempted terminal input.
                view.endpoints[1].live = state.clone();
                view.live = state.clone();
                for _ in 0..2 {
                    view.poll_endpoints(cx);
                    assert!(view.navigation_ready());
                    assert!(!view.input_ready());
                    assert_eq!(view.pending_toast, Some(0));
                    assert_eq!(
                        view.pending_navigation,
                        Some(NavigationTarget::Workspace("w1".into()))
                    );
                    assert_eq!(view.endpoints[1].toasts.entries.len(), 1);
                    view.send(
                        ClientPaneInputEvent::TextCommit("must stay fenced".into()),
                        cx,
                    );
                }
            }
            project_until(view, cx, "toast handoff", |view| {
                view.pending_toast.is_none()
            });
            assert_eq!(view.endpoints[1].toasts.entries.len(), usize::from(deleted));
            assert!(view.pending_navigation.is_none());
            assert_eq!(view.input_ready(), deleted);
            if deleted {
                view.endpoints[1]
                    .connection
                    .handle
                    .as_ref()
                    .unwrap()
                    .set_focus(&snapshot().boot_id, false)
                    .unwrap();
            }
        });
        if !deleted {
            let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
                panic!("expected focus after handoff")
            };
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["method"], "workspace.focus");
        } else {
            assert!(matches!(
                server.receive(),
                ClientMessage::ClientShellFocus { focused: false }
            ));
        }
    }
}

#[gpui_kit::test]
fn accepted_toast_survives_expiry_but_not_invalidation(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for invalidation in [
        "none",
        "dismiss",
        "replace",
        "boot",
        "membership",
        "generation",
        "overflow",
    ] {
        let (mut endpoint, mut server) = connected_endpoint("ssh:toast");
        endpoint.initial_surface = false;
        let mut wire = crate::notifications::tests::notification("accepted");
        wire.workspace_id = Some("w1".into());
        wire.pane_id = Some("w1:p1".into());
        endpoint.toasts.receive([
            crate::notifications::Notice::new(wire.clone(), Instant::now())
                .with_snapshot(endpoint.live.snapshot.as_deref())
                .preview(),
        ]);
        view.update(cx, |view, cx| {
            view.selected_endpoint = 0;
            view.endpoints.truncate(1);
            view.endpoints[0].detached = true;
            view.reset_selected();
            view.endpoints.push(endpoint);
            view.tick_toasts(false, Instant::now());
            let inbox = view.endpoints[1].connection.inbox.clone();
            view.click_toast("ssh:toast", view.endpoints[1].generation, &inbox, 0, cx);
            assert_eq!(view.pending_toast, Some(0));
            assert!(!view.input_ready());
            let after_expiry =
                view.endpoints[1].toasts.entries[0].1.expires + Duration::from_secs(1);
            view.tick_toasts(false, after_expiry);
            assert_eq!(view.endpoints[1].toasts.entries.len(), 1);
            // An accepted intent also remains eligible between timer samples.
            view.endpoints[1].toasts.entries[0].1.expires = Instant::now();
            assert!(matches!(
                view.toast_target(1, 0),
                std::task::Poll::Ready(Some(_))
            ));
            match invalidation {
                "dismiss" => view.endpoints[1].toasts.dismiss(0),
                "replace" => {
                    // Revalidate against an undrained replacement, not just the UI queue.
                    inbox.lock().unwrap().apply(ClientEvent::Message(
                        ServerMessage::SemanticNotification(wire),
                    ));
                }
                "boot" => {
                    Arc::make_mut(inbox.lock().unwrap().snapshot.as_mut().unwrap()).boot_id =
                        "other".into()
                }
                "membership" => Arc::make_mut(inbox.lock().unwrap().snapshot.as_mut().unwrap())
                    .panes
                    .clear(),
                "generation" => view.endpoints[1].stop(),
                "overflow" => {
                    let mut state = inbox.lock().unwrap();
                    state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                        wire,
                    )));
                    for _ in 0..crate::notifications::PENDING_LIMIT {
                        state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                            crate::notifications::tests::notification("other"),
                        )));
                    }
                }
                _ => {}
            }
            if invalidation != "generation" {
                view.endpoints[1].initial_surface = true;
                view.live = inbox.lock().unwrap().clone();
                view.live.surface = Some(surface(view.live.snapshot.as_ref().unwrap()));
                view.live.activation = None;
                view.navigate_toast(0, cx);
                assert!(view.pending_toast.is_none());
                if invalidation == "none" {
                    assert!(view.endpoints[1].toasts.entries.is_empty());
                } else {
                    view.endpoints[1]
                        .connection
                        .handle
                        .as_ref()
                        .unwrap()
                        .set_focus(&snapshot().boot_id, false)
                        .unwrap();
                }
            } else {
                assert_eq!(view.toast_target(1, 0), std::task::Poll::Ready(None));
            }
        });
        if invalidation == "none" {
            let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
                panic!("missing accepted focus")
            };
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&request).unwrap()["method"],
                "pane.focus"
            );
        } else if invalidation != "generation" {
            assert!(
                matches!(
                    server.receive(),
                    ClientMessage::ClientShellFocus { focused: false }
                ),
                "{invalidation}"
            );
        }
    }
}

#[gpui_kit::test]
fn toast_handoff_defers_a_contended_source_without_activating_destination(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (source, mut source_server) = connected_endpoint("ssh:source");
    let (mut target, mut target_server) = connected_endpoint("ssh:target");
    target.initial_surface = false;
    let source_inbox = source.connection.inbox.clone();
    let target_inbox = target.connection.inbox.clone();
    let target_handle = target.connection.handle.clone().unwrap();
    let mut wire = crate::notifications::tests::notification("target");
    wire.workspace_id = Some("w1".into());
    target
        .toasts
        .receive([crate::notifications::Notice::new(wire, Instant::now())
            .with_snapshot(target.live.snapshot.as_deref())
            .preview()]);
    let held = source_inbox.lock().unwrap();
    view.update(cx, |view, cx| {
        view.endpoints[0].detached = true;
        view.endpoints.extend([source, target]);
        view.selected_endpoint = 1;
        view.reset_selected();
        view.tick_toasts(false, Instant::now());
        view.click_toast(
            "ssh:target",
            view.endpoints[2].generation,
            &target_inbox,
            0,
            cx,
        );
        assert_eq!(view.selected_endpoint, 2);
        assert_eq!(view.pending_toast, Some(0));
        for _ in 0..2 {
            view.poll_endpoints(cx);
            assert!(matches!(
                view.pending_releases[0].phase,
                ReleasePhase::Deferred(_)
            ));
            assert!(!view.endpoints[2].initial_surface);
            assert!(!view.input_ready());
        }
    });
    target_handle.set_focus(&snapshot().boot_id, false).unwrap();
    assert!(matches!(
        target_server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    ));
    drop(held);
    view.update(cx, |view, cx| {
        project_until(view, cx, "source release queued", |view| {
            matches!(view.pending_releases[0].phase, ReleasePhase::Sent(_))
        })
    });
    assert!(matches!(
        source_server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    ));
    let ClientMessage::ClientShellEndpointRequest { request, .. } = source_server.receive() else {
        panic!("missing source release")
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::ClientShellSurfaceSet.as_str());
    assert_eq!(request["params"]["active"], false);
    target_handle.set_focus(&snapshot().boot_id, true).unwrap();
    assert!(matches!(
        target_server.receive(),
        ClientMessage::ClientShellFocus { focused: true }
    ));
    source_inbox.lock().unwrap().apply(ClientEvent::Response {
        request_id: request["id"].as_str().unwrap().into(),
        response: serde_json::json!({"result": {"type": "client_shell_surface_set", "active": false, "projection_revision": 7}}),
    });
    view.update(cx, |view, cx| {
        project_until(view, cx, "source release acknowledged", |view| {
            view.endpoints[2].initial_surface
        });
        assert!(view.pending_releases.is_empty());
        assert!(view.endpoints[2].initial_surface);
        assert!(!view.input_ready());
    });
    assert!(matches!(
        target_server.receive(),
        ClientMessage::ClientShellResize { .. }
    ));
    let ClientMessage::ClientShellEndpointRequest { request, .. } = target_server.receive() else {
        panic!("missing destination activation")
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], Method::ClientShellSurfaceSet.as_str());
    assert_eq!(request["params"]["active"], true);
}

#[gpui_kit::test]
fn deferred_release_is_generation_fenced_and_local_can_escape(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for change in ["retire", "boot", "local"] {
        let (source, _source_server) = connected_endpoint("ssh:source");
        let (mut target, _target_server) = connected_endpoint("ssh:target");
        target.initial_surface = false;
        let inbox = source.connection.inbox.clone();
        let drained = source.connection.drained.clone();
        let handle = source.connection.handle.clone().unwrap();
        let mut held = inbox.lock().unwrap();
        view.update(cx, |view, cx| {
            view.pending_releases.clear();
            view.endpoints.truncate(1);
            view.endpoints[0].detached = true;
            view.endpoints.extend([source, target]);
            view.selected_endpoint = 1;
            view.reset_selected();
            assert!(view.select_endpoint("ssh:target", cx));
            assert!(matches!(
                view.pending_releases[0].phase,
                ReleasePhase::Deferred(_)
            ));
            match change {
                "retire" => {
                    view.endpoints[1].stop();
                    view.endpoints[1].detached = true;
                    assert!(!Arc::ptr_eq(&inbox, &view.endpoints[1].connection.inbox));
                    assert!(handle.is_disconnected());
                }
                "boot" => {
                    Arc::make_mut(held.snapshot.as_mut().unwrap()).boot_id = "replacement".into()
                }
                _ => {
                    assert!(view.select_endpoint(LOCAL, cx));
                    assert!(view.pending_releases.is_empty());
                    assert!(handle.is_disconnected());
                }
            }
            assert!(!view.endpoints[2].initial_surface);
        });
        drop(held);
        view.update(cx, |view, cx| {
            if change == "boot" {
                project_until(view, cx, "stale source retired", |_| {
                    handle.is_disconnected()
                });
                view.endpoints[1].detached = true;
            }
        });
        wait_until(|| drained.load(Ordering::Acquire));
        if change != "local" {
            view.update(cx, |view, cx| {
                project_until(view, cx, "retired source drained", |view| {
                    view.endpoints[2].initial_surface
                });
                assert!(view.pending_releases.is_empty());
            });
        }
    }
}

#[gpui_kit::test]
fn toast_queue_failure_retains_notice(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (mut endpoint, _server) = connected_endpoint("ssh:toast");
    let mut wire = crate::notifications::tests::notification("queue failure");
    wire.workspace_id = Some("w1".into());
    endpoint
        .toasts
        .receive([crate::notifications::Notice::new(wire, Instant::now())
            .with_snapshot(endpoint.live.snapshot.as_deref())
            .preview()]);
    view.update(cx, |view, cx| {
        view.endpoints.push(endpoint);
        view.selected_endpoint = 1;
        view.options = ConnectOptions::default();
        view.reset_selected();
        // Keep the projected connected state to exercise enqueue failure itself.
        view.endpoints[1].connection.inbox = Arc::new(Mutex::new(view.live.clone()));
        view.endpoints[1]
            .connection
            .handle
            .as_ref()
            .unwrap()
            .disconnect();
        assert!(view.input_ready());
        view.tick_toasts(false, Instant::now());
        view.navigate_toast(0, cx);
        assert_eq!(view.endpoints[1].toasts.entries.len(), 1);
        assert!(view.local_error.is_some());
    });
}

#[gpui_kit::test]
fn notification_command_rejects_ineligible_cards_without_selection_or_requests(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for case in 0..7 {
        let (mut endpoint, mut server) = connected_endpoint("ssh:toast");
        let mut wire = crate::notifications::tests::notification("ineligible");
        wire.workspace_id = (case != 0).then(|| "w1".into());
        let mut notice = crate::notifications::Notice::new(wire, Instant::now())
            .with_snapshot(endpoint.live.snapshot.as_deref())
            .preview();
        if case != 4 {
            notice.promote(Instant::now());
        }
        if case == 3 {
            notice.expires = Instant::now();
        }
        endpoint.toasts.receive([notice]);
        if case == 1 || case == 2 {
            let mut state = endpoint.connection.inbox.lock().unwrap();
            let snapshot = Arc::make_mut(state.snapshot.as_mut().unwrap());
            if case == 1 {
                snapshot.workspaces.clear();
            } else {
                snapshot.boot_id = "new-boot".into();
            }
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.menu.reset();
                view.endpoints.truncate(1);
                view.endpoints.push(endpoint);
                view.selected_endpoint = 0;
                view.toasts_hidden = case == 5;
                if case == 6 {
                    view.open_keybinds(window, cx);
                }
                view.command(Command::OpenNotificationTarget, window, cx);
                assert_eq!(view.selected_endpoint, 0, "case {case}");
                assert!(view.pending_navigation.is_none());
                assert!(view.pending_toast.is_none());
                assert_eq!(view.endpoints[1].toasts.entries.len(), 1);
                view.endpoints[1]
                    .connection
                    .handle
                    .as_ref()
                    .unwrap()
                    .set_focus(&snapshot().boot_id, false)
                    .unwrap();
            })
        });
        // Ordered sentinel proves that no focus request preceded it.
        assert!(matches!(
            server.receive(),
            ClientMessage::ClientShellFocus { focused: false }
        ));
    }
}

#[gpui_kit::test]
#[cfg(feature = "qa-menu")]
fn qa_play_sound_dispatches_without_daemon_or_pane(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) =
        crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
    let (sound, played) = crate::sound::Service::recording();
    view.update(cx, |view, _| {
        view.sound = sound;
        for endpoint in &mut view.endpoints {
            endpoint.stop();
            endpoint.live = Default::default();
        }
    });
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        focus.focus(window, cx);
        window.draw(cx).clear(cx);
        let menus = crate::menus();
        let qa = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "QA")
            .unwrap();
        let action = qa
            .items
            .iter()
            .find_map(|item| match item {
                gpui_kit::MenuItem::Action { name, action, .. }
                    if name.as_ref() == "Play Sound" =>
                {
                    Some(action)
                }
                _ => None,
            })
            .unwrap();
        assert!(action.partial_eq(&crate::PlaySound));
        window.dispatch_action(action.boxed_clone(), cx);
    });
    assert_eq!(
        played.recv_timeout(Duration::from_secs(3)).unwrap(),
        SemanticNotificationSound::Done
    );
    view.update(cx, |view, _| {
        for endpoint in &view.endpoints {
            assert!(endpoint.live.snapshot.is_none());
            assert!(endpoint.live.sound_events.is_empty());
        }
        view.sound = Default::default();
    });
    assert!(matches!(
        played.recv_timeout(Duration::from_secs(3)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
}

#[gpui_kit::test]
fn inactive_endpoint_semantic_sound_reaches_worker_once(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (remote, mut server) = connected_endpoint("ssh:sound");
    let (sound, played) = crate::sound::Service::recording();
    view.update(cx, |view, _| {
        view.sound = sound;
        view.endpoints[0].detached = true;
        view.endpoints.push(remote);
        assert_eq!(view.selected_endpoint, 0);
    });
    for message in [
        ServerMessage::Notify {
            kind: NotifyKind::Sound,
            message: "legacy".into(),
            body: None,
        },
        ServerMessage::TerminalBell { count: 1 },
        ServerMessage::SemanticNotification(SemanticNotification {
            kind: SemanticNotificationKind::Custom,
            title: "test".into(),
            body: None,
            sound: Some(SemanticNotificationSound::Request),
            agent: None,
            workspace_id: None,
            tab_id: None,
            pane_id: None,
            position: None,
        }),
    ] {
        write_message(&mut server.stream, &message, MAX_FRAME_SIZE).unwrap();
    }
    view.update(cx, |view, cx| {
        wait_until(|| {
            view.poll_endpoints(cx);
            match played.try_recv() {
                Ok(sound) => {
                    assert_eq!(sound, SemanticNotificationSound::Request);
                    true
                }
                Err(_) => false,
            }
        });
        assert!(view.endpoints[1].live.sound_events.is_empty());
        // Subsequent coalesced updates cannot redeliver the moved event.
        view.endpoints[1]
            .connection
            .inbox
            .lock()
            .unwrap()
            .set_outer_focus(true);
        view.poll_endpoints(cx);
        assert!(played.try_recv().is_err());
        view.endpoints[1].stop();
    });
}

#[gpui_kit::test]
fn saved_selection_waits_for_snapshot_without_overwriting_preference(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
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

#[gpui_kit::test]
fn dialog_response_survives_initial_surface_activation(cx: &mut gpui_kit::TestAppContext) {
    let (view, cx) =
        crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
    let (mut endpoint, _server) = connected_endpoint("ssh:fixture");
    endpoint.initial_surface = false;
    let response = serde_json::json!({"result":{"type":"worktree_list","worktrees":[]}});
    {
        let mut state = endpoint.connection.inbox.lock().unwrap();
        state.dialog_response = Some(("lookup".into(), Some(Ok(response.clone()))));
        state.dirty = true;
    }
    view.update(cx, |view, cx| {
        view.endpoints[0].detached = true;
        view.endpoints.push(endpoint);
        view.selected_endpoint = 1;
        view.reset_selected();
        view.poll_endpoints(cx);
        assert!(view.live.activation.is_some());
        assert!(matches!(&view.live.dialog_response, Some((id, Some(Ok(value)))) if id == "lookup" && value == &response));
        assert!(
            view.endpoints[1]
                .connection
                .inbox
                .lock()
                .unwrap()
                .dialog_response
                .as_ref()
                .unwrap()
                .1
                .is_none()
        );
    });
}

#[gpui_kit::test]
fn every_focus_changing_command_fences_immediate_input_until_ack_and_surface(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    for (command, method, confirm_close_tab, explicit_tab) in [
        (Command::SplitRight, Method::PaneSplit),
        (Command::SplitDown, Method::PaneSplit),
        (Command::Tab, Method::TabCreate),
        (Command::Workspace, Method::WorkspaceCreate),
        (Command::NextTab, Method::TabFocus),
        (Command::PreviousTab, Method::TabFocus),
        (Command::TabNumber(1), Method::TabFocus),
        (Command::FocusLeft, Method::PaneFocusDirection),
        (Command::FocusRight, Method::PaneFocusDirection),
        (Command::FocusUp, Method::PaneFocusDirection),
        (Command::FocusDown, Method::PaneFocusDirection),
        (Command::NextPane, Method::PaneFocus),
        (Command::PreviousPane, Method::PaneFocus),
        (Command::Zoom, Method::PaneZoom),
        (Command::ClearPane, Method::PaneClear),
        (Command::ClosePane, Method::PaneClose),
        (Command::CloseTab, Method::TabClose),
        (Command::WorkspacePicker, Method::WorkspaceFocus),
        (Command::Palette, Method::CommandInvoke),
        (Command::Workspace, Method::WorkspaceClose),
        (Command::Workspace, Method::WorktreeCreate),
        (Command::Workspace, Method::WorktreeOpen),
        (Command::Workspace, Method::WorktreeRemove),
    ]
    .into_iter()
    .map(|(command, method)| (command, method, true, None))
    .chain([
        (Command::CloseTab, Method::TabClose, false, None),
        (Command::CloseTab, Method::TabClose, false, Some("inactive")),
    ]) {
        let (endpoint, mut server) = connected_endpoint("ssh:fixture");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints.truncate(1);
                view.endpoints.push(endpoint);
                view.selected_endpoint = 1;
                view.options = ConnectOptions::default();
                view.reset_selected();
                view.activation_deadline = None;
                view.config.confirm_close_tab = confirm_close_tab;
                assert!(view.input_ready());
                if let Some(id) = explicit_tab {
                    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                    let mut tab = snapshot.tabs[0].clone();
                    tab.tab_id = id.into();
                    tab.focused = false;
                    snapshot.tabs.push(tab);
                    assert_ne!(snapshot.focused_tab_id.as_deref(), Some(id));
                    view.open_tab_close(id, window, cx);
                } else if matches!(
                    method,
                    Method::WorkspaceClose
                        | Method::WorktreeCreate
                        | Method::WorktreeOpen
                        | Method::WorktreeRemove
                ) {
                    crate::menu::workspace_tests::submit_focus_change(view, method, window, cx);
                } else {
                    view.command(command, window, cx);
                }
                match command {
                    Command::ClosePane | Command::CloseTab if confirm_close_tab => {
                        view.confirm_close(window, cx);
                    }
                    Command::WorkspacePicker => view.confirm_palette_row(0, window, cx),
                    Command::Palette => {
                        // The configured entry follows all native entries except Palette.
                        view.confirm_palette_row(crate::controls::COMMANDS.len() - 1, window, cx);
                    }
                    _ => {}
                }
                if !confirm_close_tab {
                    assert!(view.menu.page.is_none());
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
        assert_eq!(request["method"], method.as_str());
        if method == Method::WorktreeOpen {
            assert_eq!(
                request["params"],
                serde_json::json!({"workspace_id": "w3",
                "path": "/endpoint/existing checkout ", "focus": true, "trust_repository": false})
            );
        }
        if method == Method::TabClose {
            let focused = snapshot().focused_tab_id.unwrap();
            assert_eq!(
                request["params"],
                serde_json::json!({"tab_id": explicit_tab.unwrap_or(&focused)})
            );
        }
        // herdr-client serializes API requests behind their predecessor's reply.
        server.respond(&request);
        let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
            panic!("missing surface barrier");
        };
        let barrier: serde_json::Value = serde_json::from_str(&request).unwrap();
        assert_eq!(barrier["method"], Method::ClientShellSurfaceSet.as_str());
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
            project_until(view, cx, "fresh frame", |view| {
                view.live.snapshot.as_ref().map(|s| s.revision) == Some(next.revision)
                    && view.live.activation.is_some()
            });
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
            project_until(view, cx, "ack ahead of the frame", |view| {
                view.live
                    .activation
                    .as_ref()
                    .and_then(|activation| activation.revision)
                    == Some(next.revision + 1)
            });
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
            project_until(view, cx, "frame matching the ack", |view| {
                view.live.surface.as_ref().map(|s| s.projection_revision) == Some(next.revision)
            });
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

#[gpui_kit::test]
fn unconfirmed_tab_close_rejects_invalid_targets_and_unready_input(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:fixture");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints.push(endpoint);
            view.selected_endpoint = 1;
            view.reset_selected();
            view.activation_deadline = None;
            view.config.confirm_close_tab = false;
            assert!(view.input_ready());
            let ready = view.live.clone();
            let tab = ready
                .snapshot
                .as_ref()
                .unwrap()
                .focused_tab_id
                .as_ref()
                .unwrap();
            for explicit in [true, false] {
                for rejection in ["missing-tab", "missing-workspace", "unready-input"] {
                    view.live = ready.clone();
                    match rejection {
                        "missing-tab" => Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                            .tabs
                            .clear(),
                        "missing-workspace" => Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                            .workspaces
                            .clear(),
                        _ => {
                            view.live.surface = None;
                            assert!(!view.input_ready());
                        }
                    }
                    if explicit {
                        view.open_tab_close(tab, window, cx);
                    } else {
                        view.command(Command::CloseTab, window, cx);
                    }
                    assert!(view.activation_deadline.is_none());
                    assert!(view.pending_navigation.is_none());
                    view.dismiss_menu(window, cx);
                }
            }
            // FIFO marker proves none of the rejected attempts reached the peer.
            view.endpoints[1]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&ready.snapshot.as_ref().unwrap().boot_id, false)
                .unwrap();
        });
    });
    assert!(matches!(
        server.receive(),
        ClientMessage::ClientShellFocus { focused: false }
    ));
}

#[gpui_kit::test]
fn retiring_release_source_unblocks_destination_without_waiting_for_timeout(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
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
            view.reset_selected();
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
            Method::ClientShellSurfaceSet.as_str()
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
    assert_eq!(endpoint.retry_delay(), Duration::from_secs(30));
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
fn retry_backoff_doubles_from_half_a_second_to_thirty_seconds() {
    let mut endpoint = Endpoint::new(
        LOCAL.into(),
        LOCAL.into(),
        ConnectTarget::Socket("/nonexistent".into()),
        true,
    );
    let delays: Vec<_> = (0..10)
        .map(|attempts| {
            endpoint.attempts = attempts;
            endpoint.retry_delay().as_millis()
        })
        .collect();
    assert_eq!(
        delays,
        [
            500, 1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000, 30000
        ]
    );
    endpoint.attempts = u32::MAX;
    assert_eq!(endpoint.retry_delay(), Duration::from_secs(30));
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
    assert_eq!(endpoint.retry_at, now + Duration::from_secs(59 + 30));
    endpoint.connect(ConnectOptions::default(), false);
    assert_eq!(
        endpoint.attempts, 9,
        "automatic retry must preserve failed attempts"
    );
    assert!(endpoint.online_since.is_none());
}

#[gpui_kit::test]
fn changed_target_and_manual_reconnect_reset_retry_history(cx: &mut gpui_kit::TestAppContext) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
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
        view.reconnect(); // Explicit isolated missing socket, never SSH/discovery.
        assert_eq!(
            view.endpoints[0].attempts, 1,
            "manual reconnect starts a fresh first attempt"
        );
        assert_eq!(view.endpoints[0].retry_delay(), Duration::from_secs(1));
    });
}

fn split_request(server: &mut Server) -> serde_json::Value {
    let ClientMessage::ClientShellEndpointRequest { request, .. } = server.receive() else {
        panic!("missing split ratio request");
    };
    let request: serde_json::Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["method"], "layout.set_split_ratio");
    request
}

#[gpui_kit::test]
fn connected_split_drag_sends_coalesced_ratios_and_stops_on_layout_change(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (endpoint, mut server) = connected_endpoint("ssh:split");
    let down = |view: &mut HerdrWindow, column, cx: &mut Context<HerdrWindow>| {
        view.split_mouse_down(
            &MouseDownEvent {
                position: mouse_position(view, column, 5.5),
                button: MouseButton::Left,
                ..Default::default()
            },
            cx,
        )
    };
    let drag = |view: &mut HerdrWindow, column, cx: &mut Context<HerdrWindow>| {
        assert!(view.split_mouse_move(
            &MouseMoveEvent {
                position: mouse_position(view, column, 9.5),
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            },
            cx
        ));
    };
    let up = |view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>| {
        assert!(view.split_mouse_up(
            &MouseUpEvent {
                position: mouse_position(view, 0., 0.),
                button: MouseButton::Left,
                ..Default::default()
            },
            cx
        ));
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            prepare_mouse(view, endpoint);
            Arc::make_mut(view.live.surface.as_mut().unwrap()).splits = vec![PaneSurfaceSplit {
                direction: PaneSurfaceSplitDirection::Horizontal,
                pos: 40,
                area: SurfaceRect {
                    x: 0,
                    y: 0,
                    width: 80,
                    height: 24,
                },
                hit_rect: SurfaceRect {
                    x: 40,
                    y: 0,
                    width: 1,
                    height: 24,
                },
                path: vec![true],
            }];
            // Projections of later answers must keep presenting this layout.
            let surface = view.live.surface.clone().unwrap();
            view.endpoints[1]
                .connection
                .inbox
                .lock()
                .unwrap()
                .apply(ClientEvent::Surface(surface));
            // Beside the border the pane keeps the pointer, even one that
            // reports mouse input; on it, the border wins.
            assert_eq!(view.split_cursor_at(mouse_position(view, 39.9, 5.)), None);
            assert!(!down(view, 39.9, cx));
            assert_eq!(
                view.split_cursor_at(mouse_position(view, 40.5, 5.)),
                Some(gpui_kit::CursorStyle::ResizeLeftRight)
            );

            // A click on the border is not a resize.
            assert!(down(view, 40.5, cx));
            up(view, cx);
            assert!(view.split_drag.is_none());
            assert!(view.live.drag_request.is_none());

            // Pressed half a cell into the border, so the grab keeps that offset.
            assert!(down(view, 40.5, cx));
            assert!(view.terminal_mouse.is_none());
            drag(view, 20.5, cx);
            assert!(view.live.drag_request.is_some());
            // Later positions wait for that answer, and only the last is kept.
            drag(view, 60.5, cx);
            drag(view, 70.5, cx);
            // The pointer leaving the border keeps the cursor it grabbed.
            assert_eq!(
                view.split_cursor_at(mouse_position(view, 5., 5.)),
                Some(gpui_kit::CursorStyle::ResizeLeftRight)
            );
        });
    });
    let first = split_request(&mut server);
    assert_eq!(
        first["params"],
        serde_json::json!({"tab_id": "w1:t1", "path": [true], "ratio": 0.25})
    );
    server.respond(&first);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            project_until(view, cx, "split answer", |view| {
                view.live.drag_request.is_none()
            });
            view.flush_split(cx);
            up(view, cx);
            // Released with the last ratio still unanswered.
            assert!(view.split_drag.is_some());
        });
    });
    let last = split_request(&mut server);
    assert_eq!(last["params"]["ratio"], serde_json::json!(0.875));
    server.respond(&last);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            project_until(view, cx, "last split answer", |view| {
                view.live.drag_request.is_none()
            });
            view.flush_split(cx);
            assert!(view.split_drag.is_none());
            assert_eq!(view.split_cursor_at(mouse_position(view, 5., 5.)), None);

            // A snapshot ahead of its surface is waited out.
            assert!(down(view, 40.5, cx));
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).revision += 1;
            drag(view, 30.5, cx);
            assert!(view.split_drag.is_some());
            assert!(view.live.drag_request.is_none());
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).revision -= 1;
            // A pane closing under the drag could move another border at the
            // same path, so the drag ends without sending.
            Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[1].pane_id = "w1:p3".into();
            drag(view, 30.5, cx);
            assert!(view.split_drag.is_none());
            assert!(view.live.drag_request.is_none());
            assert!(view.local_error.is_none());
        });
    });
}
