//! Native event-dispatch + scene-construction benchmark; never connects to a daemon.
use super::*;
use herdr_client::ClientEvent;
use std::time::Instant;
#[cfg(target_os = "macos")]
#[path = "performance_native.rs"]
mod native;

#[derive(Default, Debug, Clone, Copy)]
pub(crate) struct Counts {
    pub shapes: usize,
    pub quads: usize,
    pub glyphs: usize,
    pub decorations: usize,
    pub paint_errors: usize,
    pub metric_shapes: usize,
    pub paints: usize,
    pub run_shapes: usize,
    pub runs: usize,
}
impl Global for Counts {}

fn surface() -> PaneSurfaceFrame {
    let lines = [
        "$ cargo test --workspace  # verify sidebar rendering and terminal grid",
        "Inspecting src/main.rs: cached transcript paint, unchanged cells, exact colors.",
        "  pub fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) {",
        "+ assert_eq!(actual, expected); // regression coverage for agent output",
        "test terminal::tests::wire_colors_are_not_argb ... ok (1256789 tokens)",
        "Review: preserve wide glyphs, combining marks, cursor and popup positioning.",
    ];
    let cells = (0..8000)
        .map(|i| {
            let row = i / 160;
            let col = i % 160;
            let line = lines[row % lines.len()].as_bytes();
            CellData {
                symbol: if col == 156 {
                    "\u{754c}".into()
                } else if col == 158 {
                    "e\u{301}".into()
                } else {
                    (line[col % line.len()] as char).to_string()
                },
                skip: col == 157,
                fg: [0, 0x0298c379, 0x01000067, 0x02e5c07b][row % 4],
                bg: if row % 7 == 0 { 0x02232b36 } else { 0 },
                modifier: [0, 1, 4, 8, 256, 2, 64, 128][row % 8],
                hyperlink: None,
            }
        })
        .collect();
    PaneSurfaceFrame {
        boot_id: "layout-test".into(),
        projection_revision: 1,
        surface_revision: 1,
        frame: FrameData {
            width: 160,
            height: 50,
            cells,
            cursor: Some(CursorState {
                x: 12,
                y: 25,
                visible: true,
                shape: 5,
            }),
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    }
}

fn report(label: &str, samples: &mut [f64]) -> f64 {
    if std::env::var_os("HERDR_PERF_SAMPLES").is_some() {
        println!(
            "{}",
            serde_json::json!({"category": label, "samples_ms": samples})
        );
    }
    samples.sort_by(f64::total_cmp);
    let percentile = |p: usize| samples[(samples.len() * p).div_ceil(100).saturating_sub(1)];
    eprintln!(
        "PERF {label} n={} ms p50={:.3} p95={:.3} max={:.3}",
        samples.len(),
        percentile(50),
        percentile(95),
        samples[samples.len() - 1]
    );
    percentile(95)
}

pub fn start(handle: WindowHandle<HerdrWindow>, cx: &mut App) {
    // AppKit termination can exit(0) inside cx.quit(), before main returns its
    // ExitCode. This daemon-free driver must exit explicitly on pass AND fail.
    smoke::EXIT_CODE.store(1, std::sync::atomic::Ordering::SeqCst);
    let uncached = std::env::var_os("HERDR_PERF_UNCACHED").is_some();
    let batched = !uncached && std::env::var_os("HERDR_PERF_NO_BATCH").is_none();
    let retained = std::env::var_os("HERDR_PERF_RETAINED").is_some();
    let expect_retained = retained && std::env::var_os("HERDR_PERF_NO_RETAIN").is_none();
    let budget = match std::env::var("HERDR_PERF_P95_MS") {
        Ok(value) => value.parse::<f64>().unwrap_or(f64::NAN),
        Err(std::env::VarError::NotPresent) => {
            if cfg!(debug_assertions) {
                1000.
            } else {
                250.
            }
        }
        Err(_) => f64::NAN,
    };
    if budget.is_nan() || budget < 0. {
        eprintln!("HERDR_PERF_P95_MS must be nonnegative milliseconds or inf");
        std::process::exit(2);
    }
    eprintln!(
        "PERF mode={} retained={retained} expect_retained={expect_retained} profile={} p95_budget_ms={budget}",
        if uncached { "reference" } else { "cached" },
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    let timer = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let mut cold = vec![];
        let mut cache_cold = vec![];
        let mut hover = vec![];
        let mut scroll = vec![];
        for frame in 0..141 {
            timer.timer(Duration::from_millis(20)).await;
            let prepared = AnyWindowHandle::from(handle).update(cx, |root, window, cx| {
                if (1..=10).contains(&frame) {
                    root.downcast::<HerdrWindow>()
                        .map_err(|_| "unexpected root")?
                        .update(cx, |view, _| {
                            view.painter.borrow_mut().reset_cache();
                        });
                }
                *cx.default_global::<Counts>() = Counts::default();
                Ok::<_, &str>(
                    if frame > 110 {
                        window.viewport_size().height.to_f64() / 2.
                    } else {
                        0.
                    } + 60.
                        + (frame % 5) as f64 * 40.,
                )
            });
            let Ok(Ok(y)) = prepared else {
                eprintln!("PERF FAIL preparing frame: {prepared:?}");
                std::process::exit(1);
            };
            let start = Instant::now();
            #[cfg(target_os = "macos")]
            if frame > 0
                && let Err(error) = native::dispatch(
                    y,
                    (frame > 80).then_some(if frame % 20 < 10 { -32 } else { 32 }),
                )
            {
                eprintln!("PERF FAIL: {error}");
                std::process::exit(1);
            }
            let result = AnyWindowHandle::from(handle).update(
                cx,
                |root, window, cx| -> Result<(), String> {
                    if frame == 0 {
                        let mut snapshot = sidebar::layout_tests::snapshot(40);
                        let agent = snapshot.agents[0].clone();
                        snapshot.agents = (0..40)
                            .map(|i| {
                                let mut a = agent.clone();
                                a.pane_id = format!("perf-p{i}");
                                a.name = Some(format!("Review transcript {i}"));
                                a
                            })
                            .collect();
                        root.clone()
                            .downcast::<HerdrWindow>()
                            .map_err(|_| "unexpected root")?
                            .update(cx, |view, cx| {
                                view.live.snapshot = Some(Arc::new(snapshot));
                                view.set_surface(Some(Arc::new(surface())), cx);
                                view.painter.borrow_mut().uncached = uncached;
                                cx.notify();
                            });
                        window.resize(size(px(1640.), px(1100.)));
                    }
                    if !retained || frame <= 10 {
                        window.refresh();
                    }
                    window.draw(cx).clear();
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    let counts = *cx.global::<Counts>();
                    if frame > 0
                        && (window.mouse_position().x != px(100.)
                            || (window.mouse_position().y.to_f64() - y).abs() > 1.)
                    {
                        return Err(format!(
                            "native hover missed: {:?}, expected y={y}",
                            window.mouse_position()
                        ));
                    }
                    if frame == 89 || frame == 129 {
                        let view = root
                            .clone()
                            .downcast::<HerdrWindow>()
                            .map_err(|_| "unexpected root")?;
                        let list = usize::from(frame > 110);
                        let offset = view.read(cx).sidebar_scroll[list].offset();
                        eprintln!("PERF native scroll list={list} offset={offset:?}");
                        if offset.y >= px(0.) {
                            return Err("native scroll did not move workspace list".into());
                        }
                    }
                    if expect_retained && frame > 10 {
                        if counts.paints != 0
                            || counts.shapes != 0
                            || counts.run_shapes != 0
                            || counts.runs != 0
                            || counts.metric_shapes != 0
                            || counts.quads != 0
                            || counts.glyphs != 0
                            || counts.decorations != 0
                            || counts.paint_errors != 0
                        {
                            return Err(format!("retained terminal repainted: {counts:?}"));
                        }
                    } else if counts.glyphs < 6000 || counts.paint_errors != 0 {
                        return Err(format!("text was not painted: {counts:?}"));
                    }
                    if !(uncached || expect_retained && frame > 10) {
                        if frame > 20 && (counts.shapes != 0 || counts.metric_shapes != 0 || counts.run_shapes != 0) {
                            return Err(format!("unchanged terminal reshaped: {counts:?}"));
                        }
                        if counts.shapes > 400
                            || counts.paints == 0
                            || counts.quads != 50 * counts.paints
                            || counts.decorations != 1921 * counts.paints
                            || counts.glyphs != 6981 * counts.paints
                            || (batched && counts.runs <= 100 * counts.paints)
                        {
                            return Err(format!("terminal deterministic budget: {counts:?}"));
                        }
                    }
                    if frame == 0 {
                        cold.push(elapsed);
                    } else if frame <= 10 {
                        cache_cold.push(elapsed);
                    } else if frame > 20 && frame <= 80 {
                        hover.push(elapsed);
                    } else if frame > 80 {
                        scroll.push(elapsed);
                    }
                    if frame == 0 || frame == 80 || frame == 140 {
                        eprintln!(
                            "PERF frame={frame} {counts:?} viewport={:?}",
                            window.viewport_size()
                        );
                    }
                    if frame == 140 && !uncached {
                        let view = root
                            .downcast::<HerdrWindow>()
                            .map_err(|_| "unexpected root")?;
                        let verified =
                            view.read(cx).painter.borrow().verify_native_cache(window)?;
                        if verified < 300 {
                            return Err("insufficient native cache coverage".into());
                        }
                        eprintln!("PERF native cached/fresh glyph layouts identical: {verified}");
                        // Outside timing: a changed cell and centered popup must use the
                        // same cache without freezing content or reusing absolute positions.
                        view.update(cx, |view, cx| -> Result<(), String> {
                            let mut surface = (**view
                                .live
                                .surface
                                .as_ref()
                                .ok_or("missing fixture surface")?)
                            .clone();
                            surface.frame.cells[0].fg = 0x02ff55ee;
                            surface.popup = Some(Box::new(ClientShellPopupSurface {
                                terminal_id: "perf-popup".into(),
                                title: "Review".into(),
                                width: None,
                                height: None,
                                mouse_reporting: false,
                                sgr_pixel_mouse: false,
                                pixel_width: 0,
                                pixel_height: 0,
                                frame: FrameData {
                                    width: 20,
                                    height: 4,
                                    cells: surface.frame.cells[..80].to_vec(),
                                    cursor: Some(CursorState {
                                        x: 1,
                                        y: 1,
                                        visible: true,
                                        shape: 3,
                                    }),
                                    hyperlinks: vec![],
                                    graphics: vec![],
                                },
                            }));
                            view.set_surface(Some(Arc::new(surface)), cx);
                            cx.notify();
                            Ok(())
                        })?;
                        for redraw in 0..2 {
                            *cx.default_global::<Counts>() = Counts::default();
                            if !retained {
                                window.refresh();
                            }
                            window.draw(cx).clear();
                            let c = cx.global::<Counts>();
                            if expect_retained && redraw == 1 {
                                if c.paints != 0 || c.metric_shapes != 0 || c.paint_errors != 0
                                    || c.run_shapes != 0 || c.runs != 0
                                {
                                    return Err(format!("unchanged popup repainted: {c:?}"));
                                }
                            } else if c.quads != 54
                                || c.decorations != 1922
                                || c.glyphs <= 6981
                                || c.paint_errors != 0
                                || (redraw == 1 && (c.shapes != 0 || c.run_shapes != 0))
                            {
                                return Err(format!("popup redraw {redraw}: {c:?}"));
                            }
                        }
                        view.read(cx).painter.borrow().verify_native_cache(window)?;
                        view.update(cx, |view, cx| {
                            view.set_surface(Some(Arc::new(surface())), cx);
                            cx.notify();
                        });
                        *cx.default_global::<Counts>() = Counts::default();
                        window.draw(cx).clear();
                        let c = cx.global::<Counts>();
                        if c.paints == 0
                            || c.quads != 50 * c.paints
                            || c.glyphs != 6981 * c.paints
                            || c.decorations != 1921 * c.paints
                            || c.paint_errors != 0
                        {
                            return Err(format!("popup hide stale content: {c:?}"));
                        }
                        eprintln!(
                            "PERF native changed-cell + centered popup + underline cursor verified"
                        );
                    }
                    Ok(())
                },
            );
            if !matches!(result, Ok(Ok(()))) {
                eprintln!("PERF FAIL: {result:?}");
                std::process::exit(1);
            }
        }
        // Construct all input outside the timed setter + scene-construction region.
        let mut updates = Vec::new();
        let mut current = surface();
        for category in [
            "cursor",
            "single-cell",
            "erasure",
            "stream-scroll",
            "full-screen",
            "mixed-fallback",
        ] {
            for sample in 0..12 {
                current.surface_revision += 1;
                match category {
                    "cursor" => {
                        current.frame.cursor = Some(CursorState {
                            x: sample + 20,
                            y: 25,
                            shape: 1 + (sample % 6) as u8,
                            visible: sample % 3 != 0,
                        });
                    }
                    "single-cell" | "erasure" => {
                        let cell = &mut current.frame.cells[usize::from(sample) * 160 + 8];
                        cell.symbol = if category == "erasure" { " " } else { "@" }.into();
                        cell.fg = 0x02ff55ee;
                    }
                    "stream-scroll" => {
                        current.frame.cells.rotate_left(160);
                        for (col, cell) in current.frame.cells[49 * 160..].iter_mut().enumerate() {
                            cell.symbol =
                                char::from(b'a' + ((col + usize::from(sample)) % 26) as u8)
                                    .to_string();
                            cell.skip = false;
                        }
                    }
                    "mixed-fallback" => {
                        for cell in &mut current.frame.cells {
                            cell.modifier = 8;
                        }
                        // A single eligible pair must not stage every decorated row.
                        current.frame.cells[0].modifier = 0;
                        current.frame.cells[1].modifier = 0;
                        current.frame.cells[1].fg = current.frame.cells[0].fg;
                        current.frame.cells[0].symbol = char::from(b'a' + sample as u8).to_string();
                    }
                    _ => {
                        for (index, cell) in current.frame.cells.iter_mut().enumerate() {
                            cell.symbol =
                                char::from(b'A' + ((index + usize::from(sample)) % 26) as u8)
                                    .to_string();
                            cell.skip = false;
                            cell.fg = [0x0298c379, 0x02e5c07b][(index + usize::from(sample)) % 2];
                            cell.bg = [0, 0x02232b36][(index / 160 + usize::from(sample)) % 2];
                            cell.modifier = [1, 4, 8, 256][(index / 160 + usize::from(sample)) % 4];
                        }
                    }
                }
                updates.push((category, Some(Arc::new(current.clone()))));
            }
        }
        updates.push(("clear", None));
        updates.push(("restore", Some(Arc::new(surface()))));
        let mut update_samples = std::collections::BTreeMap::<&str, Vec<f64>>::new();
        for (category, surface) in updates {
            let result = AnyWindowHandle::from(handle).update(
                cx,
                |root, window, cx| -> Result<(), String> {
                    let view = root
                        .downcast::<HerdrWindow>()
                        .map_err(|_| "unexpected root")?;
                    let has_surface = surface.is_some();
                    *cx.default_global::<Counts>() = Counts::default();
                    let start = Instant::now();
                    view.update(cx, |view, cx| {
                        view.set_surface(surface, cx);
                        cx.notify();
                    });
                    // Never refresh here: that would mask missed entity invalidation.
                    window.draw(cx).clear();
                    update_samples
                        .entry(category)
                        .or_default()
                        .push(start.elapsed().as_secs_f64() * 1000.);
                    let changed = *cx.global::<Counts>();
                    if (has_surface && (changed.paints == 0 || changed.glyphs == 0))
                        || (!has_surface && (changed.paints != 0 || changed.glyphs != 0))
                        || changed.paint_errors != 0
                        || changed.metric_shapes != 0
                    {
                        return Err(format!("{category} invalidation: {changed:?}"));
                    }
                    *cx.default_global::<Counts>() = Counts::default();
                    window.draw(cx).clear();
                    let unchanged = *cx.global::<Counts>();
                    if unchanged.paint_errors != 0
                        || unchanged.metric_shapes != 0
                        || (expect_retained && (unchanged.paints != 0 || unchanged.shapes != 0
                            || unchanged.run_shapes != 0 || unchanged.runs != 0))
                    {
                        return Err(format!("{category} unchanged draw: {unchanged:?}"));
                    }
                    // Compare the unforced update with a fresh scene, outside timing.
                    *cx.default_global::<Counts>() = Counts::default();
                    window.refresh();
                    window.draw(cx).clear();
                    let fresh = *cx.global::<Counts>();
                    if changed.paints != fresh.paints
                        || changed.quads != fresh.quads
                        || changed.glyphs != fresh.glyphs
                        || changed.decorations != fresh.decorations
                        || changed.runs != fresh.runs
                        || fresh.paint_errors != 0
                        || fresh.metric_shapes != 0
                        || (!uncached && (fresh.shapes != 0 || fresh.run_shapes != 0))
                    {
                        return Err(format!(
                            "{category} fresh scene mismatch: {changed:?} / {fresh:?}"
                        ));
                    }
                    if !uncached {
                        view.read(cx).painter.borrow().verify_native_cache(window)?;
                    }
                    // A status-only root notification must not invalidate the terminal child.
                    *cx.default_global::<Counts>() = Counts::default();
                    view.update(cx, |view, cx| {
                        view.live.status = state::ConnectionStatus::Connected;
                        view.local_error = Some(format!("Performance {category}"));
                        cx.notify();
                    });
                    window.draw(cx).clear();
                    let status = *cx.global::<Counts>();
                    if status.paint_errors != 0
                        || status.metric_shapes != 0
                        || (expect_retained && (status.paints != 0 || status.shapes != 0
                            || status.run_shapes != 0 || status.runs != 0))
                    {
                        return Err(format!("{category} root-only notification: {status:?}"));
                    }
                    Ok(())
                },
            );
            if !matches!(result, Ok(Ok(()))) {
                eprintln!("PERF FAIL: {result:?}");
                std::process::exit(1);
            }
        }
        // Untimed native geometry acceptance: keep the surface identity unchanged.
        let resize = AnyWindowHandle::from(handle).update(cx, |root, window, cx| {
            let view = root.downcast::<HerdrWindow>().map_err(|_| "unexpected root")?;
            let before = view.read(cx).bounds;
            let surface = view.read(cx).live.surface.clone().ok_or("missing resize surface")?;
            let target = window.viewport_size() - size(px(120.), px(100.));
            *cx.default_global::<Counts>() = Counts::default();
            window.resize(target);
            Ok::<_, &str>((before, surface, target))
        });
        let Ok(Ok((before, resize_surface, target))) = resize else {
            eprintln!("PERF FAIL preparing resize: {resize:?}");
            std::process::exit(1);
        };
        let mut resized = false;
        for _ in 0..50 {
            timer.timer(Duration::from_millis(20)).await;
            let result = AnyWindowHandle::from(handle).update(cx, |root, window, cx| -> Result<bool, String> {
                if window.viewport_size() != target {
                    return Ok(false);
                }
                let view = root.downcast::<HerdrWindow>().map_err(|_| "unexpected root")?;
                window.draw(cx).clear();
                let bounds = view.read(cx).bounds;
                let current = view.read(cx).live.surface.as_ref().ok_or("resize cleared surface")?;
                let counts = *cx.global::<Counts>();
                if bounds == before || bounds.size.width <= px(0.) || bounds.size.height <= px(0.)
                    || !Arc::ptr_eq(current, &resize_surface) || counts.paints == 0
                    || counts.paint_errors != 0 || counts.metric_shapes != 0
                    || (!uncached && (counts.shapes != 0 || counts.run_shapes != 0))
                    || (!uncached && (counts.quads != 50 * counts.paints
                        || counts.glyphs != 6981 * counts.paints
                        || counts.decorations != 1921 * counts.paints))
                    || (batched && counts.runs <= 100 * counts.paints)
                {
                    return Err(format!("resize invalidation {before:?} -> {bounds:?}: {counts:?}"));
                }
                if !uncached {
                    view.read(cx).painter.borrow().verify_native_cache(window)?;
                }
                *cx.default_global::<Counts>() = Counts::default();
                window.draw(cx).clear();
                let warm = cx.global::<Counts>();
                if warm.paint_errors != 0 || warm.metric_shapes != 0
                    || (!uncached && (warm.shapes != 0 || warm.run_shapes != 0))
                    || (expect_retained && (warm.paints != 0 || warm.runs != 0))
                {
                    return Err(format!("resized geometry did not retain: {warm:?}"));
                }
                eprintln!("PERF native resize verified {before:?} -> {bounds:?}: {counts:?}, warm={warm:?}");
                Ok(true)
            });
            match result {
                Ok(Ok(true)) => { resized = true; break; }
                Ok(Ok(false)) => {}
                error => {
                    eprintln!("PERF FAIL native resize: {error:?}");
                    std::process::exit(1);
                }
            }
        }
        if !resized {
            eprintln!("PERF FAIL native resize did not settle at {target:?}");
            std::process::exit(1);
        }

        // Use a real pane/agent pair. Same revision plus a higher event sequence
        // must acknowledge completion even when the terminal scene is retained.
        let _ = AnyWindowHandle::from(handle).update(cx, |_, window, cx| {
            cx.activate(true);
            window.activate_window();
        });
        for _ in 0..50 {
            timer.timer(Duration::from_millis(20)).await;
            if matches!(AnyWindowHandle::from(handle).update(cx, |_, window, _| window.is_window_active()), Ok(true)) {
                break;
            }
        }
        for phase in 0..3 {
            let result = AnyWindowHandle::from(handle).update(cx, |root, window, cx| -> Result<(), String> {
                use herdr_client::protocol::{AgentStatus, ClientShellSnapshot, PaneSurfacePane, SurfaceRect};
                let view = root.downcast::<HerdrWindow>().map_err(|_| "unexpected root")?;
                if !window.is_window_active() {
                    return Err("acknowledgement fixture requires an active native window".into());
                }
                if phase == 0 {
                    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
                        "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
                    )).map_err(|error| error.to_string())?;
                    snapshot.agents[0].agent_status = AgentStatus::Working;
                    snapshot.agents[0].state_change_seq = 100;
                    let mut frame = surface();
                    frame.boot_id = snapshot.boot_id.clone();
                    frame.projection_revision = snapshot.revision;
                    frame.panes = snapshot.panes.iter().map(|pane| {
                        let rect = SurfaceRect { x: 0, y: 0, width: frame.frame.width, height: frame.frame.height };
                        PaneSurfacePane {
                            pane_id: pane.pane_id.clone(), content_revision: 1,
                            rect, inner_rect: rect, scrollbar_rect: None, scroll: None,
                            focused: pane.focused, mouse_reporting: false, sgr_pixel_mouse: false,
                            alternate_screen_active: false, pixel_width: 0, pixel_height: 0,
                        }
                    }).collect();
                    let frame = Arc::new(frame);
                    view.update(cx, |view, cx| -> Result<(), String> {
                        let mut state = LiveState::default();
                        state.set_outer_focus(true);
                        state.apply(ClientEvent::Snapshot(Arc::new(snapshot)));
                        state.apply(ClientEvent::Surface(frame.clone()));
                        *view.connection.inbox.lock().map_err(|_| "poisoned fixture inbox")? = state.clone();
                        view.live = state;
                        view.set_surface(Some(frame), cx);
                        cx.notify();
                        Ok(())
                    })?;
                    *cx.default_global::<Counts>() = Counts::default();
                    window.draw(cx).clear();
                    let counts = cx.global::<Counts>();
                    if counts.paints == 0 || counts.paint_errors != 0 {
                        return Err(format!("acknowledgement prime did not paint: {counts:?}"));
                    }
                } else if phase == 1 {
                    view.update(cx, |view, cx| -> Result<(), String> {
                        let surface = view.live.surface.clone().ok_or("missing primed surface")?;
                        let mut state = view.connection.inbox.lock().map_err(|_| "poisoned fixture inbox")?;
                        let mut snapshot = (**state.snapshot.as_ref().ok_or("missing primed snapshot")?).clone();
                        snapshot.agents[0].agent_status = AgentStatus::Idle;
                        snapshot.agents[0].state_change_seq += 1;
                        state.apply(ClientEvent::Snapshot(Arc::new(snapshot)));
                        state.set_outer_focus(true);
                        if state.snapshot.as_ref().ok_or("missing completion snapshot")?.agents[0].agent_status != AgentStatus::Done
                            || !state.surface.as_ref().is_some_and(|current| Arc::ptr_eq(current, &surface))
                        {
                            return Err("completion was acknowledged before draw or changed surface identity".into());
                        }
                        view.live = state.clone();
                        drop(state);
                        // Deliberately only notify the root, not the terminal child.
                        cx.notify();
                        Ok(())
                    })?;
                    *cx.default_global::<Counts>() = Counts::default();
                    window.draw(cx).clear();
                    let counts = cx.global::<Counts>();
                    if counts.paint_errors != 0 || counts.metric_shapes != 0
                        || (!uncached && (counts.shapes != 0 || counts.run_shapes != 0))
                        || (expect_retained && (counts.paints != 0 || counts.runs != 0))
                    {
                        return Err(format!("completion root-only draw repainted: {counts:?}"));
                    }
                } else {
                    // Leaving the preceding window update flushes cx.defer; do not
                    // draw again or manually acknowledge before inspecting the inbox.
                    let live = &view.read(cx).live;
                    let state = view.read(cx).connection.inbox.lock().map_err(|_| "poisoned fixture inbox")?;
                    if live.snapshot.as_ref().ok_or("missing presented snapshot")?.agents[0].agent_status != AgentStatus::Done
                        || state.snapshot.as_ref().ok_or("missing acknowledged snapshot")?.agents[0].agent_status != AgentStatus::Idle
                        || !Arc::ptr_eq(live.surface.as_ref().ok_or("missing presented surface")?, state.surface.as_ref().ok_or("missing acknowledged surface")?)
                        || (expect_retained && cx.global::<Counts>().paints != 0)
                    {
                        return Err("retained root draw did not defer completion acknowledgement".into());
                    }
                    eprintln!("PERF coherent pane Working -> Done -> Idle acknowledged after root-only draw: {:?}", cx.global::<Counts>());
                }
                Ok(())
            });
            if !matches!(result, Ok(Ok(()))) {
                eprintln!("PERF FAIL acknowledgement phase={phase}: {result:?}");
                std::process::exit(1);
            }
        }
        for (category, samples) in &mut update_samples {
            report(category, samples);
        }
        report("first-content", &mut cold);
        report("cache-cold", &mut cache_cold);
        let p95 = report("warm-hover", &mut hover).max(report("warm-scroll", &mut scroll));
        if p95 <= budget {
            eprintln!("PERF PASS p95_budget_ms={budget}");
            if std::env::var_os("HERDR_PERF_SAMPLES").is_some() {
                println!(
                    "{}",
                    serde_json::json!({"result": "PASS", "retained": retained})
                );
            } else {
                println!("PERF PASS retained={retained}");
            }
            std::process::exit(0);
        } else {
            eprintln!("PERF FAIL p95_budget_ms={budget}");
            std::process::exit(1);
        }
    })
    .detach();
}
