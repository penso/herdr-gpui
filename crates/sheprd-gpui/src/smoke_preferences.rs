//! Native Settings window checks run by the sidebar smoke driver.
use super::*;

pub(super) async fn sidebar_preferences(
    handle: WindowHandle<SheprdWindow>,
    cx: &mut AsyncApp,
) -> Result<()> {
    handle.update(cx, |view, _, cx| view.load_shared_settings(cx))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let loaded = handle.update(cx, |view, _, _| -> Result<bool> {
            if let Some(error) = &view.settings.error {
                bail!("shared settings load: {error}");
            }
            Ok(view.settings.loaded
                && view.settings.task.is_none()
                && view.settings.shared.is_some())
        })??;
        if loaded {
            break;
        }
        if Instant::now() >= deadline {
            bail!("shared settings load timed out");
        }
        cx.background_executor()
            .timer(Duration::from_millis(10))
            .await;
    }
    // Plugin output for Settings > Plugins to list and preview: an agent
    // summary (long on one agent), a model, a workspace CI value, and labels.
    handle.update(cx, |view, _, cx| {
        view.live.status = ConnectionStatus::Connected;
        if let Some(snapshot) = view.live.snapshot.as_mut() {
            let snapshot = Arc::make_mut(snapshot);
            for (index, agent) in snapshot.agents.iter_mut().enumerate() {
                let summary = if index == 0 {
                    "Migrating the settings window to the shared switch component"
                } else {
                    "refactor auth middleware"
                };
                agent.tokens = vec![
                    ("summary".into(), summary.into()),
                    ("model".into(), "opus".into()),
                ];
                agent.state_labels = vec![("working".into(), "reviewing auth".into())];
            }
            for workspace in &mut snapshot.workspaces {
                workspace.tokens = vec![("ci".into(), "green \u{b7} 2m ago".into())];
            }
        }
        cx.notify();
    })?;
    crate::settings_window::verify_native(handle, cx).await?;
    Ok(())
}
