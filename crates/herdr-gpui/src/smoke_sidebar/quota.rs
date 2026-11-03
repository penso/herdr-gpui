//! Isolated native account warning, without unrelated sidebar scenarios.
use super::*;

pub(super) async fn run(handle: WindowHandle<HerdrWindow>, cx: &mut AsyncApp) -> Result<()> {
    for (theme, label) in [("Catppuccin Mocha", "dark"), ("Catppuccin Latte", "light")] {
        for width in [360., 1200.] {
            handle.update(cx, |view, window, cx| -> Result<()> {
                view.config.theme = theme.into();
                view.theme = view.config.theme(label == "light")?;
                view.github_fixture(false, window, cx);
                view.menu.github = github::Auth::connected_fixture();
                view.menu.github.seed_quota(herdr_github::quota::Quota {
                    limit: 5000,
                    used: 5000,
                    remaining: 0,
                    reset: None,
                });
                view.menu.pr_cache.seed_rate_limit(
                    "fixture-user",
                    Instant::now() + Duration::from_secs(29 * 60),
                );
                window.resize(size(px(width), px(600.)));
                cx.notify();
                Ok(())
            })??;
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            AnyWindowHandle::from(handle).update(cx, |root, window, cx| -> Result<()> {
                let view = root
                    .downcast::<HerdrWindow>()
                    .map_err(|_| anyhow!("unexpected root"))?;
                let view = view.read(cx);
                if !view
                    .github_rate_limit_message()
                    .is_some_and(|message| message.contains("29"))
                {
                    bail!("missing countdown");
                }
                if view.menu.github.busy() {
                    bail!("opening the warning started sign-in");
                }
                window.draw(cx).clear(cx);
                Ok(())
            })??;
            #[cfg(feature = "mockup")]
            if let Some(path) = std::env::var_os("HERDR_TEST_GITHUB_CAPTURE") {
                let path =
                    std::path::PathBuf::from(path).with_extension(format!("{label}-{width}.png"));
                if !path.is_absolute() {
                    bail!("capture destination must be absolute");
                }
                let image = AnyWindowHandle::from(handle)
                    .update(cx, |_, window, _| window.render_to_image())??;
                cx.background_executor()
                    .spawn(async move {
                        image
                            .save(&path)
                            .context("save native GitHub quota capture")
                    })
                    .await?;
            }
            handle.update(cx, |view, window, cx| -> Result<()> {
                view.menu
                    .pr_cache
                    .seed_rate_limit("fixture-user", Instant::now());
                if view.github_rate_limit_message().is_some() {
                    bail!("warning survived reset");
                }
                view.dismiss_menu(window, cx);
                if !view.focus.is_focused(window) {
                    bail!("account dialog lost terminal focus");
                }
                Ok(())
            })??;
        }
    }
    Ok(())
}
