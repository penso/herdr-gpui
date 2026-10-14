//! Real native clicks and config reloads while the independent Settings window stays open.
use super::*;
use std::collections::HashMap;

mod saved;

#[derive(Default)]
struct ProviderBounds(HashMap<&'static str, Bounds<Pixels>>);
impl Global for ProviderBounds {}

pub(in crate::settings_window) fn probe(id: &'static str) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, _, cx| {
            cx.default_global::<ProviderBounds>().0.insert(id, bounds);
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

pub(super) async fn verify(
    settings: WindowHandle<SettingsWindow>,
    source: WindowHandle<HerdrWindow>,
    expected: Size<Pixels>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let fixture = crate::usage::native_fixture()?;
    let original = source.update(cx, |view, _, cx| {
        let usage = std::mem::replace(&mut view.usage, fixture);
        let original = (
            view.config_watch.is_none(),
            view.config.clone(),
            view.theme.clone(),
            usage,
            view.status_bar_visible,
            view.configured_terminal_size,
        );
        // Sidebar fixtures skip connection startup, including the normal watcher.
        if original.0 {
            view.watch_gui_config(cx);
        }
        cx.notify();
        original
    })?;
    settings.update(cx, |view, window, cx| {
        view.body_scroll.set_offset(Point::default());
        cx.activate(true);
        window.activate_window();
        cx.notify();
    })?;
    // AppKit deactivation and Settings activation arrive separately.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let (inactive, eligible, active_window) = source.update(cx, |view, _, cx| {
            (
                !view.active,
                usage_source_active(cx.entity_id(), cx),
                cx.active_window(),
            )
        })?;
        if inactive && eligible {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "Settings activation did not settle: source_inactive={inactive}, eligible={eligible}, active_window={active_window:?}, settings={settings:?}"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    saved::verify(settings, source, cx).await?;
    let scroll =
        AnyWindowHandle::from(settings).update(cx, |root, window, cx| -> Result<_> {
            window.draw(cx).clear(cx);
            let view = root
                .downcast::<SettingsWindow>()
                .map_err(|_| anyhow::anyhow!("unexpected Settings root"))?;
            let chip = cx
                .global::<ProviderBounds>()
                .0
                .get("claude")
                .context("missing detected Claude chip")?;
            Ok(view.read(cx).body_scroll.bounds().center().y - chip.center().y)
        })??;
    settings.update(cx, |view, _, cx| {
        view.body_scroll.set_offset(point(px(0.), scroll));
        cx.notify();
    })?;
    for hidden in [true, false] {
        #[cfg(feature = "mockup")]
        if hidden {
            capture_settings(settings, "usage-before.png", cx).await?;
        }
        let (target, bounds) =
            AnyWindowHandle::from(settings).update(cx, |root, window, cx| -> Result<_> {
                cx.default_global::<ProviderBounds>().0.clear();
                window.draw(cx).clear(cx);
                let view = root
                    .downcast::<SettingsWindow>()
                    .map_err(|_| anyhow::anyhow!("unexpected Settings root"))?;
                ensure!(
                    view.read(cx).config.usage.show && !view.read(cx).busy(),
                    "native provider controls not ready: show={}, busy={}",
                    view.read(cx).config.usage.show,
                    view.read(cx).busy()
                );
                let bounds = &cx.global::<ProviderBounds>().0;
                ensure!(
                    bounds.len() == 2
                        && bounds.contains_key("codex")
                        && bounds.contains_key("claude"),
                    "Settings showed undetected services: {bounds:?}"
                );
                let bounds = bounds["claude"];
                let viewport = view.read(cx).body_scroll.bounds();
                ensure!(
                    bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom(),
                    "native provider chip clipped at {expected:?}: {bounds:?}"
                );
                Ok((Target::acquire(window)?, bounds))
            })??;
        target.click(
            f64::from(f32::from(bounds.center().x)),
            f64::from(f32::from(bounds.center().y)),
        )?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let saved = settings.update(cx, |view, _, _| -> Result<_> {
                ensure!(
                    view.error.is_none(),
                    "native provider save failed: {:?}",
                    view.error
                );
                Ok(!view.busy()
                    && view
                        .config
                        .usage
                        .hide_providers
                        .iter()
                        .any(|id| id == "claude")
                        == hidden)
            })??;
            AnyWindowHandle::from(source).update(cx, |_, window, cx| window.draw(cx).clear(cx))?;
            let applied = source.update(cx, |view, _, cx| -> Result<_> {
                // Sidebar fixtures disable the timer; step its real usage phase.
                view.native_update_usage(cx);
                ensure!(!view.active, "source activated before Settings closed");
                let Some(entry) = view.usage.current() else {
                    return Ok(false);
                };
                let shown = entry.headline(
                    &view.config.usage,
                    crate::usage::HEADLINE,
                    view.usage.chosen(),
                );
                let claude_shown = shown
                    .iter()
                    .any(|reading| reading.provider.id() == "claude");
                let codex_shown = shown.iter().any(|reading| reading.provider.id() == "codex");
                let claude_fetched = shown.iter().any(|reading| {
                    reading.provider.id() == "claude"
                        && reading
                            .report
                            .as_ref()
                            .is_some_and(|report| !report.windows.is_empty())
                });
                Ok(view
                    .config
                    .usage
                    .hide_providers
                    .iter()
                    .any(|id| id == "claude")
                    == hidden
                    && claude_shown != hidden
                    && codex_shown
                    && !view.usage.busy()
                    && (hidden || claude_fetched))
            })??;
            if saved && applied {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "provider visibility did not apply with Settings open (hidden={hidden}, saved={saved}, applied={applied})"
            );
            cx.background_executor()
                .timer(Duration::from_millis(10))
                .await;
        }
        if hidden {
            source.update(cx, |view, _, cx| {
                view.usage.native_forget_claude_report()?;
                cx.notify();
                Ok::<_, anyhow::Error>(())
            })??;
        }
        #[cfg(feature = "mockup")]
        if hidden {
            capture_settings(
                settings,
                if expected.width == px(960.) {
                    "usage.png"
                } else {
                    "usage-narrow.png"
                },
                cx,
            )
            .await?;
        }
    }
    settings.update(cx, |view, _, cx| {
        view.body_scroll.set_offset(Point::default());
        cx.notify();
    })?;
    source.update(cx, move |view, _, cx| {
        if original.0 {
            view.config_watch = None;
        }
        // Real reloads replace the entire config, not only the usage flags.
        // Restore the scripted sidebar's typography and layout for later checks.
        view.config = original.1;
        view.theme = original.2;
        view.usage = original.3;
        view.status_bar_visible = original.4;
        view.configured_terminal_size = original.5;
        cx.notify();
    })?;
    eprintln!(
        "SIDEBAR native usage settings PASS at {expected:?}: only Codex/Claude detected, native disable/re-enable and fresh quota reads update inactive source before Settings closes"
    );
    Ok(())
}
