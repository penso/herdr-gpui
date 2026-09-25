//! Large files travel on an independent SSH stream; the daemon input lane stays
//! responsive. Only completed copies can paste paths, into their captured target.

use super::HerdrWindow;
use crate::{connection::ConnectionBridge, fonts::StyledFont, terminal::InputTarget};
use gpui::{prelude::*, *};
use herdr_client::{ConnectTarget, protocol::ClientPaneInputEvent};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub(crate) struct FileTransfer {
    endpoint: String,
    host: String,
    epoch: u64,
    generation: u64,
    boot: String,
    target: InputTarget,
    label: String,
    cancelled: Arc<AtomicBool>,
    sent: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    shown: (u64, u64, bool),
}

impl FileTransfer {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl Drop for FileTransfer {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn transfer_progress(sent: u64, total: u64) -> (f32, String) {
    let size = |bytes: u64| {
        let mut value = bytes as f64;
        let mut unit = "B";
        for next in ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"] {
            if value < 1024. {
                break;
            }
            value /= 1024.;
            unit = next;
        }
        let number = format!("{value:.1}");
        format!("{} {unit}", number.trim_end_matches(".0"))
    };
    let fraction = if total == 0 {
        0.
    } else {
        (sent as f64 / total as f64).clamp(0., 1.) as f32
    };
    (
        fraction,
        format!("{} / {} ({:.0}%)", size(sent), size(total), fraction * 100.),
    )
}

impl HerdrWindow {
    fn file_transfer_current(&self, transfer: &FileTransfer) -> bool {
        let endpoint = &self.endpoints[self.selected_endpoint];
        transfer.epoch == self.selection_epoch
            && transfer.generation == endpoint.generation
            && transfer.endpoint == endpoint.id
            && matches!(&endpoint.connection.target, ConnectTarget::Ssh { target, .. } if target == &transfer.host)
            && self.live.status.is_connected()
            && self.live.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.boot_id == transfer.boot
                    && match &transfer.target {
                        InputTarget::Pane(id) => {
                            snapshot.panes.iter().any(|pane| &pane.pane_id == id)
                        }
                        InputTarget::Popup(_) => true,
                    }
            })
            && self
                .live
                .surface
                .as_ref()
                .is_none_or(|surface| match &transfer.target {
                    InputTarget::Pane(id) => {
                        surface.popup.is_none()
                            && surface.panes.iter().any(|pane| &pane.pane_id == id)
                    }
                    InputTarget::Popup(id) => surface
                        .popup
                        .as_ref()
                        .is_some_and(|popup| &popup.terminal_id == id),
                })
    }

    pub(crate) fn poll_file_transfer(&mut self, cx: &mut Context<Self>) {
        let Some(transfer) = &self.file_transfer else {
            return;
        };
        if !self.file_transfer_current(transfer) {
            transfer.cancel();
        }
        let progress = (
            transfer.sent.load(Ordering::Relaxed),
            transfer.total.load(Ordering::Relaxed),
            transfer.cancelled.load(Ordering::Acquire),
        );
        if progress != transfer.shown
            && let Some(transfer) = &mut self.file_transfer
        {
            transfer.shown = progress;
            cx.notify();
        }
    }

    pub(crate) fn start_file_transfer(
        &mut self,
        target: InputTarget,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.start_file_transfer_with(
            target,
            paths,
            |host, paths, cancelled, progress| {
                herdr_client::upload_files(host, paths, cancelled, progress)
            },
            herdr_client::remove_uploaded_files,
            cx,
        );
    }

    fn start_file_transfer_with(
        &mut self,
        target: InputTarget,
        paths: Vec<PathBuf>,
        upload: impl FnOnce(
            &str,
            &[PathBuf],
            &AtomicBool,
            &mut dyn FnMut(u64, u64),
        ) -> herdr_client::Result<Vec<String>>
        + Send
        + 'static,
        cleanup: impl FnOnce(&str, &[String]) -> herdr_client::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if !self.accepts_remote_images() || paths.is_empty() {
            return;
        }
        if self.file_transfer.is_some() {
            self.local_transfer_notice(
                "Copy not started",
                "Another file copy is still running. Cancel it or wait for it to finish.".into(),
                cx,
            );
            return;
        }
        let endpoint = &self.endpoints[self.selected_endpoint];
        let ConnectTarget::Ssh { target: host, .. } = &endpoint.connection.target else {
            return;
        };
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        let host = host.clone();
        let boot = snapshot.boot_id.clone();
        let endpoint_id = endpoint.id.clone();
        let endpoint_label = crate::notifications::safe_text(&endpoint.label, 160);
        let generation = endpoint.generation;
        let label = if paths.len() == 1 {
            paths[0]
                .file_name()
                .map(|name| crate::notifications::safe_text(&name.to_string_lossy(), 160))
                .unwrap_or_else(|| "File".into())
        } else {
            format!("{} files", paths.len())
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let sent = Arc::new(AtomicU64::new(0));
        let total = Arc::new(AtomicU64::new(0));
        let worker_cancel = cancelled.clone();
        let worker_sent = sent.clone();
        let worker_total = total.clone();
        let worker_host = host.clone();
        let executor = cx.background_executor().clone();
        let work = executor.spawn(async move {
            upload(&worker_host, &paths, &worker_cancel, &mut |sent, total| {
                worker_total.store(total, Ordering::Relaxed);
                worker_sent.store(sent, Ordering::Relaxed);
            })
        });
        let cleanup_host = host.clone();
        let task_cancel = cancelled.clone();
        // The weak entity may disappear before SSH returns. Keep awaiting the
        // result so successful but unclaimed files are still cleaned up.
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let accepted = this.update(cx, |this, cx| {
                let Some(transfer) = &this.file_transfer else { return false; };
                if !Arc::ptr_eq(&transfer.cancelled, &task_cancel) {
                    return false;
                }
                let current = this.file_transfer_current(transfer);
                let cancelled = transfer.cancelled.load(Ordering::Acquire) || !current;
                let error = match &result {
                    Ok(paths) if !cancelled && this.menu.page.is_none() && this.input_ready() => {
                        let text = super::file_drop::quote_paths(&paths.iter().map(PathBuf::from).collect::<Vec<_>>());
                        text.and_then(|text| {
                            let handle = this.endpoints[this.selected_endpoint].connection.handle.as_ref().ok_or(crate::Error::NotConnected)?;
                            ConnectionBridge::send_input(handle, &transfer.boot, &transfer.target, ClientPaneInputEvent::Paste(text))?;
                            Ok(())
                        }).err().map(|error: crate::Error| error.to_string())
                    }
                    Ok(_) => Some(herdr_client::Error::UploadCancelled.to_string()),
                    Err(error) => Some(error.to_string()),
                };
                let accepted = error.is_none();
                if current && !matches!(&result, Err(herdr_client::Error::UploadCleanup { .. })) {
                    match error {
                        None => this.local_transfer_notice("Copy complete", "Remote file paths pasted. Files remain in the remote temporary directory until removed.".into(), cx),
                        Some(error) => this.local_transfer_notice(if cancelled { "Copy cancelled" } else { "Copy failed" }, error, cx),
                    }
                }
                accepted
            }).unwrap_or(false);
            let cleanup_failed = match result {
                Ok(paths) if !accepted => executor
                    .spawn(async move { cleanup(&cleanup_host, &paths) })
                    .await
                    .is_err(),
                Err(herdr_client::Error::UploadCleanup { .. }) => true,
                _ => false,
            };
            if cleanup_failed {
                let _ = this.update(cx, |this, cx| {
                    this.local_transfer_notice("Remote cleanup failed", format!("No path was pasted. Temporary files may remain on the original host ({endpoint_label})."), cx);
                });
            }
            let _ = this.update(cx, |this, cx| {
                if this.file_transfer.as_ref().is_some_and(|transfer| Arc::ptr_eq(&transfer.cancelled, &task_cancel)) {
                    this.file_transfer = None;
                    cx.notify();
                }
            });
        }).detach();
        self.file_transfer = Some(FileTransfer {
            endpoint: endpoint_id,
            host,
            epoch: self.selection_epoch,
            generation,
            boot,
            target,
            label,
            cancelled,
            sent,
            total,
            shown: (0, 0, false),
        });
        cx.notify();
    }

    pub(super) fn render_file_transfer(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let transfer = self.file_transfer.as_ref()?;
        if self.menu.page.is_some() {
            return None;
        }
        let (sent, total, cancelled) = transfer.shown;
        let (fraction, progress) = transfer_progress(sent, total);
        let title = if cancelled {
            "Cancelling..."
        } else if total > 0 && sent >= total {
            "Finalizing copy..."
        } else {
            "Copying..."
        };
        let accent = self.theme.primary();
        Some(
            div()
                .id("file-transfer")
                .debug_selector(|| "file-transfer".into())
                .absolute()
                .right(px(12.))
                .top(px(72.))
                .w((window.viewport_size().width - px(24.))
                    .max(px(0.))
                    .min(px(340.)))
                .occlude()
                .rounded(px(crate::config::corners::PANEL))
                .border_1()
                .border_color(rgb(accent))
                .bg(crate::config::background(
                    self.theme.surface,
                    self.paint_opacity(),
                ))
                .text_color(rgb(self.theme.foreground))
                .text_font(&self.config.ui)
                .text_size(px(self.config.ui.size))
                .p(px(12.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(div().child(title))
                .child(div().truncate().child(transfer.label.clone()))
                .child(
                    div()
                        .debug_selector(|| "file-transfer-track".into())
                        .w_full()
                        .h(px(6.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .overflow_hidden()
                        .bg(rgb(self.theme.active))
                        .child(
                            div()
                                .debug_selector(|| "file-transfer-progress".into())
                                .h_full()
                                .w(relative(fraction))
                                .bg(rgb(accent)),
                        ),
                )
                .child(
                    div()
                        .text_color(rgb(self.theme.readable_chrome(self.paint_opacity()).muted))
                        .child(progress),
                )
                .child(
                    div()
                        .id("cancel-file-transfer")
                        .debug_selector(|| "cancel-file-transfer".into())
                        .cursor_pointer()
                        .child(if cancelled { "Cancelling" } else { "Cancel" })
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(transfer) = &this.file_transfer {
                                transfer.cancel();
                            }
                            this.poll_file_transfer(cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{FileTransfer, HerdrWindow, transfer_progress};
    use crate::{connection::ConnectionBridge, terminal::InputTarget};
    use crate::{sidebar::layout_tests::fixture_window, state::ConnectionStatus};
    use gpui::{
        AppContext, Bounds, Context, Entity, IntoElement, Modifiers, MouseButton, Render,
        TestAppContext, VisualTestContext, Window, div, point, prelude::*, px, size,
    };
    use herdr_client::{
        Client, ClientEvent, ConnectOptions, ConnectTarget, Stream, connect_with_connector,
        protocol::{endpoint::*, *},
    };
    use std::{
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };

    const HOST: &str = "upload-test.invalid";
    const REMOTE: &str = "/tmp/herdr-upload.ABCDEF123456/it's a file";

    #[test]
    fn progress_sizes_and_percentages_are_readable_and_bounded() {
        for (sent, total, fraction, label) in [
            (0, 0, 0., "0 B / 0 B (0%)"),
            (512, 1024, 0.5, "512 B / 1 KiB (50%)"),
            (1024 * 1024, 2 * 1024 * 1024, 0.5, "1 MiB / 2 MiB (50%)"),
            (1_288_490_189, 4_294_967_296, 0.3, "1.2 GiB / 4 GiB (30%)"),
            (4_294_967_296, 8_589_934_592, 0.5, "4 GiB / 8 GiB (50%)"),
            (2048, 1024, 1., "2 KiB / 1 KiB (100%)"),
            (u64::MAX, u64::MAX, 1., "16 EiB / 16 EiB (100%)"),
        ] {
            assert_eq!(transfer_progress(sent, total), (fraction, label.into()));
        }
    }

    // Render only the actual transfer card, over real terminal mouse handlers.
    // Keeping the terminal canvas out avoids unrelated resize/paint commands.
    struct Fixture {
        view: Option<Entity<HerdrWindow>>,
        fallthrough: usize,
    }

    impl Render for Fixture {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let overlay = self
                .view
                .as_ref()
                .and_then(|view| view.update(cx, |view, cx| view.render_file_transfer(window, cx)));
            div()
                .size_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event, window, cx| {
                        this.fallthrough += 1;
                        if let Some(view) = &this.view {
                            view.update(cx, |view, cx| {
                                view.terminal_mouse_down(event, window, cx);
                            });
                        }
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, event, _, cx| {
                        this.fallthrough += 1;
                        if let Some(view) = &this.view {
                            view.update(cx, |view, cx| view.terminal_mouse_up(event, cx));
                        }
                    }),
                )
                .children(overlay)
        }
    }

    fn fixture(window: &mut Window, cx: &mut Context<Fixture>) -> Fixture {
        Fixture {
            view: Some(cx.new(|cx| fixture_window(window, cx))),
            fallthrough: 0,
        }
    }

    /// A daemon peer that has sent its welcome and snapshot fixtures.
    pub(crate) struct Peer {
        pub(crate) client: Client,
        stream: Stream,
    }

    impl Peer {
        pub(crate) fn new() -> Self {
            let (stream, mut server) = Stream::pair().unwrap();
            server
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            server
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let client = connect_with_connector(
                ConnectTarget::Local,
                ConnectOptions::default(),
                true,
                move |_, _| Ok(stream),
            )
            .unwrap();
            assert!(matches!(
                read_message(&mut server, MAX_FRAME_SIZE).unwrap(),
                ClientMessage::EndpointControl { .. }
            ));
            for (kind, data) in [
                (
                    ENDPOINT_WELCOME_KIND,
                    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json"),
                ),
                (
                    ENDPOINT_SNAPSHOT_KIND,
                    include_str!(
                        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
                    ),
                ),
            ] {
                write_message(
                    &mut server,
                    &ServerMessage::EndpointControl {
                        kind: kind.into(),
                        data: data.into(),
                    },
                    MAX_FRAME_SIZE,
                )
                .unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                match client
                    .events
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .unwrap()
                {
                    ClientEvent::Snapshot(_) => break,
                    ClientEvent::Disconnected { reason } => {
                        panic!("mock peer disconnected: {reason}")
                    }
                    _ => {}
                }
            }
            Self {
                client,
                stream: server,
            }
        }

        pub(crate) fn receive(&mut self) -> ClientMessage {
            read_message(&mut self.stream, MAX_FRAME_SIZE).unwrap()
        }

        fn prepare(&self, view: &mut HerdrWindow) {
            // Only the fixture's explicit nonexistent local socket is used to
            // initialize endpoint lifecycle flags. Replace its handle before
            // marking the synthetic projection as SSH; never reconnect to HOST.
            view.reconnect();
            if let Some(handle) = view.endpoints[0].connection.handle.take() {
                handle.disconnect();
            }
            view.endpoints[0].connection.handle = Some(self.client.handle.clone());
            view.endpoints[0].connection.target = ConnectTarget::Ssh {
                target: HOST.into(),
                session: "default".into(),
            };
            let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
                "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
            ))
            .unwrap();
            view.live.snapshot = Some(Arc::new(snapshot.clone()));
            view.live.status = ConnectionStatus::Connected;
            view.live.surface = Some(Arc::new(PaneSurfaceFrame {
                boot_id: snapshot.boot_id,
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
                panes: vec![PaneSurfacePane {
                    pane_id: "w1:p1".into(),
                    content_revision: 1,
                    rect: SurfaceRect {
                        x: 0,
                        y: 0,
                        width: 80,
                        height: 24,
                    },
                    inner_rect: SurfaceRect {
                        x: 0,
                        y: 0,
                        width: 80,
                        height: 24,
                    },
                    scrollbar_rect: None,
                    scroll: None,
                    focused: true,
                    mouse_reporting: true,
                    sgr_pixel_mouse: false,
                    alternate_screen_active: false,
                    pixel_width: 800,
                    pixel_height: 480,
                }],
                splits: vec![],
                popup: None,
                graphics: Default::default(),
            }));
            view.options = ConnectOptions::default();
            view.cell_width = 10.;
            view.bounds = Bounds::new(point(px(0.), px(0.)), size(px(800.), px(480.)));
            assert!(view.accepts_remote_images());
        }

        fn sentinel(&mut self, view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
            view.read_with(cx, |view, _| {
                ConnectionBridge::send_input(
                    &self.client.handle,
                    &view.live.snapshot.as_ref().unwrap().boot_id,
                    &InputTarget::Pane("w1:p1".into()),
                    ClientPaneInputEvent::TextCommit("sentinel".into()),
                )
                .unwrap();
            });
            assert_eq!(
                self.receive(),
                ClientMessage::ClientShellPaneInput {
                    pane_id: "w1:p1".into(),
                    events: vec![ClientPaneInputEvent::TextCommit("sentinel".into())],
                }
            );
        }
    }

    impl Drop for Peer {
        fn drop(&mut self) {
            self.client.handle.disconnect();
        }
    }

    fn popup(view: &mut HerdrWindow, id: &str) {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.popup = Some(Box::new(ClientShellPopupSurface {
            terminal_id: id.into(),
            title: String::new(),
            width: None,
            height: None,
            frame: surface.frame.clone(),
            mouse_reporting: true,
            sgr_pixel_mouse: false,
            pixel_width: 800,
            pixel_height: 480,
        }));
    }

    fn pending(view: &HerdrWindow, target: InputTarget) -> FileTransfer {
        FileTransfer {
            endpoint: view.endpoints[0].id.clone(),
            host: HOST.into(),
            epoch: view.selection_epoch,
            generation: view.endpoints[0].generation,
            boot: view.live.snapshot.as_ref().unwrap().boot_id.clone(),
            target,
            label: "large file".into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            sent: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
            shown: (0, 0, false),
        }
    }

    #[gpui::test]
    fn progress_above_four_gib_and_real_cancel_click_are_isolated(cx: &mut TestAppContext) {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let mut peer = Peer::new();
        let cancelled = view.update(cx, |view, cx| {
            peer.prepare(view);
            let transfer = pending(view, InputTarget::Pane("w1:p1".into()));
            let cancelled = transfer.cancelled.clone();
            transfer
                .sent
                .store(4 * 1024 * 1024 * 1024, Ordering::Relaxed);
            transfer
                .total
                .store(8 * 1024 * 1024 * 1024, Ordering::Relaxed);
            view.file_transfer = Some(transfer);
            view.poll_file_transfer(cx);
            assert_eq!(
                view.file_transfer.as_ref().unwrap().shown,
                (4_294_967_296, 8_589_934_592, false)
            );
            cancelled
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let track = cx.debug_bounds("file-transfer-track").unwrap();
        let progress = cx.debug_bounds("file-transfer-progress").unwrap();
        assert!((f32::from(progress.size.width) / f32::from(track.size.width) - 0.5).abs() < 0.001);
        let cancel = cx.debug_bounds("cancel-file-transfer").unwrap();
        cx.simulate_click(cancel.center(), Modifiers::default());
        assert!(cancelled.load(Ordering::Acquire));
        fixture.read_with(cx, |fixture, _| assert_eq!(fixture.fallthrough, 0));
        view.read_with(cx, |view, _| {
            assert!(view.file_transfer.as_ref().unwrap().shown.2);
            assert!(view.terminal_mouse.is_none());
            assert!(view.local_error.is_none());
        });
        peer.sentinel(&view, cx);
    }

    #[gpui::test]
    fn duplicate_copy_never_invokes_second_backend(cx: &mut TestAppContext) {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let mut peer = Peer::new();
        let calls = Arc::new(AtomicU64::new(0));
        let worker_calls = calls.clone();
        view.update(cx, |view, cx| {
            peer.prepare(view);
            view.start_file_transfer_with(
                InputTarget::Pane("w1:p1".into()),
                vec!["large file".into()],
                move |host, paths, cancelled, progress| {
                    assert_eq!(host, HOST);
                    assert_eq!(paths, [PathBuf::from("large file")]);
                    assert!(!cancelled.load(Ordering::Acquire));
                    progress(4_294_967_296, 8_589_934_592);
                    worker_calls.fetch_add(1, Ordering::Relaxed);
                    Err(herdr_client::Error::UploadPathLimit)
                },
                |_, _| panic!("failed uploads must not be cleaned up twice"),
                cx,
            );
            let token = view.file_transfer.as_ref().unwrap().cancelled.clone();
            view.start_file_transfer_with(
                InputTarget::Pane("w1:p1".into()),
                vec!["second".into()],
                |_, _, _, _| panic!("duplicate upload started"),
                |_, _| panic!("duplicate cleanup started"),
                cx,
            );
            assert!(Arc::ptr_eq(
                &token,
                &view.file_transfer.as_ref().unwrap().cancelled
            ));
            assert_eq!(
                view.endpoints[0].toasts.entries.back().unwrap().1.title,
                "Copy not started"
            );
        });
        cx.run_until_parked();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        view.read_with(cx, |view, _| {
            assert!(view.file_transfer.is_none());
            let notice = &view.endpoints[0].toasts.entries.back().unwrap().1;
            assert_eq!(notice.title, "Copy failed");
            assert_eq!(
                notice.body.as_deref(),
                Some(herdr_client::Error::UploadPathLimit.to_string().as_str())
            );
        });
        peer.sentinel(&view, cx);
    }

    #[gpui::test]
    fn captured_target_fences_cancel_and_reset_cancels_immediately(cx: &mut TestAppContext) {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let peer = Peer::new();
        view.update(cx, |view, cx| {
            peer.prepare(view);
            let live = view.live.clone();
            let epoch = view.selection_epoch;
            let generation = view.endpoints[0].generation;
            let id = view.endpoints[0].id.clone();
            for case in 0..12 {
                view.live = live.clone();
                view.selection_epoch = epoch;
                view.endpoints[0].generation = generation;
                view.endpoints[0].id = id.clone();
                view.endpoints[0].connection.target = ConnectTarget::Ssh {
                    target: HOST.into(),
                    session: "default".into(),
                };
                let target = if matches!(case, 7 | 8) {
                    popup(view, "popup");
                    InputTarget::Popup("popup".into())
                } else {
                    InputTarget::Pane("w1:p1".into())
                };
                view.file_transfer = Some(pending(view, target));
                assert!(view.file_transfer_current(view.file_transfer.as_ref().unwrap()));
                match case {
                    0 => view.selection_epoch += 1,
                    1 => view.endpoints[0].generation += 1,
                    2 => Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                        .boot_id
                        .push_str("-new"),
                    3 => Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                        .panes
                        .clear(),
                    4 => Arc::make_mut(view.live.surface.as_mut().unwrap())
                        .panes
                        .clear(),
                    5 => view.endpoints[0].id.push_str("-new"),
                    6 => popup(view, "popup"),
                    7 => popup(view, "replacement"),
                    8 => Arc::make_mut(view.live.surface.as_mut().unwrap()).popup = None,
                    9 => view.live.status = ConnectionStatus::Disconnected,
                    10 => {
                        view.endpoints[0].connection.target = ConnectTarget::Ssh {
                            target: "other.invalid".into(),
                            session: "default".into(),
                        }
                    }
                    11 => {
                        let cancelled = view.file_transfer.as_ref().unwrap().cancelled.clone();
                        view.detach_endpoint();
                        assert!(
                            cancelled.load(Ordering::Acquire),
                            "reset must cancel without polling"
                        );
                    }
                    _ => unreachable!(),
                }
                view.poll_file_transfer(cx);
                assert!(view.file_transfer.as_ref().unwrap().shown.2, "case {case}");
            }
        });
    }

    #[gpui::test]
    fn success_pastes_quoted_paths_to_captured_pane_or_popup(cx: &mut TestAppContext) {
        for is_popup in [false, true] {
            let (fixture, cx) = cx.add_window_view(fixture);
            let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
            let mut peer = Peer::new();
            view.update(cx, |view, cx| {
                peer.prepare(view);
                let target = if is_popup {
                    popup(view, "popup");
                    InputTarget::Popup("popup".into())
                } else {
                    InputTarget::Pane("w1:p1".into())
                };
                view.start_file_transfer_with(
                    target,
                    vec!["source".into()],
                    |_, _, _, progress| {
                        progress(9, 9);
                        Ok(vec![REMOTE.into()])
                    },
                    |_, _| panic!("accepted upload was removed"),
                    cx,
                );
                Arc::make_mut(view.live.snapshot.as_mut().unwrap()).focused_pane_id =
                    Some("different-pane".into());
            });
            cx.run_until_parked();
            let events = vec![ClientPaneInputEvent::Paste(
                "'/tmp/herdr-upload.ABCDEF123456/it'\\''s a file'".into(),
            )];
            assert_eq!(
                peer.receive(),
                if is_popup {
                    ClientMessage::ClientShellPopupInput {
                        terminal_id: "popup".into(),
                        events,
                    }
                } else {
                    ClientMessage::ClientShellPaneInput {
                        pane_id: "w1:p1".into(),
                        events,
                    }
                }
            );
            view.read_with(cx, |view, _| {
                assert!(view.file_transfer.is_none());
                assert!(view.local_error.is_none());
                assert_eq!(
                    view.endpoints[0].toasts.entries.back().unwrap().1.title,
                    "Copy complete"
                );
            });
            peer.sentinel(&view, cx);
        }
    }

    #[gpui::test]
    fn stale_or_cancelled_success_cleans_original_host_without_pasting(cx: &mut TestAppContext) {
        for case in 0..4 {
            let (fixture, cx) = cx.add_window_view(fixture);
            let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
            let mut peer = Peer::new();
            let (tx, rx) = mpsc::channel();
            view.update(cx, |view, cx| {
                peer.prepare(view);
                view.start_file_transfer_with(
                    InputTarget::Pane("w1:p1".into()),
                    vec!["source".into()],
                    |_, _, _, _| Ok(vec![REMOTE.into()]),
                    move |host, paths| {
                        tx.send((host.to_owned(), paths.to_vec())).unwrap();
                        Ok(())
                    },
                    cx,
                );
                match case {
                    0 => {
                        view.selection_epoch += 1;
                        view.endpoints[0].connection.target = ConnectTarget::Ssh {
                            target: "other.invalid".into(),
                            session: "default".into(),
                        };
                    }
                    1 => view.file_transfer.as_ref().unwrap().cancel(),
                    2 => view.menu.page = Some(crate::menu::Page::Palette),
                    3 => view.pending_toast = Some(1),
                    _ => unreachable!(),
                }
            });
            cx.run_until_parked();
            assert_eq!(rx.try_recv().unwrap(), (HOST.into(), vec![REMOTE.into()]));
            view.read_with(cx, |view, _| assert!(view.file_transfer.is_none()));
            peer.sentinel(&view, cx);
        }
    }

    #[gpui::test]
    fn dropping_entity_cancels_but_detached_completion_still_cleans(cx: &mut TestAppContext) {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let weak = view.downgrade();
        let peer = Peer::new();
        let (tx, rx) = mpsc::channel();
        let cancelled = view.update(cx, |view, cx| {
            peer.prepare(view);
            view.start_file_transfer_with(
                InputTarget::Pane("w1:p1".into()),
                vec!["source".into()],
                |_, _, _, _| Ok(vec![REMOTE.into()]),
                move |host, paths| {
                    tx.send((host.to_owned(), paths.to_vec())).unwrap();
                    Ok(())
                },
                cx,
            );
            view.file_transfer.as_ref().unwrap().cancelled.clone()
        });
        fixture.update(cx, |fixture, _| {
            fixture.view = None;
        });
        drop(view);
        cx.update(|_, _| {});
        assert!(weak.upgrade().is_none());
        assert!(cancelled.load(Ordering::Acquire));
        cx.run_until_parked();
        assert_eq!(rx.try_recv().unwrap(), (HOST.into(), vec![REMOTE.into()]));
    }

    #[gpui::test]
    fn old_completion_preserves_replacement_and_reports_cleanup_failure(cx: &mut TestAppContext) {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let mut peer = Peer::new();
        let (tx, rx) = mpsc::channel();
        let replacement = view.update(cx, |view, cx| {
            peer.prepare(view);
            view.endpoints[0].label = "Original\n host".into();
            view.start_file_transfer_with(
                InputTarget::Pane("w1:p1".into()),
                vec!["source".into()],
                |_, _, _, _| Ok(vec![REMOTE.into()]),
                move |host, paths| {
                    tx.send((host.to_owned(), paths.to_vec())).unwrap();
                    Err(herdr_client::Error::UploadCleanupPath)
                },
                cx,
            );
            let original = view.file_transfer.as_ref().unwrap().cancelled.clone();
            let replacement = pending(view, InputTarget::Pane("w1:p1".into()));
            let token = replacement.cancelled.clone();
            view.file_transfer = Some(replacement);
            view.endpoints[0].label = "Replacement host".into();
            assert!(original.load(Ordering::Acquire));
            token
        });
        cx.run_until_parked();
        assert_eq!(rx.try_recv().unwrap(), (HOST.into(), vec![REMOTE.into()]));
        view.read_with(cx, |view, _| {
            assert!(Arc::ptr_eq(
                &replacement,
                &view.file_transfer.as_ref().unwrap().cancelled
            ));
            assert!(!replacement.load(Ordering::Acquire));
            let notice = &view.endpoints[0].toasts.entries.back().unwrap().1;
            assert_eq!(notice.title, "Remote cleanup failed");
            assert_eq!(notice.body.as_deref(), Some("No path was pasted. Temporary files may remain on the original host (Original host)."));
        });
        peer.sentinel(&view, cx);
    }

    #[gpui::test]
    fn backend_cleanup_failure_survives_host_switch_without_disclosing_diagnostics(
        cx: &mut TestAppContext,
    ) {
        let (fixture, cx) = cx.add_window_view(fixture);
        let view = fixture.read_with(cx, |fixture, _| fixture.view.clone().unwrap());
        let mut peer = Peer::new();
        view.update(cx, |view, cx| {
            peer.prepare(view);
            view.endpoints[0].label = "Original\n host".into();
            view.start_file_transfer_with(
                InputTarget::Pane("w1:p1".into()),
                vec!["/private/source".into()],
                |_, _, _, _| {
                    Err(herdr_client::Error::UploadCleanup {
                        source: Box::new(herdr_client::Error::UploadIo(std::io::Error::other(
                            "/private/source secret",
                        ))),
                        cleanup: Box::new(herdr_client::Error::UploadIo(std::io::Error::other(
                            "/private/remote secret",
                        ))),
                    })
                },
                |_, _| panic!("backend already attempted cleanup; no successful paths exist"),
                cx,
            );
            view.endpoints.push(crate::endpoint::Endpoint::new(
                "other".into(),
                "Other host".into(),
                ConnectTarget::Ssh {
                    target: "other.invalid".into(),
                    session: "default".into(),
                },
                true,
            ));
            view.selected_endpoint = 1;
            view.selection_epoch += 1;
            view.poll_file_transfer(cx);
            assert!(view.file_transfer.as_ref().unwrap().shown.2);
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.file_transfer.is_none());
            assert!(view.local_error.is_none());
            assert!(view.endpoints[0].toasts.entries.is_empty());
            let notices = &view.endpoints[1].toasts.entries;
            assert_eq!(notices.len(), 1);
            let notice = &notices.back().unwrap().1;
            assert_eq!(notice.title, "Remote cleanup failed");
            assert!(notice.visible);
            assert_eq!(notice.body.as_deref(), Some("No path was pasted. Temporary files may remain on the original host (Original host)."));
            for forbidden in ["/private", "secret", HOST, "other.invalid", "Other host"] {
                assert!(!notice.body.as_ref().unwrap().contains(forbidden));
            }
        });
        peer.sentinel(&view, cx);
    }
}
