use super::*;

pub(super) async fn verify(
    settings: WindowHandle<SettingsWindow>,
    source: WindowHandle<HerdrWindow>,
    cx: &mut AsyncApp,
) -> Result<()> {
    let saved: crate::usage::UsageConfig = toml::from_str(
        "show = true\nhide_providers = ['openrouter', 'codex']\n[providers.openrouter]\napi_key = 'native-fixture-not-a-real-key'\n",
    )?;
    let original_settings = settings.update(cx, |view, _, cx| {
        let original = std::mem::replace(&mut view.config.usage, saved);
        cx.notify();
        original
    })?;
    let original_source = source.update(cx, |view, _, cx| {
        // No quota worker or cached report: saved preferences alone must
        // keep both the API-key-only and hide-only choices visible.
        let original = (std::mem::take(&mut view.usage), view.config.usage.show);
        view.config.usage.show = false;
        cx.notify();
        original
    })?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let visible = AnyWindowHandle::from(settings).update(cx, |_, window, cx| {
            cx.default_global::<ProviderBounds>().0.clear();
            window.draw(cx).clear(cx);
            let bounds = &cx.global::<ProviderBounds>().0;
            bounds.contains_key("openrouter")
                && bounds.contains_key("codex")
                && !bounds.contains_key("claude")
        })?;
        if visible {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "saved API-key-only/hide-only provider choices vanished without cached reports"
        );
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    let scroll = AnyWindowHandle::from(settings).update(cx, |root, _, cx| -> Result<_> {
        let view = root
            .downcast::<SettingsWindow>()
            .map_err(|_| anyhow::anyhow!("unexpected Settings root"))?;
        let chip = cx.global::<ProviderBounds>().0["openrouter"];
        Ok(view.read(cx).body_scroll.bounds().center().y - chip.center().y)
    })??;
    settings.update(cx, |view, _, cx| {
        view.body_scroll.set_offset(point(px(0.), scroll));
        cx.notify();
    })?;
    AnyWindowHandle::from(settings).update(cx, |root, window, cx| -> Result<()> {
        window.draw(cx).clear(cx);
        let view = root
            .downcast::<SettingsWindow>()
            .map_err(|_| anyhow::anyhow!("unexpected Settings root"))?;
        let viewport = view.read(cx).body_scroll.bounds();
        for id in ["openrouter", "codex"] {
            let bounds = cx.global::<ProviderBounds>().0[id];
            ensure!(
                bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom(),
                "saved provider {id} is clipped: {bounds:?}"
            );
        }
        Ok(())
    })??;
    source.update(cx, |view, _, _| -> Result<()> {
        ensure!(
            view.usage.current().is_none() && !view.usage.busy(),
            "saved provider visibility required a quota read"
        );
        Ok(())
    })??;
    #[cfg(feature = "mockup")]
    capture_settings(settings, "usage-saved.png", cx).await?;
    settings.update(cx, |view, _, cx| {
        view.config.usage = original_settings;
        view.body_scroll.set_offset(Point::default());
        cx.notify();
    })?;
    source.update(cx, |view, _, cx| {
        view.usage = original_source.0;
        view.config.usage.show = original_source.1;
        cx.notify();
    })?;
    // Allow the source observer to replace the temporary empty detection mask.
    cx.background_executor()
        .timer(Duration::from_millis(10))
        .await;
    eprintln!(
        "SIDEBAR native saved usage providers PASS: API-key-only OpenRouter and hide-only Codex stay selectable with no cached quota report or quota read"
    );
    Ok(())
}
