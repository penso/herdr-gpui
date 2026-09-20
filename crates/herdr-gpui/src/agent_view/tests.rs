//! Headless tests of the real window, not a stand-in composer host.
#![allow(clippy::unwrap_used)]

use super::*;
use crate::{agent_mode::PaneTarget, state::ConnectionStatus};
use core::prelude::v1::test;
use herdr_client::{ClientHandle, ConnectOptions, ConnectTarget, protocol::*};
use std::{
    fs,
    os::unix::{fs::DirBuilderExt, net::UnixListener},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SNAPSHOT: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json");
const WELCOME: &str =
    include_str!("../../../herdr-protocol/tests/fixtures/endpoint-welcome-v1.json");
const WAIT: Duration = Duration::from_secs(3);

// macOS's default temporary directory can exceed sockaddr_un's path budget.
// Each test owns only this private directory and its exact socket, never a daemon.
struct SocketDirectory(PathBuf);

impl SocketDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = PathBuf::from("/tmp").join(format!(
            "ha-{:x}-{:x}-{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }

    fn socket(&self) -> PathBuf {
        self.0.join("client.sock")
    }
}

impl Drop for SocketDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cancelled_handle() -> ClientHandle {
    let directory = SocketDirectory::new();
    let client = herdr_client::connect(
        ConnectTarget::Socket(directory.socket()),
        ConnectOptions::default(),
    )
    .unwrap();
    client.handle.disconnect();
    client.handle
}

fn snapshot() -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(SNAPSHOT).unwrap();
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "w1:t2".into();
    tab.number = 2;
    tab.label = "second".into();
    tab.focused = false;
    snapshot.tabs.push(tab);
    for (id, tab) in [("w1:p2", "w1:t1"), ("w1:p3", "w1:t2")] {
        let mut pane = snapshot.panes[0].clone();
        pane.pane_id = id.into();
        pane.tab_id = tab.into();
        pane.focused = false;
        snapshot.panes.push(pane);
    }
    snapshot
}

fn surface(snapshot: &ClientShellSnapshot, width: u16, height: u16) -> PaneSurfaceFrame {
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: 1,
        frame: FrameData {
            width,
            height,
            cells: vec![
                CellData {
                    symbol: " ".into(),
                    fg: 0,
                    bg: 0,
                    modifier: 0,
                    skip: false,
                    hyperlink: None,
                };
                usize::from(width) * usize::from(height)
            ],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![PaneSurfacePane {
            pane_id: snapshot.focused_pane_id.clone().unwrap(),
            content_revision: 1,
            rect,
            inner_rect: rect,
            scrollbar_rect: None,
            scroll: None,
            focused: true,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 0,
            pixel_height: 0,
        }],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    }
}

fn install(view: &mut HerdrWindow, snapshot: ClientShellSnapshot, cx: &mut Context<HerdrWindow>) {
    view.options.surface_size.cols = view.options.surface_size.cols.max(1);
    view.options.surface_size.rows = view.options.surface_size.rows.max(1);
    view.set_surface(
        Some(Arc::new(surface(
            &snapshot,
            view.options.surface_size.cols,
            view.options.surface_size.rows,
        ))),
        cx,
    );
    view.live.snapshot = Some(Arc::new(snapshot));
    view.live.status = ConnectionStatus::Connected;
    view.live.activation = None;
    let endpoint = &mut view.endpoints[view.selected_endpoint];
    endpoint.set_fixture_surface_active();
    endpoint.live = view.live.clone();
    *endpoint.connection.inbox.lock().unwrap() = view.live.clone();
    view.sync_composer(cx);
    cx.notify();
}

// Layout may resize the terminal after toggling the composer. Model the next
// daemon surface before exercising input, rather than weakening input_ready().
fn refresh_fixture_surface(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    let snapshot = view.live.snapshot.as_deref().unwrap().clone();
    install(view, snapshot, cx);
}

fn settle_fixture_surface(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        for _ in 0..4 {
            window.refresh();
            window.draw(cx).clear();
            view.update(cx, refresh_fixture_surface);
            window.refresh();
            window.draw(cx).clear();
            if view.read(cx).input_ready() {
                return;
            }
        }
        panic!("fixture surface did not match the settled terminal viewport");
    });
}

fn tab(id: &str) -> TabTarget {
    TabTarget {
        endpoint_id: crate::endpoint::LOCAL.into(),
        boot_id: "boot-v1".into(),
        tab_id: id.into(),
    }
}

fn new_view(
    handle: ClientHandle,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> HerdrWindow {
    let mut view = HerdrWindow::new(
        ConnectTarget::Socket("/unused-agent-view-test.sock".into()),
        window,
        cx,
        true,
    );
    view.endpoints[view.selected_endpoint].connection.handle = Some(handle);
    install(&mut view, snapshot(), cx);
    view
}

fn edit(view: &HerdrWindow, text: &str, window: &mut Window, cx: &mut Context<HerdrWindow>) {
    view.composer.update(cx, |editor, cx| {
        editor.replace_text_in_range(None, text, window, cx);
    });
}

fn assert_only_resize_error(view: &HerdrWindow) {
    assert!(
        view.local_error
            .as_deref()
            .is_none_or(|error| error.starts_with("Resize:")),
        "unexpected input/navigation error: {:?}",
        view.local_error,
    );
}

fn focus_pane(view: &mut HerdrWindow, pane_id: &str, cx: &mut Context<HerdrWindow>) {
    let mut snapshot = view.live.snapshot.as_deref().unwrap().clone();
    let pane = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == pane_id)
        .unwrap();
    let tab_id = pane.tab_id.clone();
    snapshot.focused_pane_id = Some(pane_id.into());
    snapshot.focused_tab_id = Some(tab_id.clone());
    snapshot.workspaces[0].active_tab_id = tab_id.clone();
    for pane in &mut snapshot.panes {
        pane.focused = pane.pane_id == pane_id;
    }
    for tab in &mut snapshot.tabs {
        tab.focused = tab.tab_id == tab_id;
    }
    snapshot.revision += 1;
    install(view, snapshot, cx);
}

#[gpui::test]
fn inactive_tab_context_menu_and_keyboard_modes_do_not_navigate(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    let before = view.update(cx, |view, _| view.live.snapshot.clone());
    let bounds = cx.debug_bounds("tab-w1:t2").unwrap();
    cx.simulate_mouse_down(bounds.center(), MouseButton::Right, Modifiers::default());
    view.update(cx, |view, _| {
        assert!(matches!(&view.menu.page, Some(crate::menu::Page::TabMode(target)) if *target == tab("w1:t2")));
        assert_eq!(view.live.snapshot, before);
        assert_only_resize_error(view);
        assert!(view.navigation_fence.is_none());
    });
    cx.simulate_keystrokes("down enter");
    view.update_in(cx, |view, window, cx| {
        assert!(view.menu.page.is_none());
        assert_eq!(
            view.agent_modes
                .mode(crate::endpoint::LOCAL, "boot-v1", "w1:t2"),
            ViewMode::Agent
        );
        assert_eq!(
            view.agent_modes
                .mode(crate::endpoint::LOCAL, "boot-v1", "w1:t1"),
            ViewMode::Terminal
        );
        assert!(view.focus.is_focused(window));
        assert!(view.composer_target.is_none());
        assert_eq!(view.live.snapshot, before);
        assert_only_resize_error(view);
        view.open_tab_menu(tab("w1:t1"), point(px(200.), px(40.)), window, cx);
    });
    cx.simulate_keystrokes("down enter");
    view.update_in(cx, |view, window, cx| {
        assert!(view.composer.focus_handle(cx).is_focused(window));
        assert_eq!(view.composer_target.as_ref().unwrap().pane_id, "w1:p1");
        view.open_tab_menu(tab("w1:t2"), point(px(200.), px(40.)), window, cx);
    });
    cx.simulate_keystrokes("up enter");
    view.update_in(cx, |view, window, cx| {
        assert_eq!(
            view.agent_modes
                .mode(crate::endpoint::LOCAL, "boot-v1", "w1:t2"),
            ViewMode::Terminal
        );
        assert!(view.composer.focus_handle(cx).is_focused(window));
        assert_eq!(view.live.snapshot, before);
        view.open_tab_menu(tab("w1:t1"), point(px(200.), px(40.)), window, cx);
    });
    cx.simulate_keystrokes("up enter");
    view.update_in(cx, |view, window, _| {
        assert_eq!(
            view.agent_modes
                .mode(crate::endpoint::LOCAL, "boot-v1", "w1:t1"),
            ViewMode::Terminal
        );
        assert!(view.focus.is_focused(window));
        assert_eq!(view.live.snapshot, before);
        assert_only_resize_error(view);
    });
}

#[gpui::test]
fn real_window_composer_isolates_keys_paste_and_ime(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        crate::bind_keys(cx);
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx)
    });
    cx.simulate_input("a\u{1f600}");
    cx.simulate_keystrokes("left right end enter shift-enter");
    cx.simulate_input("last");
    cx.simulate_keystrokes("cmd-a cmd-c cmd-x cmd-v cmd-t");
    let editor = view.update(cx, |view, cx| {
        assert_eq!(view.composer.read(cx).text(), "a\u{1f600}\n\nlast");
        assert_eq!(view.input_probe.keys, 0);
        assert_eq!(view.input_probe.text, 0);
        assert_eq!(view.input_probe.actions, 0);
        assert_only_resize_error(view);
        view.composer.clone()
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.replace_and_mark_text_in_range(None, "\u{1f680}", None, window, cx);
        assert!(editor.is_composing());
    });
    cx.simulate_keystrokes("cmd-enter");
    view.update(cx, |view, cx| {
        assert!(view.composer_notice.is_none(), "preedit must not submit");
        assert!(view.marked.is_empty(), "terminal IME state stays untouched");
        assert_eq!(view.composer.read(cx).draft().text(), "a\u{1f600}\n\nlast");
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.replace_text_in_range(None, "\u{1f680}", window, cx);
        assert!(!editor.is_composing());
    });
    settle_fixture_surface(&view, cx);
    cx.simulate_keystrokes("cmd-enter");
    view.update(cx, |view, cx| {
        assert_eq!(view.composer.read(cx).text(), "a\u{1f600}\n\nlast\u{1f680}");
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Input not queued:")
        );
        assert_only_resize_error(view);
        assert_eq!(view.input_probe.keys, 0);
        assert_eq!(view.input_probe.text, 0);
    });
}

#[gpui::test]
fn drafts_and_selections_survive_pane_tab_and_mode_switches(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        view.set_tab_mode(&tab("w1:t2"), ViewMode::Agent, window, cx);
        edit(view, "first \u{1f600}", window, cx);
    });
    cx.simulate_keystrokes("shift-left");
    view.update_in(cx, |view, window, cx| {
        let first = view.composer.read(cx).draft();
        view.composer.update(cx, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "uncommitted", None, window, cx);
        });
        focus_pane(view, "w1:p2", cx);
        assert!(view.composer.read(cx).text().is_empty());
        edit(view, "second pane", window, cx);
        let second = view.composer.read(cx).draft();
        focus_pane(view, "w1:p3", cx);
        edit(view, "other tab", window, cx);
        let third = view.composer.read(cx).draft();
        view.set_tab_mode(&tab("w1:t2"), ViewMode::Terminal, window, cx);
        assert!(view.composer_target.is_none());
        view.set_tab_mode(&tab("w1:t2"), ViewMode::Agent, window, cx);
        assert_eq!(view.composer.read(cx).draft(), third);
        focus_pane(view, "w1:p2", cx);
        assert_eq!(view.composer.read(cx).draft(), second);
        focus_pane(view, "w1:p1", cx);
        assert_eq!(view.composer.read(cx).draft(), first);
        assert!(!view.composer.read(cx).is_composing());
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Terminal, window, cx);
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        assert_eq!(view.composer.read(cx).draft(), first);
    });
}

#[gpui::test]
fn identical_cross_host_targets_preserve_independent_drafts_and_reject_old_native_input(
    cx: &mut TestAppContext,
) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut remote = crate::endpoint::Endpoint::new(
                "remote".into(),
                "Remote".into(),
                ConnectTarget::Socket("/unused-remote-agent-test.sock".into()),
                false,
            );
            remote.connection.handle = view.endpoints[0].connection.handle.clone();
            view.endpoints.push(remote);
            install(view, snapshot(), cx);
            window.focus(&view.focus);
        });
        let mut old = captured_terminal_handler(&view, cx);
        old.replace_and_mark_text_in_range(None, "local preedit", None, window, cx);
        assert_eq!(view.read(cx).marked, "local preedit");
        view.update(cx, |view, cx| {
            view.selected_endpoint = 1;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            view.marked = "remote preedit".into();
        });
        old.replace_text_in_range(None, "wrong host", window, cx);
        old.unmark_text(window, cx);
        assert_eq!(view.read(cx).marked, "remote preedit");
        assert_eq!(view.read(cx).input_probe.text, 0);
        view.update(cx, |view, cx| {
            view.selected_endpoint = 0;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            view.marked = "returned local preedit".into();
        });
        old.unmark_text(window, cx);
        assert_eq!(view.read(cx).marked, "returned local preedit");
        let mut previous_generation = captured_terminal_handler(&view, cx);
        view.update(cx, |view, _| view.endpoints[0].generation += 1);
        previous_generation.unmark_text(window, cx);
        assert_eq!(view.read(cx).marked, "returned local preedit");
        view.update(cx, |view, cx| {
            view.selected_endpoint = 1;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            let remote = TabTarget {
                endpoint_id: "remote".into(),
                ..tab("w1:t1")
            };
            // A stale local mode callback must not change the same tab on remote.
            view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
            assert!(!view.agent_tab_active());
            view.set_tab_mode(&remote, ViewMode::Agent, window, cx);
            edit(view, "remote draft", window, cx);
            let remote_draft = view.composer.read(cx).draft();
            view.composer.update(cx, |editor, cx| {
                editor.replace_and_mark_text_in_range(None, "uncommitted", None, window, cx);
            });
            view.selected_endpoint = 0;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
            assert!(view.composer.read(cx).text().is_empty());
            edit(view, "local draft", window, cx);
            let local_draft = view.composer.read(cx).draft();
            view.selected_endpoint = 1;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            assert_eq!(view.composer.read(cx).draft(), remote_draft);
            assert!(!view.composer.read(cx).is_composing());
            let mut rebooted = snapshot();
            rebooted.boot_id = "remote-reboot".into();
            install(view, rebooted, cx);
            assert!(view.composer_target.is_none());
            assert_eq!(view.drafts.len(), 1);
            view.selected_endpoint = 0;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            assert_eq!(view.composer.read(cx).draft(), local_draft);
            view.selected_endpoint = 1;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            view.set_tab_mode(&remote, ViewMode::Agent, window, cx);
            edit(view, "removed endpoint draft", window, cx);
            view.selected_endpoint = 0;
            view.selection_epoch += 1;
            install(view, snapshot(), cx);
            view.endpoints.pop();
            view.sync_composer(cx);
            assert!(view.drafts.is_empty());
            assert_eq!(view.composer.read(cx).draft(), local_draft);
            assert_eq!(
                view.agent_modes.mode("remote", "boot-v1", "w1:t1"),
                ViewMode::Terminal
            );
        });
    });
}

#[gpui::test]
fn send_requires_active_current_viewport_but_local_editing_does_not(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        refresh_fixture_surface(view, cx);
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "local draft", window, cx);
        assert!(view.input_ready());
        view.options.surface_size.cols += 1;
        view.sync_composer(cx);
        assert!(view.composer_editable());
        edit(view, " still editable", window, cx);
        let draft = view.composer.read(cx).draft();
        view.submit_composer(cx);
        assert_eq!(view.composer.read(cx).draft(), draft);
        assert_eq!(
            view.composer_notice.as_deref(),
            Some("Waiting for the selected pane's current surface.")
        );
        refresh_fixture_surface(view, cx);
        view.live.activation = Some(crate::state::SurfaceActivation {
            request: "pending-activation".into(),
            boot: "boot-v1".into(),
            revision: None,
            failed: false,
            focus: None,
            active: true,
        });
        view.sync_composer(cx);
        assert!(!view.input_ready());
        assert!(view.composer_editable());
        view.submit_composer(cx);
        assert_eq!(view.composer.read(cx).draft(), draft);
        assert_eq!(
            view.composer_notice.as_deref(),
            Some("Waiting for the selected pane's current surface.")
        );
    });
}

#[gpui::test]
fn popup_and_authoritative_inbox_changes_preserve_unsent_draft(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "keep me", window, cx);
        refresh_fixture_surface(view, cx);
        let draft = view.composer.read(cx).draft();
        let baseline = view.live.clone();
        let mut popup = baseline.surface.as_deref().unwrap().clone();
        popup.popup = Some(Box::new(ClientShellPopupSurface {
            terminal_id: "popup".into(),
            title: "Popup".into(),
            width: None,
            height: None,
            frame: popup.frame.clone(),
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            pixel_width: 0,
            pixel_height: 0,
        }));
        view.set_surface(Some(Arc::new(popup.clone())), cx);
        view.sync_composer(cx);
        edit(view, "ignored while popup active", window, cx);
        view.submit_composer(cx);
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Popup active.")
        );
        assert_eq!(view.composer.read(cx).draft(), draft);
        view.set_surface(baseline.surface.clone(), cx);
        view.sync_composer(cx);
        assert!(view.composer_block_reason().is_none());

        // The rendered clone is valid throughout; only the authoritative inbox moves.
        for change in 0..5 {
            let mut changed = baseline.clone();
            match change {
                0 => changed.surface = Some(Arc::new(popup.clone())),
                1 => {
                    Arc::make_mut(changed.snapshot.as_mut().unwrap()).focused_pane_id =
                        Some("w1:p2".into())
                }
                2 => Arc::make_mut(changed.snapshot.as_mut().unwrap()).boot_id = "next-boot".into(),
                3 => changed.status = ConnectionStatus::Disconnected,
                _ => {
                    changed.activation = Some(crate::state::SurfaceActivation {
                        request: "pending-activation".into(),
                        boot: "boot-v1".into(),
                        revision: None,
                        failed: false,
                        focus: None,
                        active: true,
                    })
                }
            }
            *view.endpoints[view.selected_endpoint]
                .connection
                .inbox
                .lock()
                .unwrap() = changed;
            view.submit_composer(cx);
            assert_eq!(view.composer.read(cx).draft(), draft);
            assert!(
                view.composer_notice
                    .as_deref()
                    .unwrap()
                    .contains("recipient or popup changed")
            );
        }
        *view.endpoints[view.selected_endpoint]
            .connection
            .inbox
            .lock()
            .unwrap() = baseline;
        let inbox = view.endpoints[view.selected_endpoint]
            .connection
            .inbox
            .clone();
        let guard = inbox.lock().unwrap();
        view.submit_composer(cx);
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .contains("state is updating")
        );
        assert_eq!(view.composer.read(cx).draft(), draft);
        drop(guard);
        view.submit_composer(cx);
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Input not queued:")
        );
        assert_eq!(view.composer.read(cx).draft(), draft);
    });
}

#[gpui::test]
fn reconnect_preserves_same_boot_drafts_but_deletions_and_new_boot_prune(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        view.set_tab_mode(&tab("w1:t2"), ViewMode::Agent, window, cx);
        edit(view, "first", window, cx);
        focus_pane(view, "w1:p2", cx);
        edit(view, "deleted pane", window, cx);
        focus_pane(view, "w1:p3", cx);
        edit(view, "deleted tab", window, cx);
        let saved = view.live.snapshot.as_deref().unwrap().clone();
        let old_inbox = view.endpoints[view.selected_endpoint]
            .connection
            .inbox
            .clone();
        // Invalid geometry fails synchronously before creating a socket worker.
        // Exercise the actual reconnect/reset path without discovering a daemon.
        view.options.surface_size.cols = 0;
        view.reconnect(cx);
        assert!(!Arc::ptr_eq(
            &old_inbox,
            &view.endpoints[view.selected_endpoint].connection.inbox
        ));
        assert!(view.composer_target.is_none());
        assert_eq!(view.drafts.len(), 3);
        *old_inbox.lock().unwrap() = crate::LiveState::default();
        assert_eq!(view.drafts.len(), 3);
        install(view, saved, cx);
        assert_eq!(view.composer.read(cx).text(), "deleted tab");
        assert_eq!(
            view.agent_modes
                .mode(crate::endpoint::LOCAL, "boot-v1", "w1:t2"),
            ViewMode::Agent
        );
        focus_pane(view, "w1:p1", cx);
        assert_eq!(view.composer.read(cx).text(), "first");
        let mut reduced = view.live.snapshot.as_deref().unwrap().clone();
        reduced.panes.retain(|pane| pane.pane_id == "w1:p1");
        reduced.tabs.retain(|tab| tab.tab_id == "w1:t1");
        reduced.revision += 1;
        install(view, reduced.clone(), cx);
        assert!(view.drafts.is_empty());
        assert_eq!(view.composer.read(cx).text(), "first");
        assert_eq!(
            view.agent_modes
                .mode(crate::endpoint::LOCAL, "boot-v1", "w1:t2"),
            ViewMode::Terminal
        );
        reduced.boot_id = "new-boot".into();
        install(view, reduced, cx);
        assert!(view.composer_target.is_none());
        assert!(view.composer.read(cx).text().is_empty());
        assert!(view.drafts.is_empty());
        assert_eq!(
            view.agent_modes
                .mode(crate::endpoint::LOCAL, "new-boot", "w1:t1"),
            ViewMode::Terminal
        );
    });
}

#[gpui::test]
fn stale_submit_cannot_send_an_identical_draft_after_pane_switch(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "identical prompt", window, cx);
        focus_pane(view, "w1:p2", cx);
        edit(view, "identical prompt", window, cx);
        focus_pane(view, "w1:p1", cx);
        let original = view.composer.read(cx).draft();
        let revision = view.composer.read(cx).revision();
        view.composer
            .update(cx, |_, cx| cx.emit(composer::Submit { revision }));
        // Both operations share one update so the subscription effect cannot run
        // until after the recipient has changed, even though the text is equal.
        focus_pane(view, "w1:p2", cx);
        assert_eq!(view.composer.read(cx).draft(), original);
        assert_ne!(view.composer.read(cx).revision(), revision);
    });
    cx.run_until_parked();
    settle_fixture_surface(&view, cx);
    view.update(cx, |view, cx| {
        assert_eq!(view.composer_target.as_ref().unwrap().pane_id, "w1:p2");
        assert_eq!(view.composer.read(cx).text(), "identical prompt");
        assert!(
            view.composer_notice.is_none(),
            "stale event reached submission"
        );
        assert_only_resize_error(view);
        refresh_fixture_surface(view, cx);
        let revision = view.composer.read(cx).revision();
        view.composer
            .update(cx, |_, cx| cx.emit(composer::Submit { revision }));
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        // A current event must reach the canceled queue: this also proves the
        // previous no-op was revision fencing, not a missing subscription.
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Input not queued:")
        );
        assert_eq!(view.composer.read(cx).text(), "identical prompt");
    });
}

#[gpui::test]
fn navigation_fence_waits_for_focus_change_not_unrelated_revision(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "first pane", window, cx);
        focus_pane(view, "w1:p2", cx);
        edit(view, "second pane", window, cx);
        focus_pane(view, "w1:p1", cx);
        let snapshot = view.live.snapshot.as_deref().unwrap().clone();
        view.navigation_fence = Some(NavigationFence::new(
            &snapshot,
            "navigation-test".into(),
            Some(NavigationTarget::Pane("w1:p2".into())),
        ));
        view.sync_composer(cx);
        edit(view, "must not edit while navigating", window, cx);
        view.submit_composer(cx);
        assert_eq!(view.composer.read(cx).text(), "first pane");
        assert_eq!(
            view.composer_notice.as_deref(),
            Some("Waiting for Herdr to confirm navigation.")
        );

        let mut unrelated = snapshot;
        unrelated.revision += 1;
        unrelated.tabs[0].label = "renamed without navigation".into();
        install(view, unrelated, cx);
        assert!(view.navigation_fence.is_some());
        view.submit_composer(cx);
        assert_eq!(
            view.composer_notice.as_deref(),
            Some("Waiting for Herdr to confirm navigation.")
        );
        assert_eq!(view.composer.read(cx).text(), "first pane");

        focus_pane(view, "w1:p3", cx);
        assert!(
            view.navigation_fence.is_some(),
            "an unrelated focus change is not the captured goal"
        );
        focus_pane(view, "w1:p1", cx);
        assert!(view.navigation_fence.is_some());
        assert_eq!(view.composer.read(cx).text(), "first pane");
        focus_pane(view, "w1:p2", cx);
        assert!(view.navigation_fence.is_none());
        assert!(view.composer_block_reason().is_none());
        assert_eq!(view.composer.read(cx).text(), "second pane");
        view.submit_composer(cx);
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Input not queued:")
        );
        assert_eq!(view.composer.read(cx).text(), "second pane");
        focus_pane(view, "w1:p1", cx);
        assert_eq!(view.composer.read(cx).text(), "first pane");
    });
}

#[gpui::test]
fn surface_gap_preserves_composer_focus_ime_and_local_native_input(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    let (draft, visible, revision) = view.update_in(cx, |view, window, cx| {
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "draft ", window, cx);
        view.composer.update(cx, |editor, cx| {
            editor.replace_and_mark_text_in_range(None, "\u{1f680}", Some(0..2), window, cx);
        });
        let editor = view.composer.read(cx);
        (editor.draft(), editor.text().to_owned(), editor.revision())
    });
    view.update(cx, |view, cx| {
        let mut next = view.live.snapshot.as_deref().unwrap().clone();
        next.revision += 1;
        view.endpoints[view.selected_endpoint]
            .connection
            .inbox
            .lock()
            .unwrap()
            .apply(herdr_client::ClientEvent::Snapshot(Arc::new(next)));
        let next = view.endpoints[view.selected_endpoint]
            .connection
            .take_update()
            .unwrap();
        assert!(next.surface.is_none());
        view.set_surface(next.surface.clone(), cx);
        view.live = next;
        view.sync_composer(cx);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("composer-editor-viewport").is_some());
    view.update_in(cx, |view, window, cx| {
        assert!(view.composer.focus_handle(cx).is_focused(window));
        let editor = view.composer.read(cx);
        assert!(editor.is_composing());
        assert_eq!(editor.draft(), draft);
        assert_eq!(editor.text(), visible);
        assert_eq!(editor.revision(), revision);
        assert!(view.composer_editable());
        view.submit_composer(cx);
        assert_eq!(
            view.composer_notice.as_deref(),
            Some("Waiting for a focused pane and its terminal surface.")
        );
    });
    // Exercise the newly painted native registration, not a direct editor call.
    cx.simulate_input("committed");
    cx.simulate_input(" locally");
    view.update_in(cx, |view, window, cx| {
        assert!(view.composer.focus_handle(cx).is_focused(window));
        assert_eq!(view.composer.read(cx).text(), "draft committed locally");
        assert!(!view.composer.read(cx).is_composing());
        assert!(view.marked.is_empty());
        assert_eq!(view.input_probe.text, 0);
        assert_eq!(view.input_probe.keys, 0);
        assert_only_resize_error(view);
        view.submit_composer(cx);
        assert_eq!(
            view.composer_notice.as_deref(),
            Some("Waiting for a focused pane and its terminal surface.")
        );
        let snapshot = view.live.snapshot.as_deref().unwrap().clone();
        install(view, snapshot, cx);
        assert!(view.composer_block_reason().is_none());
        view.submit_composer(cx);
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Input not queued:")
        );
        assert_eq!(view.composer.read(cx).text(), "draft committed locally");
    });
}

fn captured_terminal_handler(view: &Entity<HerdrWindow>, cx: &App) -> impl InputHandler + use<> {
    let state = view.read(cx);
    crate::input::terminal_handler(
        state.bounds,
        view.clone(),
        crate::input::TerminalBinding::new(
            &state.endpoints[state.selected_endpoint].id,
            state.live.snapshot.as_deref(),
            state.live.surface.as_deref(),
        ),
        state.terminal_input_epoch,
        state.selection_epoch,
        state.endpoints[state.selected_endpoint].generation,
    )
}

#[gpui::test]
fn stale_terminal_registration_cannot_mutate_after_mode_round_trip_or_pane_change(
    cx: &mut TestAppContext,
) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    cx.update(|window, cx| {
        view.update(cx, refresh_fixture_surface);
        for change_pane in [false, true] {
            let mut old = captured_terminal_handler(&view, cx);
            old.replace_and_mark_text_in_range(None, "old preedit", None, window, cx);
            assert_eq!(
                view.read(cx).marked,
                "old preedit",
                "registration must initially be usable"
            );
            view.update(cx, |view, cx| {
                if change_pane {
                    focus_pane(view, "w1:p2", cx);
                } else {
                    view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
                    edit(view, "local draft", window, cx);
                    // Even bypassing the guarded adapter, the root's native
                    // callbacks cannot accept input while composer owns focus.
                    view.replace_text_in_range(None, "wrong recipient", window, cx);
                    view.replace_and_mark_text_in_range(None, "wrong recipient", None, window, cx);
                    assert!(view.marked.is_empty());
                    assert!(view.selected_text_range(true, window, cx).is_none());
                    assert!(view.text_for_range(0..100, &mut None, window, cx).is_none());
                    assert_eq!(view.composer.read(cx).text(), "local draft");
                }
            });
            old.replace_text_in_range(None, "stale while composer focused", window, cx);
            assert_eq!(view.read(cx).input_probe.text, 0);
            if !change_pane {
                view.update(cx, |view, cx| {
                    view.set_tab_mode(&tab("w1:t1"), ViewMode::Terminal, window, cx)
                });
            }
            assert!(view.read(cx).focus.is_focused(window));
            let mut current = captured_terminal_handler(&view, cx);
            current.replace_and_mark_text_in_range(None, "current preedit", None, window, cx);
            assert_eq!(view.read(cx).marked, "current preedit");
            old.replace_text_in_range(None, "stale commit", window, cx);
            old.replace_and_mark_text_in_range(None, "stale preedit", None, window, cx);
            old.unmark_text(window, cx);
            assert_eq!(view.read(cx).marked, "current preedit");
            assert_eq!(view.read(cx).input_probe.text, 0);
            assert!(old.selected_text_range(true, window, cx).is_none());
            assert!(old.marked_text_range(window, cx).is_none());
            assert!(old.text_for_range(0..100, &mut None, window, cx).is_none());
            assert!(old.bounds_for_range(0..1, window, cx).is_none());
            assert!(current.marked_text_range(window, cx).is_some());
            assert_only_resize_error(view.read(cx));
        }
    });
}

#[gpui::test]
fn opening_menu_disables_retained_composer_handler_before_redraw(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "keep draft", window, cx);
    });
    for tab_menu in [false, true] {
        cx.update(|window, cx| {
            let editor = view.read(cx).composer.clone();
            // Epoch rejection is covered by composer unit tests. This retained
            // native adapter deliberately checks only focus so disabling the
            // editor itself must also reject callbacks before the next paint.
            let mut old = crate::input_guard::GuardedInputHandler::new(
                Bounds::default(),
                editor.clone(),
                |editor: &composer::Composer, window, cx| {
                    editor.focus_handle(cx).is_focused(window)
                },
            );
            old.replace_and_mark_text_in_range(None, "preedit", None, window, cx);
            assert!(editor.read(cx).is_composing());
            let draft = editor.read(cx).draft();
            view.update(cx, |view, cx| {
                if tab_menu {
                    view.open_tab_menu(tab("w1:t2"), point(px(200.), px(40.)), window, cx);
                } else {
                    view.open_menu(window, cx);
                }
            });
            assert!(!editor.read(cx).is_composing());
            assert_eq!(editor.read(cx).draft(), draft);
            // Defeat the focus predicate too: no draw or sync_composer occurs
            // between opening the menu and these late platform callbacks.
            window.focus(&editor.focus_handle(cx));
            old.replace_text_in_range(None, "late commit", window, cx);
            old.replace_and_mark_text_in_range(None, "late preedit", None, window, cx);
            old.unmark_text(window, cx);
            assert_eq!(editor.read(cx).draft(), draft);
            assert!(!editor.read(cx).is_composing());
            assert!(old.selected_text_range(true, window, cx).is_none());
            assert_eq!(view.read(cx).input_probe.text, 0);
            view.update(cx, |view, cx| {
                view.menu.page = None;
                view.sync_composer(cx);
                window.focus(&view.composer.focus_handle(cx));
            });
        });
    }
}

#[gpui::test]
fn only_correlated_navigation_failure_releases_latest_fence(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        refresh_fixture_surface(view, cx);
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "unsent draft", window, cx);
        let draft = view.composer.read(cx).draft();
        for request_id in ["older", "latest"] {
            view.endpoints[view.selected_endpoint]
                .connection
                .inbox
                .lock()
                .unwrap()
                .track_request(request_id.into());
            view.navigation_fence = Some(NavigationFence::new(
                view.live.snapshot.as_deref().unwrap(),
                request_id.into(),
                Some(NavigationTarget::Pane("w1:p2".into())),
            ));
        }
        view.sync_composer(cx);
        for request_id in ["older", "unrelated", "latest"] {
            view.endpoints[view.selected_endpoint]
                .connection
                .inbox
                .lock()
                .unwrap()
                .apply(herdr_client::ClientEvent::Response {
                    request_id: request_id.into(),
                    response: serde_json::json!({"error": "navigation rejected"}),
                });
            // Repeated unrelated failures need not dirty the mailbox when the
            // displayed error and tracked request status are both unchanged.
            if let Some(next) = view.endpoints[view.selected_endpoint]
                .connection
                .take_update()
            {
                view.live = next;
            }
            view.sync_composer(cx);
            assert_eq!(view.composer.read(cx).draft(), draft);
            if request_id != "latest" {
                assert!(view.navigation_fence.is_some());
                assert!(!view.composer_editable());
                view.submit_composer(cx);
                assert_eq!(
                    view.composer_notice.as_deref(),
                    Some("Waiting for Herdr to confirm navigation.")
                );
            }
        }
        assert!(view.navigation_fence.is_none());
        assert!(view.composer_editable());
        assert_eq!(
            view.live.request_status("latest"),
            Some(crate::state::RequestStatus::Failed)
        );
        view.submit_composer(cx);
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Input not queued:")
        );
        assert_eq!(view.composer.read(cx).draft(), draft);
    });
}

#[gpui::test]
fn explicit_new_tab_button_is_allowed_but_composer_keyboard_shortcut_is_not(
    cx: &mut TestAppContext,
) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    let draft = view.update_in(cx, |view, window, cx| {
        crate::bind_keys(cx);
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "keep while creating tab", window, cx);
        view.composer.read(cx).draft()
    });
    cx.simulate_keystrokes("cmd-t");
    settle_fixture_surface(&view, cx);
    view.update_in(cx, |view, window, cx| {
        assert!(view.composer.focus_handle(cx).is_focused(window));
        assert_eq!(view.input_probe.actions, 0);
        assert_only_resize_error(view);
        assert_eq!(view.composer.read(cx).draft(), draft);
        // Suppress the already exercised canceled resize retry so the next
        // redraw cannot overwrite the button's specific command error.
        view.last_queued_options = Some(view.options);
    });
    // The plus button is right-aligned in the same row as the rendered tabs.
    let tab_bounds = cx.debug_bounds("tab-w1:t1").unwrap();
    let position = cx.update(|window, _| {
        point(
            window.viewport_size().width - px(16.),
            tab_bounds.center().y,
        )
    });
    cx.simulate_click(position, Modifiers::default());
    view.update(cx, |view, cx| {
        assert_eq!(view.input_probe.actions, 1);
        assert!(
            view.local_error
                .as_deref()
                .unwrap()
                .starts_with("tab.create:"),
            "{:?}",
            view.local_error
        );
        assert_eq!(view.composer.read(cx).draft(), draft);
        assert_eq!(view.composer_target.as_ref().unwrap().pane_id, "w1:p1");
        assert!(
            view.navigation_fence.is_none(),
            "failed enqueue must not create a fence"
        );
    });
}

#[gpui::test]
fn bottom_right_menu_stays_in_viewport_and_composer_shrinks_terminal(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    for viewport in [size(px(800.), px(600.)), size(px(480.), px(360.))] {
        cx.simulate_resize(viewport);
        cx.run_until_parked();
        let terminal_before = view.update(cx, |view, _| view.bounds);
        view.update_in(cx, |view, window, cx| {
            view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        });
        let composer = cx.debug_bounds("agent-composer").unwrap();
        let editor = cx.debug_bounds("composer-editor-viewport").unwrap();
        let terminal_after = view.update(cx, |view, _| view.bounds);
        assert!(terminal_after.size.height > px(0.));
        assert!(terminal_after.size.height < terminal_before.size.height);
        assert_eq!(terminal_after.size.width, terminal_before.size.width);
        assert!(terminal_after.bottom() <= composer.top());
        assert!(editor.size.width > px(0.) && editor.size.height > px(0.));
        assert!(editor.left() >= composer.left() && editor.right() <= composer.right());
        assert!(editor.top() >= composer.top() && editor.bottom() <= composer.bottom());
        assert!(composer.right() <= viewport.width && composer.bottom() <= viewport.height);
        view.update_in(cx, |view, window, cx| {
            view.open_tab_menu(
                tab("w1:t2"),
                point(viewport.width - px(1.), viewport.height - px(1.)),
                window,
                cx,
            );
        });
        let menu = cx.debug_bounds("tab-mode-menu").unwrap();
        assert!(menu.size.width > px(0.) && menu.size.height > px(0.));
        assert!(menu.left() >= px(0.) && menu.top() >= px(0.));
        assert!(menu.right() <= viewport.width && menu.bottom() <= viewport.height);
        for selector in ["tab-mode-terminal", "tab-mode-agent"] {
            let row = cx.debug_bounds(selector).unwrap();
            assert!(row.left() >= menu.left() && row.right() <= menu.right());
            assert!(row.top() >= menu.top() && row.bottom() <= menu.bottom());
        }
        cx.simulate_keystrokes("escape");
        view.update_in(cx, |view, window, cx| {
            view.set_tab_mode(&tab("w1:t1"), ViewMode::Terminal, window, cx);
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn successful_submit_queues_one_paste_enter_batch_and_only_then_clears(cx: &mut TestAppContext) {
    let directory = SocketDirectory::new();
    let listener = UnixListener::bind(directory.socket()).unwrap();
    listener.set_nonblocking(true).unwrap();
    let client = herdr_client::connect(
        ConnectTarget::Socket(directory.socket()),
        ConnectOptions::default(),
    )
    .unwrap();
    let deadline = Instant::now() + WAIT;
    let mut peer = loop {
        match listener.accept() {
            Ok((peer, _)) => break peer,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "mock client did not connect");
                std::thread::yield_now();
            }
            Err(error) => panic!("mock accept: {error}"),
        }
    };
    peer.set_nonblocking(false).unwrap();
    peer.set_read_timeout(Some(WAIT)).unwrap();
    peer.set_write_timeout(Some(WAIT)).unwrap();
    let hello: ClientMessage = read_message(&mut peer, MAX_FRAME_SIZE).unwrap();
    assert!(
        matches!(hello, ClientMessage::EndpointControl { kind, .. } if kind == endpoint::ENDPOINT_HELLO_KIND)
    );
    for (kind, data) in [
        (endpoint::ENDPOINT_WELCOME_KIND, WELCOME),
        (endpoint::ENDPOINT_SNAPSHOT_KIND, SNAPSHOT),
    ] {
        write_message(
            &mut peer,
            &ServerMessage::EndpointControl {
                kind: kind.into(),
                data: data.into(),
            },
            MAX_GRAPHICS_FRAME_SIZE,
        )
        .unwrap();
    }
    assert!(matches!(
        client.events.recv_timeout(WAIT).unwrap(),
        herdr_client::ClientEvent::Connected(_)
    ));
    assert!(matches!(
        client.events.recv_timeout(WAIT).unwrap(),
        herdr_client::ClientEvent::Snapshot(_)
    ));
    let (view, cx) = cx.add_window_view(|window, cx| new_view(client.handle.clone(), window, cx));
    view.update_in(cx, |view, window, cx| {
        refresh_fixture_surface(view, cx);
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        edit(view, "first\nsecond\n", window, cx);
        assert_eq!(view.composer.read(cx).text(), "first\nsecond\n");
        // An earlier saved entry must not resurrect a successfully queued prompt.
        let target: PaneTarget = view.composer_target.clone().unwrap();
        view.drafts
            .insert(target.clone(), view.composer.read(cx).draft());
        view.submit_composer(cx);
        assert!(view.composer.read(cx).text().is_empty());
        assert!(!view.drafts.contains_key(&target));
        assert!(
            view.composer_notice
                .as_deref()
                .unwrap()
                .starts_with("Input queued, not confirmed.")
        );
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Terminal, window, cx);
        view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx);
        assert!(view.composer.read(cx).text().is_empty());
    });
    let deadline = Instant::now() + WAIT;
    let message = loop {
        assert!(Instant::now() < deadline, "prompt batch did not arrive");
        let message: ClientMessage = read_message(&mut peer, MAX_FRAME_SIZE).unwrap();
        match message {
            ClientMessage::ClientShellResize { .. } | ClientMessage::ClientShellFocus { .. } => {}
            message => break message,
        }
    };
    assert_eq!(
        message,
        ClientMessage::ClientShellPaneInput {
            pane_id: "w1:p1".into(),
            events: vec![
                ClientPaneInputEvent::Paste("first\nsecond\n".into()),
                ClientPaneInputEvent::Key {
                    code: ClientKeyCode::Enter,
                    modifiers: 0,
                    kind: ClientKeyKind::Press,
                    repeat_count: 1,
                    shifted_codepoint: None,
                    generated_text: None,
                    tracks_release: false,
                    physical_key_id: None,
                    windows_record: None,
                },
            ],
        }
    );
    // A FIFO marker proves no extra input message was queued, without a sleep.
    client.handle.set_focus("boot-v1", false).unwrap();
    let deadline = Instant::now() + WAIT;
    let marker = loop {
        assert!(Instant::now() < deadline, "FIFO marker did not arrive");
        let message: ClientMessage = read_message(&mut peer, MAX_FRAME_SIZE).unwrap();
        match message {
            ClientMessage::ClientShellResize { .. } => {}
            message => break message,
        }
    };
    assert_eq!(marker, ClientMessage::ClientShellFocus { focused: false });
    client.handle.disconnect();
}

#[gpui::test]
fn direct_terminal_keys_and_paste_respect_navigation_and_latest_recipient(cx: &mut TestAppContext) {
    let handle = cancelled_handle();
    let (view, cx) = cx.add_window_view(|window, cx| new_view(handle, window, cx));
    view.update_in(cx, |view, window, cx| {
        refresh_fixture_surface(view, cx);
        window.focus(&view.focus);
        let snapshot = view.live.snapshot.clone().unwrap();
        view.navigation_fence = Some(NavigationFence::new(
            &snapshot,
            "pending".into(),
            Some(NavigationTarget::Pane("w1:p2".into())),
        ));
        let enter = KeyDownEvent {
            keystroke: Keystroke::parse("enter").unwrap(),
            is_held: false,
        };
        view.key_down(&enter, window, cx);
        assert_eq!(
            view.local_error.as_deref(),
            Some("Navigation pending; input was not sent.")
        );
        view.navigation_fence = None;
        let mut inbox = view.endpoints[view.selected_endpoint]
            .connection
            .inbox
            .lock()
            .unwrap();
        Arc::make_mut(inbox.snapshot.as_mut().unwrap()).focused_pane_id = Some("w1:p2".into());
        drop(inbox);
        for key in ["enter", "ctrl-c", "cmd-v"] {
            cx.write_to_clipboard(ClipboardItem::new_string("local test paste".into()));
            let event = KeyDownEvent {
                keystroke: Keystroke::parse(key).unwrap(),
                is_held: false,
            };
            view.local_error = None;
            view.key_down(&event, window, cx);
            assert_eq!(
                view.local_error.as_deref(),
                Some("Terminal target changed; input was not sent."),
                "{key}"
            );
        }
    });
}
