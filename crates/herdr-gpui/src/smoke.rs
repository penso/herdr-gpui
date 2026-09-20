//! Native opt-in smoke driver. No test platform or blocking waits on the UI thread.
use super::*;
use std::{
    sync::atomic::{AtomicU8, Ordering},
    time::Instant,
};

pub static EXIT_CODE: AtomicU8 = AtomicU8::new(0);

pub fn start_sidebar(handle: WindowHandle<HerdrWindow>, cx: &mut App) {
    EXIT_CODE.store(1, Ordering::SeqCst);
    #[cfg(target_os = "macos")]
    if let Err(error) = app_icon::verify_native() {
        eprintln!("ICON native FAIL: {error}");
        cx.quit();
        return;
    }
    cx.set_global(sidebar::layout_tests::PaintedProbes::default());
    let timer = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        for frame in 0..12 {
            timer.timer(Duration::from_millis(100)).await;
            let result = AnyWindowHandle::from(handle).update(
                cx,
                |root, window, cx| -> Result<(), String> {
                    use crate::sidebar::layout_tests::PaintedProbes;
                    let (w, h) =
                        [(1200., 780.), (640., 400.), (1000., 650.), (800., 600.)][frame / 3];
                    if frame % 3 == 0 {
                        window.resize(size(px(w), px(h)));
                    } else if window.viewport_size() != size(px(w), px(h)) {
                        return Err(format!(
                            "native resize did not settle: {:?}",
                            window.viewport_size()
                        ));
                    }
                    cx.default_global::<PaintedProbes>().0.clear();
                    root.downcast::<HerdrWindow>()
                        .map_err(|_| "unexpected root")?
                        .update(cx, |_, cx| cx.notify());
                    window.refresh();
                    window.draw(cx).clear();
                    if frame > 0 && sidebar::GITHUB_ICON.clone().use_render_image(window, cx).is_none() {
                        return Err("embedded GitHub SVG did not render".into());
                    }
                    let probes = &cx.global::<PaintedProbes>().0;
                    let mut failed = false;
                    for input in [
                        "herdr",
                        "main",
                        "review",
                        "Claude Code",
                        "agent",
                        "1256789",
                        "herdr-gpui-sidebar-rendering-regression-investigation",
                        "fix/sidebar-label-width-and-overflow-regression",
                        "Investigate sidebar rendering and verify long agent labels",
                    ] {
                        let p = probes
                            .get(input)
                            .ok_or_else(|| format!("missing paint: {input}"))?;
                        if frame == 0 {
                            eprintln!("SIDEBAR frame={frame} input={input:?} {p:?}");
                        }
                        let expected_short = input.len() < 20;
                        let title_icon = matches!(input, "herdr" | "herdr-gpui-sidebar-rendering-regression-investigation");
                        let expected_width = px(sidebar::LABEL_WIDTH - if title_icon { sidebar::ICON_RESERVE } else { 0. });
                        if p.glyph_text != p.cached
                            || (expected_short && p.glyph_text != input)
                            || (!expected_short
                                && (p.width < px(150.) || !p.glyph_text.ends_with('\u{2026}')))
                            || p.clipped
                            || p.bounds.size.width != expected_width
                            || p.mask.size.width != expected_width
                            || p.width > p.bounds.size.width
                            || p.bounds.size.height != px(16.)
                        {
                            eprintln!("SIDEBAR bad paint frame={frame} input={input:?} {p:?}");
                            failed = true;
                        }
                    }
                    if failed {
                        return Err("incomplete/cropped native glyph output".into());
                    }
                    eprintln!(
                        "SIDEBAR verified frame={frame} viewport={:?} labels=9 clipped=0",
                        window.viewport_size()
                    );
                    Ok(())
                },
            );
            if !matches!(result, Ok(Ok(()))) {
                eprintln!("SIDEBAR native FAIL: {result:?}");
                let _ = cx.update(|cx| cx.quit());
                return;
            }
        }
        #[cfg(target_os = "macos")]
        for step in 0..4 {
            let point = AnyWindowHandle::from(handle).update(cx, |root, window, cx| {
                let view = root
                    .downcast::<HerdrWindow>()
                    .map_err(|_| "unexpected root")?;
                view.update(cx, |view, _| {
                    if step == 0 {
                        if let Some(snapshot) = view.live.snapshot.as_mut() {
                            let snapshot = Arc::make_mut(snapshot);
                            snapshot.focused_workspace_id = Some("w4".into());
                            for workspace in &mut snapshot.workspaces {
                                workspace.focused = workspace.workspace_id == "w4";
                            }
                        }
                        view.marked = "preserve active child".into();
                    }
                });
                window.refresh();
                cx.default_global::<sidebar::layout_tests::PaintedProbes>()
                    .0
                    .clear();
                window.draw(cx).clear();
                let label = match step {
                    0 => "\u{25be}",
                    1 => "\u{25b8}",
                    _ => "menu",
                };
                cx.global::<sidebar::layout_tests::PaintedProbes>()
                    .0
                    .get(label)
                    .map(|probe| probe.bounds.center())
                    .ok_or_else(|| "missing native click target".to_owned())
                    .and_then(|point| Ok((sidebar::native_tests::Target::acquire(window)?, point)))
            });
            let result = match point {
                Ok(Ok((target, point))) => target.click(point.x.to_f64(), point.y.to_f64()),
                error => Err(format!("native target: {error:?}")),
            };
            timer.timer(Duration::from_millis(50)).await;
            let result = if step == 3 {
                result.and_then(|()| {
                    let target = AnyWindowHandle::from(handle)
                        .update(cx, |_, window, _| sidebar::native_tests::Target::acquire(window))
                        .map_err(|e| e.to_string())??;
                    target.click(700., 500.)
                })
            } else {
                result
            };
            let verified = AnyWindowHandle::from(handle).update(
                cx,
                |root, window, cx| -> Result<(), String> {
                    result?;
                    let view = root
                        .downcast::<HerdrWindow>()
                        .map_err(|_| "unexpected root")?;
                    let state = view.read(cx);
                    if step < 2 {
                        if state
                            .collapsed_repos
                            .contains("/fixture/agent-launcher/.git")
                            != (step == 0)
                            || state.marked != "preserve active child"
                            || state.live.snapshot.as_ref().is_none_or(|s| {
                                s.workspaces.len() != 40
                                    || s.focused_workspace_id.as_deref() != Some("w4")
                            })
                        {
                            return Err(
                                "collapse navigated, removed rows, or lost selection".into()
                            );
                        }
                    } else if step == 2 {
                        if state.menu.page != Some(menu::Page::Menu) {
                            return Err("native footer click did not open menu".into());
                        }
                        let before = state.input_probe;
                        window.dispatch_action(Box::new(RunCommand { command: Command::Tab }), cx);
                        for key in ["down", "enter", "x", "escape"] {
                            window.dispatch_keystroke(
                                Keystroke {
                                    key: key.into(),
                                    ..Default::default()
                                },
                                cx,
                            );
                        }
                        let state = view.read(cx);
                        if state.menu.page.is_some()
                            || state.input_probe.text != before.text
                            || state.input_probe.keys != before.keys
                            || state.input_probe.actions != before.actions
                        {
                            return Err("menu keyboard handling leaked terminal input".into());
                        }
                    } else if state.menu.page.is_some() {
                        return Err("outside click did not dismiss native menu".into());
                    }
                    Ok(())
                },
            );
            if !matches!(verified, Ok(Ok(()))) {
                eprintln!("SIDEBAR native interaction FAIL: {verified:?}");
                let _ = cx.update(|cx| cx.quit());
                return;
            }
        }
        #[cfg(target_os = "macos")]
        if let Err(error) = sidebar_hosts(handle, cx).await {
            eprintln!("SIDEBAR native hosts FAIL: {error}");
            let _ = cx.update(|cx| cx.quit());
            return;
        }
        eprintln!("SIDEBAR native PASS: 12 Menlo draws, 4 sizes, collapse/expand, menu keyboard isolation and outside dismissal; host routing, disabled selection, scoped repositories, resized host/agent glyphs, independent scroll and decoy key window");
        EXIT_CODE.store(0, Ordering::SeqCst);
        let _ = cx.update(|cx| cx.quit());
    })
    .detach();
}

#[cfg(target_os = "macos")]
async fn sidebar_hosts(handle: WindowHandle<HerdrWindow>, cx: &mut AsyncApp) -> Result<(), String> {
    use sidebar::{layout_tests::PaintedProbes, native_tests::Target};
    const REMOTE: &str = "Synthetic host with a deliberately long label";
    let decoy = cx
        .update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                cx.new(|cx| {
                    HerdrWindow::new(
                        ConnectTarget::Socket("/unused-decoy.sock".into()),
                        window,
                        cx,
                        true,
                    )
                })
            })
        })
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    handle
        .update(cx, |view, window, cx| {
            view.endpoints.clear();
            for (index, label) in ["Local", REMOTE, "Disabled"].into_iter().enumerate() {
                let mut endpoint = endpoint::Endpoint::new(
                    if index == 0 {
                        endpoint::LOCAL.into()
                    } else {
                        format!("fixture-{index}")
                    },
                    label.into(),
                    ConnectTarget::Socket(format!("/unused-sidebar-{index}.sock").into()),
                    index != 2,
                );
                let mut snapshot = sidebar::layout_tests::snapshot(6);
                snapshot.workspaces.drain(0..3);
                snapshot.workspaces[0].label = format!("repository-{index}");
                snapshot.workspaces[1].custom_label = true;
                snapshot.workspaces[1].label = format!("child-{index}");
                snapshot.workspaces.truncate(2);
                snapshot.agents.truncate(1);
                snapshot.agents[0].name =
                    Some(format!("agent-{index}-with-a-deliberately-long-label"));
                endpoint.live.snapshot = Some(Arc::new(snapshot));
                if let Ok(mut inbox) = endpoint.connection.inbox.lock() {
                    *inbox = endpoint.live.clone();
                }
                view.endpoints.push(endpoint);
            }
            view.selected_endpoint = 0;
            view.live = view.endpoints[0].live.clone();
            view.collapsed_repos.clear();
            view.marked = "preserve collapse composition".into();
            window.resize(size(px(480.), px(780.)));
            cx.notify();
        })
        .map_err(|e| e.to_string())?;
    cx.update(|cx| cx.activate(true))
        .map_err(|e| e.to_string())?;
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
    // The decoy is deliberately key. Neither acquisition nor delivery may use it.
    let target = decoy
        .update(cx, |_, window, _| Target::acquire(window))
        .map_err(|e| e.to_string())??;
    target.make_key();
    if !target.is_key() {
        return Err("decoy did not become key".into());
    }
    drop(target);
    let viewport = handle
        .update(cx, |_, window, _| window.viewport_size())
        .map_err(|e| e.to_string())?;
    if viewport != size(px(480.), px(780.)) {
        return Err(format!("narrow resize not settled: {viewport:?}"));
    }
    for (step, label, expected) in [
        (0, REMOTE, 0), // collapse unselected host
        (1, REMOTE, 0), // expand
        (2, REMOTE, 1),
        (3, "Local", 0),
        (4, "Disabled", 0),
        (5, "child-1", 1),
        (6, "child-0", 0),
        (7, "agent-1-with-a-deliberately-long-label", 1),
        (8, "agent-0-with-a-deliberately-long-label", 0),
        (9, "repository-1", 0),
    ] {
        let decoy_target = decoy
            .update(cx, |_, window, _| Target::acquire(window))
            .map_err(|e| e.to_string())??;
        if !decoy_target.is_key() {
            return Err(format!("decoy lost key status at step {step}"));
        }
        drop(decoy_target);
        let (target, point) = AnyWindowHandle::from(handle)
            .update(cx, |_, window, cx| -> Result<_, String> {
                cx.default_global::<PaintedProbes>().0.clear();
                window.refresh();
                window.draw(cx).clear();
                let probes = &cx.global::<PaintedProbes>().0;
                for (name, prefix) in [
                    (REMOTE, "Synthetic host"),
                    ("agent-1-with-a-deliberately-long-label", "agent-1-with-a"),
                ] {
                    let p = probes.get(name).ok_or_else(|| format!("missing {name}"))?;
                    if p.clipped
                        || p.glyph_text != p.cached
                        || !p.glyph_text.ends_with('\u{2026}')
                        || !p.glyph_text.starts_with(prefix)
                        || p.bounds.size.width < px(110.)
                        || p.width > p.bounds.size.width
                    {
                        return Err(format!("narrow native label: {name}: {p:?}"));
                    }
                }
                let p = probes
                    .get(label)
                    .ok_or_else(|| format!("missing target {label}"))?;
                let mut point = p.bounds.center();
                if step < 2 {
                    point.x =
                        p.bounds.left() - px(sidebar::HOST_GAP + sidebar::HOST_ARROW_WIDTH / 2.);
                }
                if step == 9 {
                    // The title ends at the label column's edge, even with an avatar.
                    point.x =
                        p.bounds.right() + px((sidebar::LABEL_GAP + sidebar::ARROW_RESERVE) / 2.);
                }
                Ok((Target::acquire(window)?, point))
            })
            .map_err(|e| e.to_string())??;
        target.click(point.x.to_f64(), point.y.to_f64())?;
        drop(target);
        AnyWindowHandle::from(handle)
            .update(cx, |root, window, cx| -> Result<(), String> {
                cx.default_global::<PaintedProbes>().0.clear();
                window.refresh();
                window.draw(cx).clear();
                let entity = root
                    .downcast::<HerdrWindow>()
                    .map_err(|_| "unexpected root")?;
                let view = entity.read(cx);
                if view.selected_endpoint != expected {
                    return Err(format!(
                        "step {step}: selected {} expected {expected}",
                        view.selected_endpoint
                    ));
                }
                if step < 2
                    && (view.endpoints[1].collapsed != (step == 0)
                        || view.marked != "preserve collapse composition")
                {
                    return Err("host collapse changed selection/composition".into());
                }
                if (5..=8).contains(&step) {
                    let expected = if step < 7 {
                        NavigationTarget::Workspace("w4".to_owned())
                    } else {
                        NavigationTarget::Pane("p0".to_owned())
                    };
                    if view.pending_navigation.as_ref() != Some(&expected) {
                        return Err(format!("wrong duplicate-ID route at step {step}"));
                    }
                }
                if step == 9
                    && (!view.endpoints[1]
                        .collapsed_repos
                        .contains("/fixture/agent-launcher/.git")
                        || !view.collapsed_repos.is_empty())
                {
                    return Err("repository collapse escaped endpoint scope".into());
                }
                let probes = &cx.global::<PaintedProbes>().0;
                if step == 0
                    && (probes.contains_key("child-1")
                        || !probes.contains_key("agent-1-with-a-deliberately-long-label"))
                {
                    return Err("host collapse hid agents or left workspace visible".into());
                }
                if step == 9 && (probes.contains_key("child-1") || !probes.contains_key("child-0"))
                {
                    return Err("repository visibility not scoped".into());
                }
                Ok(())
            })
            .map_err(|e| e.to_string())??;
        eprintln!("SIDEBAR native host step={step} verified");
    }
    // Exercise wider, narrower, then restored native allocations. This catches
    // stale truncated font runs as well as host labels left at the old 116px.
    for (window_width, preferred, host_width, agent_width, host_prefix, agent_prefix) in [
        (
            800.,
            Some(400.),
            284.,
            362.,
            "Synthetic host",
            "agent-1-with-a",
        ),
        (360., None, 77., 82., "Synthetic", "agent-1"),
        (
            800.,
            Some(160.),
            117.,
            122.,
            "Synthetic host",
            "agent-1-with-a",
        ),
        (
            800.,
            Some(480.),
            364.,
            442.,
            REMOTE,
            "agent-1-with-a-deliberately-long-label",
        ),
        (480., None, 116., 194., "Synthetic host", "agent-1-with-a"),
    ] {
        handle
            .update(cx, |view, window, cx| {
                view.sidebar_width = preferred;
                window.resize(size(px(window_width), px(780.)));
                cx.notify();
            })
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let settled = AnyWindowHandle::from(handle)
            .update(cx, |_, window, cx| -> Result<bool, String> {
                if window.viewport_size() != size(px(window_width), px(780.)) {
                    return Ok(false);
                }
                cx.default_global::<PaintedProbes>().0.clear();
                window.refresh();
                window.draw(cx).clear();
                for (name, prefix, expected_width) in [
                    (REMOTE, host_prefix, host_width),
                    ("agent-1-with-a-deliberately-long-label", agent_prefix, agent_width),
                ] {
                    let p = cx
                        .global::<PaintedProbes>()
                        .0
                        .get(name)
                        .ok_or("missing narrow label")?;
                    if p.clipped
                        || p.glyph_text != p.cached
                        || (p.glyph_text != name && !p.glyph_text.ends_with('\u{2026}'))
                        || !p.glyph_text.starts_with(prefix)
                        || p.bounds.size.width != px(expected_width)
                        || p.mask.size.width != px(expected_width)
                        || p.width > p.bounds.size.width
                    {
                        return Err(format!("resized native label window={window_width} preferred={preferred:?}: {p:?}"));
                    }
                }
                Ok(true)
            })
            .map_err(|e| e.to_string())??;
            if settled {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!("{window_width}px resize timed out"));
            }
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
        }
        eprintln!(
            "SIDEBAR native resized labels verified: window={window_width} preferred={preferred:?} host={host_width} agent={agent_width}"
        );
    }
    for font_size in [16., 20., 12.] {
        AnyWindowHandle::from(handle)
            .update(cx, |root, window, cx| -> Result<(), String> {
                let entity = root
                    .downcast::<HerdrWindow>()
                    .map_err(|_| "unexpected root")?;
                entity.update(cx, |view, _| -> Result<(), String> {
                    view.config.sidebar.size = font_size;
                    view.config.theme = if font_size == 12. { "Default" } else { "Nord" }.into();
                    view.theme = view.config.theme()?;
                    Ok(())
                })?;
                cx.default_global::<PaintedProbes>().0.clear();
                window.refresh();
                window.draw(cx).clear();
                for name in [REMOTE, "agent-1-with-a-deliberately-long-label"] {
                    let probe = cx
                        .global::<PaintedProbes>()
                        .0
                        .get(name)
                        .ok_or("missing scaled label")?;
                    if probe.clipped
                        || probe.glyph_text != probe.cached
                        || !probe.glyph_text.ends_with('\u{2026}')
                        || probe.width > probe.bounds.size.width
                        // Native layout snaps fractional line heights to device pixels.
                        || (probe.bounds.size.height - px(font_size * 4. / 3.)).abs()
                            > px(1. / window.scale_factor())
                    {
                        return Err(format!("scaled native label size={font_size}: {probe:?}"));
                    }
                }
                eprintln!("SIDEBAR native themed font labels verified: size={font_size}");
                Ok(())
            })
            .map_err(|e| e.to_string())??;
    }
    handle
        .update(cx, |view, _, cx| -> Result<(), String> {
            let snapshot = Arc::make_mut(
                view.live
                    .snapshot
                    .as_mut()
                    .ok_or("missing scroll snapshot")?,
            );
            let agent = snapshot.agents[0].clone();
            let workspace = snapshot.workspaces[0].clone();
            for index in 0..40 {
                let mut agent = agent.clone();
                agent.pane_id = format!("scroll-p{index}");
                snapshot.agents.push(agent);
                let mut workspace = workspace.clone();
                workspace.workspace_id = format!("scroll-w{index}");
                workspace.worktree = None;
                snapshot.workspaces.push(workspace);
            }
            cx.notify();
            Ok(())
        })
        .map_err(|e| e.to_string())??;
    for list in 0..2 {
        AnyWindowHandle::from(handle)
            .update(cx, |root, window, cx| -> Result<(), String> {
                window.refresh();
                window.draw(cx).clear();
                let entity = root
                    .downcast::<HerdrWindow>()
                    .map_err(|_| "unexpected root")?;
                let scroll = entity.read(cx).sidebar_scroll.clone();
                let other = scroll[1 - list].offset();
                scroll[list].set_offset(point(px(0.), px(-80.)));
                window.refresh();
                window.draw(cx).clear();
                if scroll[list].offset().y != px(-80.) || scroll[1 - list].offset() != other {
                    return Err(format!("scroll handles are not independent: list={list}"));
                }
                Ok(())
            })
            .map_err(|e| e.to_string())??;
    }
    decoy
        .update(cx, |view, window, _| -> Result<(), String> {
            if view.selected_endpoint != 0
                || !view.collapsed_repos.is_empty()
                || view.menu.page.is_some()
                || view.pending_navigation.is_some()
                || view.selection_epoch != 0
            {
                return Err("fixture click affected decoy".into());
            }
            window.remove_window();
            Ok(())
        })
        .map_err(|e| e.to_string())??;
    Ok(())
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InputProbe {
    pub actions: u64,
    pub keys: u64,
    pub text: u64,
}

const STEPS: &[&str] = &[
    "initial painted surface",
    "Cmd-T new tab",
    "Cmd-D right split",
    "Cmd-Shift-D below split",
    "previous tab",
    "next tab",
    "Cmd-N workspace",
    "workspace navigation",
    "return to full-width tab",
    "text commit + Enter output",
    "native resize",
    "reconnect persisted state",
    "input after reconnect",
    "external workspace pushed to idle GUI",
];

const EXTERNAL_TIMEOUT: Duration = Duration::from_secs(3);

struct ExternalWorkspace {
    // Keep the independent connection alive until the GUI has observed the change.
    _client: herdr_client::Client,
    id: String,
    sent: Instant,
    responded: Instant,
}

fn create_external_workspace(
    target: ConnectTarget,
    options: ConnectOptions,
    boot: String,
) -> Result<ExternalWorkspace, String> {
    use herdr_client::ClientEvent;
    let client = herdr_client::connect(target, options).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut request = None;
    loop {
        let event = client
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|e| format!("external workspace request: {e}"))?;
        match event {
            ClientEvent::Snapshot(snapshot) if request.is_none() => {
                if snapshot.boot_id != boot {
                    return Err("external client connected to a different daemon boot".into());
                }
                let sent = Instant::now();
                let id = client
                    .handle
                    .request(
                        &boot,
                        "workspace.create",
                        serde_json::json!({
                            "focus": false, "label": "external-gui-smoke"
                        }),
                    )
                    .map_err(|e| e.to_string())?;
                request = Some((id, sent));
            }
            ClientEvent::Response {
                request_id,
                response,
            } => {
                let responded = Instant::now();
                let (expected, sent) = request.as_ref().ok_or("unsolicited external response")?;
                if &request_id != expected || response.get("error").is_some() {
                    return Err(format!("external workspace response: {response}"));
                }
                let id = response["result"]["workspace"]["workspace_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or("external response missing workspace ID")?
                    .to_owned();
                return Ok(ExternalWorkspace {
                    id,
                    sent: *sent,
                    responded,
                    _client: client,
                });
            }
            ClientEvent::Disconnected { reason } | ClientEvent::CommandRejected { reason, .. } => {
                return Err(reason);
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            return Err("external workspace request timed out".into());
        }
    }
}

pub fn start(handle: WindowHandle<HerdrWindow>, cx: &mut App) {
    let timer = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let mut step = 0;
        let mut since = Instant::now();
        let mut boot = String::new();
        let mut workspace = String::new();
        let mut first_tab = String::new();
        let mut second_tab = String::new();
        let mut split_pane = String::new();
        let mut old_size = ClientSurfaceSize { cols: 0, rows: 0 };
        let marker = format!("HERDR_GUI_{}_OK", std::process::id());
        let reconnected_marker = format!("{marker}_RECONNECTED");
        let mut frames = 0_u64;
        let mut external_rx = None;
        let mut external = None;
        let mut baseline = None;
        loop {
            timer.timer(Duration::from_millis(100)).await;
            let result = AnyWindowHandle::from(handle).update(cx, |root, window, cx| -> Result<bool, String> {
                let view = root.downcast::<HerdrWindow>().map_err(|_| "unexpected window root")?;
                // Observe only: no focus, draw, refresh, request, or reconnect can help
                // deliver this snapshot. The normal GUI event consumer must do it.
                if step == 13 {
                    let view = view.read(cx);
                    let (before, inbox) = baseline.as_ref().ok_or("missing external baseline")?;
                    if !Arc::ptr_eq(inbox, &view.endpoints[view.selected_endpoint].connection.inbox) || !view.live.status.is_connected()
                        || view.endpoints[view.selected_endpoint].connection.handle.as_ref().is_none_or(|h| h.is_disconnected())
                        || view.local_error.is_some() || view.live.error.is_some() {
                        return Err("GUI connection changed or failed during external creation".into());
                    }
                    if external.is_none() {
                        let rx: &std::sync::mpsc::Receiver<Result<ExternalWorkspace, String>> = external_rx.as_ref().ok_or("missing external receiver")?;
                        match rx.try_recv() {
                            Ok(result) => external = Some(result?),
                            Err(std::sync::mpsc::TryRecvError::Empty) => {},
                            Err(error) => return Err(format!("external worker: {error}")),
                        }
                    }
                    if since.elapsed() > Duration::from_secs(12) {
                        return Err("external workspace worker/GUI timed out".into());
                    }
                    let Some(created) = &external else { return Ok(false) };
                    let elapsed = created.sent.elapsed();
                    if elapsed > EXTERNAL_TIMEOUT {
                        return Err(format!("external workspace not consumed by GUI within {EXTERNAL_TIMEOUT:?}: elapsed={elapsed:?} id={} snapshot={:?}", created.id, view.live.snapshot));
                    }
                    let Some(snapshot) = &view.live.snapshot else { return Ok(false) };
                    let before: &Arc<ClientShellSnapshot> = before;
                    if snapshot.boot_id != before.boot_id || snapshot.focused_workspace_id != before.focused_workspace_id
                        || snapshot.focused_tab_id != before.focused_tab_id || snapshot.focused_pane_id != before.focused_pane_id {
                        return Err("external unfocused creation changed GUI boot/focus".into());
                    }
                    if !snapshot.workspaces.iter().any(|w| w.workspace_id == created.id && w.label == "external-gui-smoke") { return Ok(false); }
                    if snapshot.revision <= before.revision || snapshot.workspaces.len() != before.workspaces.len() + 1
                        || snapshot.tabs.len() != before.tabs.len() + 1 {
                        return Err(format!("incorrect external workspace snapshot: {snapshot:?}"));
                    }
                    eprintln!("GUI external workspace push verified: id={} revision={} -> {} command_to_observed_ms={} response_to_observed_ms={} bound_ms={} observation_poll_ms=100 unchanged_connection=true unchanged_focus=true no_refresh=true",
                        created.id, before.revision, snapshot.revision, elapsed.as_millis(), created.responded.elapsed().as_millis(), EXTERNAL_TIMEOUT.as_millis());
                    eprintln!("GUI integration PASS: same boot={boot}, 3 workspaces / 4 tabs, persisted shell output after reconnect, external workspace pushed to idle GUI");
                    EXIT_CODE.store(0, Ordering::SeqCst);
                    cx.quit();
                    return Ok(true);
                }
                if frames == 0 {
                    // Exercise the regression: no foreground app or pre-existing input focus.
                    cx.hide();
                    window.blur();
                }
                // on_next_frame runs BEFORE draw, and hidden windows may not receive it.
                // Build the real native window's dispatch tree and input handler synchronously,
                // without activating the app or relying on desktop/OS keyboard focus.
                window.focus(&view.read(cx).focus);
                window.refresh();
                window.draw(cx).clear();
                frames += 1;
                let focused = view.read(cx).focus.is_focused(window);
                let active = window.is_window_active();
                let actions_ready = window.is_action_available(&RunCommand { command: Command::Tab }, cx);
                let probe = view.read(cx).input_probe;
                let (live, local_error, options, last_queued_options, bounds) = {
                    let view = view.read(cx);
                    (view.live.clone(), view.local_error.clone(), view.options, view.last_queued_options, view.bounds)
                };
                let diagnostic = || format!(
                    "step={step} ({}) elapsed={:?} frames={frames} focus={focused} actions_ready={actions_ready} active={} probe={probe:?} status={:.160} local_error={:.240} live.error={:.240} connected={} snapshot={:?} surface={:?} size={:?} last_queued_options={last_queued_options:?}",
                    STEPS[step], since.elapsed(), active, live.status,
                    local_error.as_deref().unwrap_or("none"), live.error.as_deref().unwrap_or("none"), live.status.is_connected(),
                    live.snapshot.as_ref().map(|s| (s.revision, s.workspaces.len(), s.tabs.len())),
                    live.surface.as_ref().map(|s| (s.projection_revision, s.panes.len(), s.frame.width, s.frame.height)), options.surface_size
                );
                if local_error.is_some() || live.error.is_some() {
                    return Err(diagnostic());
                }
                if since.elapsed() > Duration::from_secs(20) {
                    return Err(format!("timeout: {}", diagnostic()));
                }
                if !focused || !actions_ready { return Ok(false); }
                let (Some(snapshot), Some(surface)) = (&live.snapshot, &live.surface) else { return Ok(false) };
                if !live.status.is_connected() || snapshot.boot_id != surface.boot_id || snapshot.revision != surface.projection_revision {
                    return Ok(false);
                }
                surface.frame.validate().map_err(|e| format!("invalid frame: {e}; {}", diagnostic()))?;
                if let Some(error) = &snapshot.config_diagnostic {
                    return Err(format!("config diagnostic: {error:?}; {}", diagnostic()));
                }
                let focused_tab = snapshot.focused_tab_id.as_deref().unwrap_or_default();
                let focused_workspace = snapshot.focused_workspace_id.as_deref().unwrap_or_default();
                let key = |name: &str, window: &mut Window, cx: &mut App| -> Result<(), String> {
                    let before = view.read(cx).input_probe;
                    let key = Keystroke::parse(name).map_err(|e| e.to_string())?;
                    if !window.dispatch_keystroke(key, cx) { return Err(format!("unhandled keystroke {name}")); }
                    let after = view.read(cx).input_probe;
                    let delivered = if name.starts_with("cmd-") {
                        after.actions == before.actions + 1
                    } else {
                        after.keys == before.keys + 1
                    };
                    if !delivered { return Err(format!("keystroke {name} missed intended handler: before={before:?} after={after:?}; {}", diagnostic())); }
                    Ok(())
                };
                match step {
                    0 if !focused_tab.is_empty() && surface.panes.len() == 1 && bounds.size.width > px(0.) => {
                        boot = snapshot.boot_id.clone();
                        workspace = focused_workspace.into();
                        first_tab = focused_tab.into();
                        key("cmd-t", window, cx)?;
                    }
                    1 if snapshot.tabs.len() == 2 && focused_tab != first_tab && surface.panes.len() == 1 => {
                        second_tab = focused_tab.into();
                        split_pane = snapshot.focused_pane_id.clone().unwrap_or_default();
                        key("cmd-d", window, cx)?;
                    }
                    2 if surface.panes.len() == 2 => {
                        let old = surface.panes.iter().find(|p| p.pane_id == split_pane).ok_or("original split pane missing")?;
                        let new = surface.panes.iter().find(|p| Some(&p.pane_id) == snapshot.focused_pane_id.as_ref()).ok_or("focused split missing")?;
                        if new.rect.x <= old.rect.x || new.rect.y != old.rect.y { return Err(format!("right split geometry: {}", diagnostic())); }
                        split_pane = new.pane_id.clone();
                        key("cmd-shift-d", window, cx)?;
                    }
                    3 if surface.panes.len() == 3 => {
                        let old = surface.panes.iter().find(|p| p.pane_id == split_pane).ok_or("original split pane missing")?;
                        let new = surface.panes.iter().find(|p| Some(&p.pane_id) == snapshot.focused_pane_id.as_ref()).ok_or("focused split missing")?;
                        if new.rect.y <= old.rect.y || new.rect.x != old.rect.x { return Err(format!("down split geometry: {}", diagnostic())); }
                        window.dispatch_action(Box::new(RunCommand { command: Command::PreviousTab }), cx);
                    }
                    4 if focused_tab == first_tab && surface.panes.len() == 1 => {
                        window.dispatch_action(Box::new(RunCommand { command: Command::NextTab }), cx);
                    }
                    5 if focused_tab == second_tab && surface.panes.len() == 3 => {
                        key("cmd-n", window, cx)?;
                    }
                    6 if snapshot.workspaces.len() == 2 && focused_workspace != workspace && surface.panes.len() == 1 => {
                        view.update(cx, |view, cx| { view.navigate(NavigationTarget::Workspace(&workspace), cx); window.focus(&view.focus); });
                    }
                    7 if focused_workspace == workspace && focused_tab == second_tab && surface.panes.len() == 3 => {
                        // Use the full-width tab so the exact output row cannot wrap in a split.
                        window.dispatch_action(Box::new(RunCommand { command: Command::PreviousTab }), cx);
                    }
                    8 if focused_tab == first_tab && surface.panes.len() == 1 => {
                        let command = format!("echo HERDR_GUI_{}\"_OK\"", std::process::id());
                        type_text(&command, &view, window, cx)?;
                        key("enter", window, cx)?;
                    }
                    9 if has_output(&surface.frame, &marker) => {
                        eprintln!("GUI shell output verified (not command echo): {marker}");
                        old_size = options.surface_size;
                        window.resize(size(px(1000.), px(650.)));
                    }
                    10 if options.surface_size != old_size && last_queued_options == Some(options)
                        && surface.frame.width == options.surface_size.cols && surface.frame.height == options.surface_size.rows => {
                        eprintln!("GUI native resize verified: {:?} -> {:?}", old_size, options.surface_size);
                        view.update(cx, |view, cx| { view.reconnect(cx); window.focus(&view.focus); cx.notify(); });
                    }
                    11 if snapshot.boot_id == boot && snapshot.workspaces.len() == 2 && snapshot.tabs.len() == 3
                        && focused_workspace == workspace && focused_tab == first_tab && has_output(&surface.frame, &marker) => {
                        type_text(&format!("echo HERDR_GUI_{}\"_OK_RECONNECTED\"", std::process::id()), &view, window, cx)?;
                        key("enter", window, cx)?;
                    }
                    12 if has_output(&surface.frame, &reconnected_marker) => {
                        eprintln!("GUI input pipeline verified: frames={frames} focus={focused} active={active} probe={probe:?}");
                        eprintln!("GUI fresh input after reconnect verified: {reconnected_marker}");
                        let target = view.read(cx).endpoints[view.read(cx).selected_endpoint].connection.target.clone();
                        if !matches!(&target, ConnectTarget::Socket(_)) {
                            return Err("external smoke requires an explicit isolated socket".into());
                        }
                        baseline = Some((snapshot.clone(), view.read(cx).endpoints[view.read(cx).selected_endpoint].connection.inbox.clone()));
                        let boot = boot.clone();
                        let (tx, rx) = std::sync::mpsc::channel();
                        std::thread::Builder::new().name("external-workspace-smoke".into()).spawn(move || {
                            let _ = tx.send(create_external_workspace(target, options, boot));
                        }).map_err(|e| e.to_string())?;
                        external_rx = Some(rx);
                    }
                    _ => return Ok(false),
                }
                eprintln!("GUI step {step} ({}) verified; waiting for {}", STEPS[step], STEPS[step + 1]);
                step += 1;
                since = Instant::now();
                Ok(false)
            });
            match result {
                Ok(Ok(true)) => break,
                Ok(Ok(false)) => {},
                error => {
                    EXIT_CODE.store(1, Ordering::SeqCst);
                    eprintln!("GUI integration FAIL: {error:?}");
                    let _ = cx.update(|cx| cx.quit());
                    break;
                }
            }
        }
    }).detach();
}

fn type_text(
    text: &str,
    view: &Entity<HerdrWindow>,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    for ch in text.chars() {
        let before = view.read(cx).input_probe.text;
        if !window.dispatch_keystroke(
            Keystroke {
                modifiers: Modifiers::default(),
                key: ch.to_string(),
                key_char: Some(ch.to_string()),
            },
            cx,
        ) {
            return Err(format!("unhandled text keystroke {ch:?}"));
        }
        if view.read(cx).input_probe.text != before + 1 {
            return Err(format!("text keystroke {ch:?} missed native input handler"));
        }
    }
    Ok(())
}

fn has_output(frame: &FrameData, marker: &str) -> bool {
    frame.width > 0
        && frame.cells.chunks(usize::from(frame.width)).any(|row| {
            row.iter()
                .map(|cell| cell.symbol.as_str())
                .collect::<String>()
                .trim()
                == marker
        })
}
