//! Native daemon-free agent fixture. Only the canceled transport is real.
use super::*;
use crate::agent_mode::{TabTarget, ViewMode};
use herdr_client::ClientHandle;
use std::{
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    sync::atomic::Ordering,
    time::{SystemTime, UNIX_EPOCH},
};

const SNAPSHOT: &str =
    include_str!("../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json");
const PROMPT: &str = "Review caf\u{e9}\n\nKeep the draft local";

// Creation, transport startup and removal run only on the background executor.
// An exclusive private directory makes the missing socket an owned test resource,
// rather than a guessed path that could belong to a personal daemon.
fn prepare() -> Result<(ClientHandle, PathBuf), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = PathBuf::from("/tmp").join(format!("hag-{:x}-{stamp:x}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|e| e.to_string())?;
    let result = herdr_client::connect(
        ConnectTarget::Socket(directory.join("client.sock")),
        ConnectOptions::default(),
    );
    match result {
        Ok(client) => {
            client.handle.disconnect();
            Ok((client.handle, directory))
        }
        Err(error) => {
            let _ = std::fs::remove_dir(&directory);
            Err(error.to_string())
        }
    }
}

fn tab(id: &str) -> TabTarget {
    TabTarget {
        endpoint_id: endpoint::LOCAL.into(),
        boot_id: "boot-v1".into(),
        tab_id: id.into(),
    }
}

fn snapshot() -> Result<ClientShellSnapshot, String> {
    let mut snapshot: ClientShellSnapshot =
        serde_json::from_str(SNAPSHOT).map_err(|e| e.to_string())?;
    let mut second = snapshot.tabs.first().ok_or("fixture missing tab")?.clone();
    second.tab_id = "w1:t2".into();
    second.number = 2;
    second.label = "second".into();
    second.focused = false;
    snapshot.tabs.push(second);
    for (id, tab_id) in [("w1:p2", "w1:t1"), ("w1:p3", "w1:t2")] {
        let mut pane = snapshot
            .panes
            .first()
            .ok_or("fixture missing pane")?
            .clone();
        pane.pane_id = id.into();
        pane.tab_id = tab_id.into();
        pane.focused = false;
        snapshot.panes.push(pane);
    }
    Ok(snapshot)
}

fn surface(snapshot: &ClientShellSnapshot, width: u16, height: u16) -> PaneSurfaceFrame {
    let blank = CellData {
        symbol: " ".into(),
        fg: 7,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    };
    let mut cells = vec![blank; usize::from(width) * usize::from(height)];
    let lines = [
        "Agent transcript (fixture, no daemon)",
        "You: review the native composer",
        "Agent: checking local draft isolation",
        "Unicode: caf\u{e9}, na\u{ef}ve, \u{3bb}, \u{3a9}",
        "  [ok] terminal remains visible",
        "  [ok] no prompt has been submitted",
        "Enter adds a line; Cmd-Enter sends",
        "Canceled transport retains your draft",
        "----------------------------------------",
        "Second split pane: independent draft",
        "You: inspect focus and mode switching",
        "Agent: waiting for a local prompt",
        "  01 parse the selected recipient",
        "  02 preserve Unicode and selection",
        "  03 confirm the current surface",
        "  04 keep failed sends editable",
        "This output is a protocol cell fixture",
        "No shell, agent, or daemon was started",
    ];
    for (row, line) in cells.chunks_mut(usize::from(width)).zip(lines) {
        for (cell, ch) in row.iter_mut().zip(line.chars()) {
            cell.symbol = ch.to_string();
        }
    }
    let split = snapshot.focused_tab_id.as_deref() == Some("w1:t1");
    let area = SurfaceRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let panes = snapshot
        .panes
        .iter()
        .filter(|pane| Some(&pane.tab_id) == snapshot.focused_tab_id.as_ref())
        .enumerate()
        .map(|(index, pane)| {
            let rect = if split {
                SurfaceRect {
                    y: if index == 0 { 0 } else { height / 2 + 1 },
                    height: if index == 0 {
                        height / 2
                    } else {
                        height.saturating_sub(height / 2 + 1)
                    },
                    ..area
                }
            } else {
                area
            };
            PaneSurfacePane {
                pane_id: pane.pane_id.clone(),
                content_revision: snapshot.revision,
                rect,
                inner_rect: rect,
                scrollbar_rect: None,
                scroll: None,
                focused: pane.focused,
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                alternate_screen_active: false,
                pixel_width: 0,
                pixel_height: 0,
            }
        })
        .collect();
    PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: snapshot.revision,
        frame: FrameData {
            width,
            height,
            cells,
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes,
        splits: if split {
            vec![PaneSurfaceSplit {
                direction: PaneSurfaceSplitDirection::Horizontal,
                pos: height / 2,
                area,
                hit_rect: SurfaceRect {
                    y: height / 2,
                    height: 1,
                    ..area
                },
                path: vec![],
            }]
        } else {
            vec![]
        },
        popup: None,
        graphics: Default::default(),
    }
}

fn install(
    view: &mut HerdrWindow,
    snapshot: ClientShellSnapshot,
    cx: &mut Context<HerdrWindow>,
) -> Result<(), String> {
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
    *endpoint
        .connection
        .inbox
        .try_lock()
        .map_err(|_| "fixture inbox locked")? = view.live.clone();
    view.sync_composer(cx);
    cx.notify();
    Ok(())
}

fn focus_pane(
    view: &mut HerdrWindow,
    id: &str,
    cx: &mut Context<HerdrWindow>,
) -> Result<(), String> {
    let mut snapshot = view
        .live
        .snapshot
        .as_deref()
        .ok_or("missing snapshot")?
        .clone();
    let tab_id = snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == id)
        .ok_or("missing pane")?
        .tab_id
        .clone();
    snapshot.focused_pane_id = Some(id.into());
    snapshot.focused_tab_id = Some(tab_id.clone());
    snapshot
        .workspaces
        .first_mut()
        .ok_or("missing workspace")?
        .active_tab_id = tab_id.clone();
    for pane in &mut snapshot.panes {
        pane.focused = pane.pane_id == id;
    }
    for tab in &mut snapshot.tabs {
        tab.focused = tab.tab_id == tab_id;
    }
    snapshot.revision += 1;
    install(view, snapshot, cx)
}

fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn keys(names: &str, window: &mut Window, cx: &mut App) -> Result<(), String> {
    for name in names.split_whitespace() {
        let key = Keystroke::parse(name).map_err(|e| e.to_string())?;
        check(
            window.dispatch_keystroke(key, cx),
            &format!("unhandled keystroke: {name}"),
        )?;
    }
    Ok(())
}

fn text(value: &str, window: &mut Window, cx: &mut App) -> Result<(), String> {
    for ch in value.chars() {
        check(
            window.dispatch_keystroke(
                Keystroke {
                    modifiers: Modifiers::default(),
                    key: ch.to_string(),
                    key_char: Some(ch.to_string()),
                },
                cx,
            ),
            "native text dispatch missed input handler",
        )?;
    }
    Ok(())
}

pub(super) fn start(handle: WindowHandle<HerdrWindow>, cx: &mut App) {
    smoke::EXIT_CODE.store(1, Ordering::SeqCst);
    let background = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let prepared = background.spawn(async { prepare() }).await;
        let (client, directory) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("AGENT native FAIL: setup: {error}");
                let _ = cx.update(|cx| cx.quit());
                return;
            }
        };
        let clipboard = if std::env::var_os("HERDR_AGENT_TEST_CLIPBOARD").is_some() {
            cx.update(|cx| cx.read_from_clipboard()).ok().flatten()
        } else {
            None
        };
        let result = run(
            handle,
            client,
            directory.join("client.sock"),
            clipboard.is_some(),
            cx,
        )
        .await;
        if let Some(clipboard) = clipboard {
            let _ = cx.update(|cx| cx.write_to_clipboard(clipboard));
        }
        if let Err(error) = &result {
            eprintln!("AGENT native FAIL: {error}");
        }
        if result.is_ok() && std::env::var_os("HERDR_AGENT_PREVIEW").is_some() {
            eprintln!("AGENT preview: holding final local fixture for 60 seconds");
            background.timer(Duration::from_secs(60)).await;
        }
        let cleanup = background
            .spawn(async move { std::fs::remove_dir(directory) })
            .await;
        if result.is_ok() && cleanup.is_ok() {
            eprintln!("AGENT native PASS");
            smoke::EXIT_CODE.store(0, Ordering::SeqCst);
        } else if let Err(error) = cleanup {
            eprintln!("AGENT native FAIL: cleanup: {error}");
        }
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

async fn run(
    handle: WindowHandle<HerdrWindow>,
    client: ClientHandle,
    socket: PathBuf,
    test_clipboard: bool,
    cx: &mut AsyncApp,
) -> Result<(), String> {
    let timer = cx.background_executor().clone();
    AnyWindowHandle::from(handle)
        .update(cx, |root, _, cx| {
            root.downcast::<HerdrWindow>()
                .map_err(|_| "unexpected root")?
                .update(cx, |view, cx| {
                    check(
                        view.endpoints[view.selected_endpoint]
                            .connection
                            .handle
                            .is_none(),
                        "main must construct agent window with fixture=true",
                    )?;
                    view.endpoints[view.selected_endpoint].connection.target =
                        ConnectTarget::Socket(socket);
                    view.endpoints[view.selected_endpoint].connection.handle = Some(client);
                    install(view, snapshot()?, cx)
                })
        })
        .map_err(|e| e.to_string())??;
    let mut baseline = Bounds::default();
    let mut saved = composer::Draft::default();
    let mut menu_snapshot = None;
    for viewport in [size(px(800.), px(600.)), size(px(640.), px(400.))] {
        for step in 0..11 {
            // Yield to AppKit/layout and subscription effects; never sleep the UI thread.
            timer.timer(Duration::from_millis(100)).await;
            let mut click = None;
            #[cfg(target_os = "macos")]
            let mut click_target = None;
            AnyWindowHandle::from(handle).update(cx, |root, window, cx| -> Result<(), String> {
                let view = root.downcast::<HerdrWindow>().map_err(|_| "unexpected root")?;
                window.refresh();
                window.draw(cx).clear();
                match step {
                    0 => {
                        window.resize(viewport);
                        view.update(cx, |view, cx| {
                            view.set_tab_mode(&tab("w1:t1"), ViewMode::Terminal, window, cx);
                            view.set_tab_mode(&tab("w1:t2"), ViewMode::Terminal, window, cx);
                        });
                    }
                    1 => {
                        check(window.viewport_size() == viewport, "native resize did not settle")?;
                        baseline = view.read(cx).bounds;
                        menu_snapshot = view.read(cx).live.snapshot.clone();
                        check(baseline.size.height > px(0.), "terminal has no painted bounds")?;
                        click = Some((baseline.origin.x.to_f64() + 40., 20.));
                        #[cfg(target_os = "macos")]
                        {
                            click_target = Some(sidebar::native_tests::Target::acquire(window)?);
                        }
                    }
                    2 => {
                        check(matches!(&view.read(cx).menu.page, Some(menu::Page::TabMode(target)) if *target == tab("w1:t1")), "native right click missed first tab menu")?;
                        keys("down enter", window, cx)?;
                    }
                    3 => {
                        let state = view.read(cx);
                        check(state.agent_tab_active() && state.menu.page.is_none(), "menu did not choose Agent mode")?;
                        check(state.live.snapshot == menu_snapshot, "native tab menu changed daemon selection")?;
                        check(state.composer.focus_handle(cx).is_focused(window), "composer did not receive focus")?;
                        check(state.bounds.size.height > px(0.) && state.bounds.size.height < baseline.size.height && state.bounds.size.width == baseline.size.width, "composer did not reduce actual terminal bounds")?;
                        keys("cmd-a backspace", window, cx)?;
                        text("Review caf\u{e9}", window, cx)?;
                        keys("enter shift-enter", window, cx)?;
                        text("Keep the draft local", window, cx)?;
                        if test_clipboard {
                            keys("cmd-a cmd-c cmd-x cmd-v cmd-t", window, cx)?;
                        } else {
                            eprintln!("AGENT clipboard: opt-in only (HERDR_AGENT_TEST_CLIPBOARD); shortcuts also have headless coverage");
                            keys("end cmd-t", window, cx)?;
                        }
                    }
                    4 => {
                        check(view.read(cx).composer.read(cx).text() == PROMPT, "multiline native keys/clipboard changed the draft")?;
                        // GPUI 0.2.2 has no public getter for Window's registered input
                        // handler. This is EntityInputHandler IME coverage, not OS IME delivery.
                        eprintln!("AGENT IME: explicit EntityInputHandler fallback");
                        view.read(cx).composer.clone().update(cx, |editor, cx| {
                            editor.replace_and_mark_text_in_range(None, "\u{3bb}", None, window, cx);
                        });
                        keys("cmd-enter", window, cx)?;
                    }
                    5 => {
                        for _ in 0..4 {
                            view.update(cx, |view, cx| {
                                let snapshot = view.live.snapshot.as_deref().ok_or("missing fixture snapshot")?.clone();
                                install(view, snapshot, cx)
                            })?;
                            window.refresh();
                            window.draw(cx).clear();
                            if view.read(cx).input_ready() {
                                break;
                            }
                        }
                        check(view.read(cx).input_ready(), "fixture surface did not match the settled terminal viewport")?;
                        let state = view.read(cx);
                        check(state.composer.read(cx).is_composing() && state.composer_notice.is_none(), "preedit triggered submission")?;
                        check(state.marked.is_empty() && state.composer.read(cx).draft().text() == PROMPT, "IME touched root or committed draft")?;
                        let terminal_bottom = state.bounds.bottom();
                        state.composer.clone().update(cx, |editor, cx| -> Result<(), String> {
                            let start = PROMPT.encode_utf16().count();
                            let bounds = editor.bounds_for_range(start..start + 1, Bounds::new(point(px(0.), px(0.)), viewport), window, cx)
                                .ok_or("IME has no painted composer bounds")?;
                            check(bounds.top() >= terminal_bottom && bounds.bottom() <= viewport.height && bounds.left() >= px(0.) && bounds.right() <= viewport.width,
                                "IME bounds escaped the visible composer")?;
                            editor.replace_text_in_range(None, "\u{3bb}", window, cx);
                            Ok(())
                        })?;
                        keys("cmd-enter", window, cx)?;
                    }
                    6 => {
                        let state = view.read(cx);
                        check(state.composer_notice.as_deref().is_some_and(|notice| notice.starts_with("Input not queued:")), "Send did not fail against the canceled transport")?;
                        check(state.composer.read(cx).text() == format!("{PROMPT}\u{3bb}"), "failed Send lost the draft")?;
                        keys("shift-left", window, cx)?;
                    }
                    7 => {
                        saved = view.read(cx).composer.read(cx).draft();
                        view.update(cx, |view, cx| view.open_tab_menu(tab("w1:t2"), point(px(340.), px(40.)), window, cx));
                        window.refresh();
                        window.draw(cx).clear();
                        keys("down enter", window, cx)?;
                    }
                    8 => {
                        view.update(cx, |view, cx| -> Result<(), String> {
                            check(view.agent_modes.mode(endpoint::LOCAL, "boot-v1", "w1:t2") == ViewMode::Agent && view.live.snapshot.as_ref().and_then(|s| s.focused_tab_id.as_deref()) == Some("w1:t1"), "inactive menu navigated or missed mode")?;
                            check(view.navigation_fence.is_none(), "mode change queued navigation")?;
                            check(view.live.snapshot == menu_snapshot, "inactive tab menu changed daemon snapshot")?;
                            focus_pane(view, "w1:p2", cx)?;
                            check(view.composer.read(cx).text().is_empty(), "draft leaked into second pane")?;
                            view.composer.update(cx, |editor, cx| editor.replace_text_in_range(None, "second pane", window, cx));
                            let second = view.composer.read(cx).draft();
                            focus_pane(view, "w1:p3", cx)?;
                            check(view.composer.read(cx).text().is_empty(), "draft leaked into second tab")?;
                            view.composer.update(cx, |editor, cx| editor.replace_text_in_range(None, "other tab", window, cx));
                            let third = view.composer.read(cx).draft();
                            view.set_tab_mode(&tab("w1:t2"), ViewMode::Terminal, window, cx);
                            view.set_tab_mode(&tab("w1:t2"), ViewMode::Agent, window, cx);
                            check(view.composer.read(cx).draft() == third, "mode switch lost second tab draft")?;
                            focus_pane(view, "w1:p2", cx)?;
                            check(view.composer.read(cx).draft() == second, "pane switch lost draft")?;
                            focus_pane(view, "w1:p1", cx)?;
                            check(view.composer.read(cx).draft() == saved, "return lost first draft or selection")?;
                            view.set_tab_mode(&tab("w1:t1"), ViewMode::Terminal, window, cx);
                            Ok(())
                        })?;
                    }
                    9 => {
                        check(view.read(cx).bounds == baseline, "Terminal mode did not restore painted bounds")?;
                        view.update(cx, |view, cx| view.set_tab_mode(&tab("w1:t1"), ViewMode::Agent, window, cx));
                    }
                    _ => {
                        let state = view.read(cx);
                        check(state.composer.read(cx).draft() == saved, "mode round trip lost draft/selection")?;
                        check(state.bounds.size.height > px(0.) && state.bounds.size.height < baseline.size.height, "Agent mode layout was not restored")?;
                        check(state.input_probe.keys == 0 && state.input_probe.text == 0 && state.input_probe.actions == 0 && state.marked.is_empty(), &format!("composer input escaped to the root: {:?}, marked_bytes={}", state.input_probe, state.marked.len()))?;
                        check(state.local_error.as_deref().is_none_or(|error| error.starts_with("Resize:")), "prototype dispatched a non-resize daemon operation")?;
                        eprintln!("AGENT verified viewport={viewport:?}: native menu, keys, IME, retained Send, pane/tab drafts, bounds; clipboard={test_clipboard}");
                        // Repeat from clean secondary drafts while preserving the final
                        // first-pane draft for the optional screenshot preview.
                        view.update(cx, |view, _| view.drafts.clear());
                    }
                }
                Ok(())
            }).map_err(|e| e.to_string())??;
            if let Some((x, y)) = click {
                #[cfg(target_os = "macos")]
                click_target
                    .ok_or("missing native fixture target")?
                    .right_click(x, y)?;
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = (x, y);
                    return Err("native agent fixture requires macOS".into());
                }
            }
        }
    }
    Ok(())
}
