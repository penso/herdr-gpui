//! Loading and saving preferences off the UI thread, then reconciling the result,
//! including the config watcher and the saves that finish when the app quits.
use super::{SettingsWindow, layouts, themes};
use crate::{
    config::{Config, FontFace, Theme},
    herdr_settings::{self, Edit},
};
use gpui::*;

pub(super) struct Loaded {
    pub(super) config: Config,
    pub(super) theme: Theme,
    pub(super) shared: Option<herdr_settings::Settings>,
    pub(super) error: Option<String>,
}

pub(super) type SaveCompletion =
    std::result::Result<Option<herdr_settings::Settings>, std::sync::Arc<crate::Error>>;

#[cfg(test)]
type SizeWriter = dyn Fn(Vec<(FontFace, f32)>) -> crate::Result<()> + Send + Sync;

#[cfg(test)]
#[derive(Clone)]
pub(super) struct SizeIo {
    pub(super) write: std::sync::Arc<SizeWriter>,
    pub(super) load: fn() -> crate::Result<Loaded>,
}

impl SettingsWindow {
    pub(super) fn watch_config(&mut self, cx: &mut Context<Self>) {
        let Ok(path) = Config::local_path() else {
            return;
        };
        let daemon = crate::config::daemon_config_path(|key| std::env::var_os(key));
        self._watch = Some(cx.spawn(async move |this, cx| {
            let mut watch = crate::config::watch::Watch::default();
            let mut pending = None;
            loop {
                let (path, daemon) = (path.clone(), daemon.clone());
                let sample = cx
                    .background_executor()
                    .spawn(async move {
                        use crate::config::watch::fingerprint;
                        [fingerprint(&path), fingerprint(&daemon)]
                    })
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if let Some((sample, revision)) = pending
                            && revision != this.load_revision
                        {
                            watch.accept(sample);
                            pending = None;
                        }
                        if watch.observe(sample) && !this.busy() {
                            pending = Some((sample, this.load_revision));
                            this.reload(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
            }
        }));
    }

    pub(super) fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<crate::Result<()>> {
        #[cfg(test)]
        if let Some(io) = self.size_io.clone() {
            return self.shutdown_with(move |sizes| (io.write)(sizes), cx);
        }
        self.shutdown_with(|sizes| Config::save_font_sizes(&sizes), cx)
    }

    pub(super) fn shutdown_with(
        &mut self,
        write: impl FnOnce(Vec<(FontFace, f32)>) -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) -> Task<crate::Result<()>> {
        self.quitting = true;
        self._watch = None;
        let pending = self.take_pending_control_sizes();
        let theme = self.take_shutdown_theme();
        let layout = self
            .layout_intent
            .take()
            .filter(|_| !self.layout_saving)
            .map(|mode| self.layout_operation(mode));
        let completion = self.save_completion.take();
        let executor = cx.background_executor().clone();
        cx.background_executor().spawn(async move {
            let mut shared = None;
            let mut preceding = Ok(());
            if let Some(completion) = completion {
                // Yield rather than occupying the executor thread needed by
                // the preceding save, including single-threaded test workers.
                loop {
                    match completion.try_recv() {
                        Ok(result) => {
                            match result {
                                Ok(snapshot) => shared = snapshot,
                                Err(error) => preceding = Err(crate::Error::SettingsSave(error)),
                            }
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            executor.timer(std::time::Duration::from_millis(10)).await;
                        }
                    }
                }
            }
            let sizes = if pending.is_empty() {
                Ok(())
            } else {
                write(pending)
            };
            let theme = theme.map_or(Ok(()), |theme| theme(shared));
            let layout = layout.map_or(Ok(()), |write| write());
            preceding.and(sizes).and(theme).and(layout)
        })
    }

    pub(super) fn save_control_sizes(
        &mut self,
        sizes: Vec<(FontFace, f32)>,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        if let Some(io) = self.size_io.clone() {
            self.save_with(move || (io.write)(sizes), io.load, false, cx);
            return;
        }
        self.save_native(move || Config::save_font_sizes(&sizes), cx);
    }

    pub(super) fn loader(cx: &App) -> impl FnOnce() -> crate::Result<Loaded> + Send + 'static {
        let light = matches!(
            cx.window_appearance(),
            WindowAppearance::Light | WindowAppearance::VibrantLight
        );
        let text_system = cx.text_system().clone();
        move || {
            let mut config = Config::load()?;
            config.resolve_font_fallbacks(|| text_system.all_font_names());
            let (shared, error, theme) = match herdr_settings::Settings::load() {
                Ok(shared) => {
                    config.apply_shared_notifications(&shared);
                    let theme = if config.theme == "Follow Herdr" {
                        shared.theme(light)?.with_contrast(config.contrast)
                    } else {
                        config.theme()?
                    };
                    (Some(shared), None, theme)
                }
                Err(error) if config.theme == "Follow Herdr" => return Err(error),
                Err(error) => (
                    None,
                    Some(format!("Load shared settings: {error}")),
                    config.theme()?,
                ),
            };
            Ok(Loaded {
                config,
                theme,
                shared,
                error,
            })
        }
    }

    pub(super) fn apply_loaded(&mut self, loaded: crate::Result<Loaded>, cx: &mut Context<Self>) {
        self.load_revision = self.load_revision.wrapping_add(1);
        let valid = loaded.is_ok();
        match loaded {
            Ok(loaded) => {
                let live = self
                    .theme_intent
                    .as_ref()
                    .map(|_| (self.config.theme.clone(), self.theme.clone()));
                self.config = loaded.config;
                if let Some(mode) = self.layout_intent {
                    self.config.layout.mode = mode;
                }
                self.theme = loaded.theme;
                if let Some((name, theme)) = live {
                    self.config.theme = name;
                    self.theme = theme;
                }
                self.shared = loaded.shared;
                self.error = loaded.error;
            }
            Err(error) => {
                self.error = Some(format!(
                    "Could not reload settings; keeping current preferences: {error}"
                ))
            }
        }
        // Appearance may have changed while the loader was running. Resolve
        // Follow Herdr from the prepared snapshot against the current OS mode.
        self.sync_appearance(cx);
        self.drive_theme_intent(cx);
        if valid {
            self.publish_appearance(cx);
        }
        cx.notify();
    }

    pub(super) fn reload(&mut self, cx: &mut Context<Self>) {
        self.reload_with(Self::loader(cx), cx);
    }

    pub(super) fn reload_with(
        &mut self,
        load: impl FnOnce() -> crate::Result<Loaded> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy() {
            return;
        }
        self.loading = true;
        self.error = None;
        // Size intents can arrive while this read is running. Keep the model
        // alive until reconciliation has transferred them to the save worker.
        let retained = cx.entity();
        let load = cx.background_executor().spawn(async move { load() });
        cx.spawn(async move |_, cx| {
            let loaded = load.await;
            retained.update(cx, |this, cx| {
                this.loading = false;
                if this.quitting {
                    return;
                }
                this.apply_loaded(loaded, cx);
                this.finish_close(cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn save_native(
        &mut self,
        operation: impl FnOnce() -> crate::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.save_with(operation, Self::loader(cx), false, cx);
    }

    pub(super) fn save_shared(&mut self, edit: Edit, cx: &mut Context<Self>) {
        let Some(shared) = self.shared.clone() else {
            return;
        };
        self.save_with(
            move || shared.save(edit).map(|_| ()),
            Self::loader(cx),
            true,
            cx,
        );
    }

    pub(super) fn save_with(
        &mut self,
        operation: impl FnOnce() -> crate::Result<()> + Send + 'static,
        load: impl FnOnce() -> crate::Result<Loaded> + Send + 'static,
        shared: bool,
        cx: &mut Context<Self>,
    ) {
        self.save_with_completion(operation, load, shared, |_| {}, cx);
    }

    pub(super) fn save_with_completion(
        &mut self,
        operation: impl FnOnce() -> crate::Result<()> + Send + 'static,
        load: impl FnOnce() -> crate::Result<Loaded> + Send + 'static,
        shared: bool,
        on_saved: impl FnOnce(&mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.busy() {
            return;
        }
        self.saving = true;
        self.error = None;
        self.status = Some("Saving...".into());
        let retained = cx.entity();
        let (finished, completion) = std::sync::mpsc::sync_channel(1);
        self.save_completion = Some(completion);
        let work = cx.background_executor().spawn(async move {
            let saved = operation().map_err(std::sync::Arc::new);
            // A persistence error can occur after replacement. Always reconcile.
            let loaded = load();
            // Only our successful, reconciled write may advance the queued
            // shared edit's optimistic-concurrency snapshot during shutdown.
            let shared = if saved.is_ok() {
                loaded
                    .as_ref()
                    .ok()
                    .and_then(|loaded| loaded.shared.clone())
            } else {
                None
            };
            let _ = finished.send(saved.clone().map(|()| shared));
            (saved, loaded)
        });
        cx.spawn(async move |_, cx| {
            let (saved, loaded) = work.await;
            retained.update(cx, |this, cx| {
                this.saving = false;
                this.save_completion = None;
                if saved.is_ok() {
                    on_saved(cx);
                }
                if this.quitting {
                    return;
                }
                let reloaded = loaded.is_ok();
                let theme_save = std::mem::take(&mut this.theme_saving);
                let layout_save = std::mem::take(&mut this.layout_saving);
                if saved.is_err() {
                    this.closing = None;
                }
                if theme_save && saved.is_ok() {
                    this.theme_intent = None;
                    themes::clear_theme_draft(cx);
                }
                if layout_save && saved.is_ok() {
                    this.layout_intent = None;
                    layouts::clear_layout_draft(cx);
                }
                this.apply_loaded(loaded, cx);
                if theme_save && saved.is_ok() {
                    this.broadcast_theme(cx);
                }
                if layout_save && saved.is_ok() {
                    this.broadcast_layout(cx);
                }
                this.finish_close(cx);
                if !this.saving {
                    this.status = Some(
                        match (saved.is_ok(), reloaded) {
                            (true, true) => "Saved",
                            (true, false) => "Saved; could not reload current preferences",
                            (false, true) => "Save failed; reloaded current preferences",
                            (false, false) => "Save failed; reload before editing again",
                        }
                        .into(),
                    );
                }
                if let Err(error) = &saved {
                    this.error = Some(format!("Save settings: {error}"));
                }
                let _ = this.source.update(cx, |source, _| {
                    // The source's guarded file watcher owns applying changes.
                    // Direct loads here can overwrite its active picker preview.
                    if shared && saved.is_ok() {
                        // Use the existing connection, without stealing its response lane.
                        if let Some(endpoint) = source.endpoints.iter().find(|endpoint| {
                            !matches!(
                                endpoint.connection.target,
                                herdr_client::ConnectTarget::Ssh { .. }
                            )
                        }) && let (Some(handle), Some(snapshot)) =
                            (&endpoint.connection.handle, &endpoint.live.snapshot)
                            && endpoint.live.status.is_connected()
                        {
                            let status = match handle.request(
                                &snapshot.boot_id,
                                herdr_client::Method::ServerReloadConfig,
                                serde_json::json!({}),
                            ) {
                                Ok(_) => "Saved; daemon reload queued (not acknowledged)".into(),
                                Err(error) => {
                                    format!("Saved; daemon reload not queued: {error}")
                                }
                            };
                            if !this.saving {
                                this.status = Some(status);
                            }
                        }
                    }
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
