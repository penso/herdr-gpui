//! Independent native preferences window. Disk work never owns a window or a socket.
mod controls;
mod layouts;
mod plugins;
pub(crate) use layouts::{apply_loaded_layout, layout_load_revision};
#[cfg(all(feature = "integration-test", target_os = "macos"))]
mod native;
mod themes;
#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(crate) use native::verify_native;
pub(crate) use themes::{
    apply_loaded_theme, apply_theme_draft, theme_load_revision, theme_pending,
};

use crate::{
    HerdrWindow,
    config::{Config, FontFace, Theme, corners, mix},
    fonts::StyledFont,
    herdr_settings::{self, Edit},
};
use gpui::{prelude::*, *};

actions!(settings_window, [Close]);

pub(crate) fn key_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("cmd-w", Close, Some("SettingsWindow")),
        KeyBinding::new("ctrl-w", Close, Some("SettingsWindow")),
    ]
}

#[derive(Default)]
struct SettingsWindowHandle {
    window: Option<WindowHandle<SettingsWindow>>,
    model: Option<WeakEntity<SettingsWindow>>,
}
impl Global for SettingsWindowHandle {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Section {
    #[default]
    Appearance,
    Fonts,
    Indicators,
    Sound,
    Notifications,
    Integrations,
    Plugins,
    General,
}

impl Section {
    const ALL: [Self; 8] = [
        Self::Appearance,
        Self::Fonts,
        Self::Indicators,
        Self::Sound,
        Self::Notifications,
        Self::Integrations,
        Self::Plugins,
        Self::General,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Fonts => "Fonts",
            Self::Indicators => "Indicators",
            Self::Sound => "Sound",
            Self::Notifications => "Notifications",
            Self::Integrations => "Integrations",
            Self::Plugins => "Plugins",
            Self::General => "General",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Appearance => "icons/theme.svg",
            Self::Fonts => "icons/pencil.svg",
            Self::Indicators => "icons/pulse.svg",
            Self::Sound => "icons/chart.svg",
            Self::Notifications => "icons/bell.svg",
            Self::Integrations => "icons/agent-generic.svg",
            Self::Plugins => "icons/plug.svg",
            Self::General => "icons/settings.svg",
        }
    }

    /// Sections drawn from the source window's live daemon state.
    fn follows_source(self) -> bool {
        matches!(self, Self::Integrations | Self::Plugins)
    }

    fn description(self) -> &'static str {
        match self {
            Self::Appearance => "A workspace that feels like yours.",
            Self::Fonts => "Make every line comfortable to read.",
            Self::Indicators => "See what your agents are doing at a glance.",
            Self::Sound => "A little signal when something needs you.",
            Self::Notifications => "Stay informed without losing your place.",
            Self::Integrations => "Connect the agents you work with.",
            Self::Plugins => "Run the actions your Herdr plugins provide.",
            Self::General => "The small details of your daily workflow.",
        }
    }
}

pub(crate) fn open(source: WeakEntity<HerdrWindow>, cx: &mut App) {
    cx.defer(move |cx| {
        open_with(source, cx, |view, window, cx| {
            view.initialize_theme_browser(window, cx);
            view.initialize_controls(cx);
            view.reload(cx);
            view.watch_config(cx);
        })
    });
}

fn open_with(
    source: WeakEntity<HerdrWindow>,
    cx: &mut App,
    initialize: impl FnOnce(&mut SettingsWindow, &mut Window, &mut Context<SettingsWindow>) + 'static,
) {
    if let Some(handle) = cx.default_global::<SettingsWindowHandle>().window
        && handle
            .update(cx, |view, window, cx| {
                view.retarget_source(source.clone(), cx);
                window.activate_window();
            })
            .is_ok()
    {
        return;
    }
    // A detached save retains this model after close. Reopening must not create
    // a second editor with a stale baseline while that write is still running.
    let retained = cx
        .default_global::<SettingsWindowHandle>()
        .model
        .as_ref()
        .and_then(WeakEntity::upgrade);
    let bounds = Bounds::centered(None, size(px(960.), px(780.)), cx);
    match cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(680.), px(560.))),
            titlebar: Some(crate::titlebar::options("Settings")),
            ..Default::default()
        },
        move |window, cx| {
            let model = retained.unwrap_or_else(|| {
                cx.new(|cx| {
                    let mut view = SettingsWindow::new(source.clone(), cx);
                    initialize(&mut view, window, cx);
                    view
                })
            });
            model.update(cx, |view, cx| {
                view.retarget_source(source, cx);
                view._appearance = Some(cx.observe_window_appearance(window, |view, _, cx| {
                    view.apply_window_appearance(cx);
                }));
            });
            let weak = model.downgrade();
            window.on_window_should_close(cx, move |window, cx| {
                weak.update(cx, |view, cx| view.should_close(window, cx))
                    .unwrap_or(true)
            });
            let focus = model.read(cx).focus.clone();
            window.focus(&focus, cx);
            cx.default_global::<SettingsWindowHandle>().model = Some(model.downgrade());
            model
        },
    ) {
        Ok(handle) => cx.default_global::<SettingsWindowHandle>().window = Some(handle),
        Err(_) => tracing::error!("Unable to open Settings window"),
    }
}

struct SettingsWindow {
    config: Config,
    theme: Theme,
    shared: Option<herdr_settings::Settings>,
    source: WeakEntity<HerdrWindow>,
    section: Section,
    /// The source daemon reports integration assets that need an update.
    integration_updates: bool,
    themes: themes::ThemeBrowser,
    controls: controls::Controls,
    plugins: plugins::Plugins,
    error: Option<String>,
    status: Option<String>,
    focus: FocusHandle,
    body_scroll: ScrollHandle,
    loading: bool,
    saving: bool,
    quitting: bool,
    save_completion: Option<std::sync::mpsc::Receiver<SaveCompletion>>,
    _quit: Subscription,
    new_window_target: Option<herdr_client::ConnectTarget>,
    #[cfg(test)]
    size_io: Option<SizeIo>,
    _source: Option<Subscription>,
    _appearance: Option<Subscription>,
    _watch: Option<Task<()>>,
    load_revision: u64,
    theme_revision: u64,
    theme_intent: Option<themes::ThemeIntent>,
    theme_saving: bool,
    layout_intent: Option<crate::config::LayoutMode>,
    layout_saving: bool,
    #[cfg(test)]
    layout_io: Option<layouts::LayoutIo>,
    theme_loading: bool,
    theme_waiting: bool,
    theme_light: bool,
    theme_load_failed: bool,
    closing: Option<AnyWindowHandle>,
    theme_cache: std::collections::VecDeque<(String, Theme)>,
    _theme_guard: Subscription,
    #[cfg(test)]
    theme_io: Option<themes::ThemeIo>,
}

struct Loaded {
    config: Config,
    theme: Theme,
    shared: Option<herdr_settings::Settings>,
    error: Option<String>,
}

type SaveCompletion =
    std::result::Result<Option<herdr_settings::Settings>, std::sync::Arc<crate::Error>>;

#[cfg(test)]
type SizeWriter = dyn Fn(Vec<(FontFace, f32)>) -> crate::Result<()> + Send + Sync;

#[cfg(test)]
#[derive(Clone)]
struct SizeIo {
    write: std::sync::Arc<SizeWriter>,
    load: fn() -> crate::Result<Loaded>,
}

impl SettingsWindow {
    fn new(source: WeakEntity<HerdrWindow>, cx: &mut Context<Self>) -> Self {
        let appearance = cx
            .try_global::<crate::app::InitialAppearance>()
            .cloned()
            .unwrap_or_default();
        let (config, theme, shared) =
            source
                .upgrade()
                .map_or((appearance.config, appearance.theme, None), |source| {
                    let source = source.read(cx);
                    (
                        source.config.clone(),
                        source.theme.clone(),
                        source.settings.shared.clone(),
                    )
                });
        let integration_updates = source
            .upgrade()
            .is_some_and(|source| source.read(cx).integration_updates_available());
        let subscription = source
            .upgrade()
            .map(|source| cx.observe(&source, Self::source_changed));
        let new_window_target = source.upgrade().and_then(|source| {
            source
                .read(cx)
                .endpoints
                .first()
                .map(|endpoint| endpoint.connection.target.clone())
        });
        let quit = cx.on_app_quit(|this, cx| {
            let task = this.shutdown(cx);
            async move {
                if task.await.is_err() {
                    tracing::warn!("Unable to finish queued Settings changes during shutdown");
                }
            }
        });
        Self {
            config,
            theme,
            shared,
            source,
            section: Section::Appearance,
            integration_updates,
            themes: themes::ThemeBrowser::new(cx),
            controls: controls::Controls::new(cx),
            plugins: plugins::Plugins::new(cx),
            error: appearance.error,
            status: None,
            focus: cx.focus_handle(),
            body_scroll: ScrollHandle::new(),
            loading: false,
            saving: false,
            quitting: false,
            save_completion: None,
            _quit: quit,
            new_window_target,
            #[cfg(test)]
            size_io: None,
            _source: subscription,
            _appearance: None,
            _watch: None,
            load_revision: 0,
            theme_revision: 0,
            theme_intent: None,
            theme_saving: false,
            layout_intent: None,
            layout_saving: false,
            #[cfg(test)]
            layout_io: None,
            theme_loading: false,
            theme_waiting: false,
            theme_light: false,
            theme_load_failed: false,
            closing: None,
            theme_cache: Default::default(),
            // Shared-theme/OS callbacks in main windows also publish here. Fence
            // their prepared, older palette while an explicit choice is live.
            _theme_guard: cx.observe_global::<crate::app::InitialAppearance>(|this, cx| {
                if this.theme_intent.is_some()
                    && cx
                        .try_global::<crate::app::InitialAppearance>()
                        .is_some_and(|appearance| {
                            appearance.config.theme != this.config.theme
                                || appearance.theme != this.theme
                        })
                {
                    this.drive_theme_intent(cx);
                }
                if this.layout_intent.is_some()
                    && cx
                        .try_global::<crate::app::InitialAppearance>()
                        .is_some_and(|appearance| {
                            appearance.config.layout.mode != this.config.layout.mode
                        })
                {
                    this.broadcast_layout(cx);
                }
            }),
            #[cfg(test)]
            theme_io: None,
        }
    }

    fn source_changed(&mut self, source: Entity<HerdrWindow>, cx: &mut Context<Self>) {
        let updates = source.read(cx).integration_updates_available();
        if self.section.follows_source() || updates != self.integration_updates {
            self.integration_updates = updates;
            cx.notify();
        }
    }

    fn retarget_source(&mut self, source: WeakEntity<HerdrWindow>, cx: &mut Context<Self>) {
        if source.entity_id() == self.source.entity_id()
            || self
                .source
                .upgrade()
                .is_some_and(|source| source.read(cx).integrations.busy)
        {
            return;
        }
        let Some(owner) = source.upgrade() else {
            return;
        };
        self._source = Some(cx.observe(&owner, Self::source_changed));
        self.integration_updates = owner.read(cx).integration_updates_available();
        self.source = source;
        self.new_window_target = owner
            .read(cx)
            .endpoints
            .first()
            .map(|endpoint| endpoint.connection.target.clone());
        if self.section == Section::Integrations {
            owner.update(cx, |source, cx| source.load_integrations(cx));
        }
        cx.notify();
    }

    fn apply_window_appearance(&mut self, cx: &mut Context<Self>) {
        self.sync_appearance(cx);
        self.drive_theme_intent(cx);
        self.publish_appearance(cx);
    }

    fn sync_appearance(&mut self, cx: &mut Context<Self>) {
        if self.config.theme == "Follow Herdr"
            && self.theme_intent.is_none()
            && let Some(shared) = &self.shared
        {
            let light = matches!(
                cx.window_appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            );
            match shared.theme(light) {
                Ok(theme) => self.theme = theme.with_contrast(self.config.contrast),
                Err(error) => self.error = Some(format!("Apply system appearance: {error}")),
            }
        }
        // The theme browser owns its unsaved preview; changing system chrome
        // preserves its selected choice instead of selecting the applied theme.
        self.sync_theme_browser(cx);
        self.sync_controls(cx);
        cx.notify();
    }

    fn publish_appearance(&self, cx: &mut Context<Self>) {
        // An in-flight load/save will publish its validated result. In
        // particular, initial source chrome may still be a picker preview.
        if self.busy() {
            return;
        }
        let previous = cx.try_global::<crate::app::InitialAppearance>();
        let keys_changed =
            previous.is_none_or(|previous| previous.config.keybindings != self.config.keybindings);
        let layout_changed =
            previous.is_some_and(|previous| previous.config.layout.mode != self.config.layout.mode);
        // Future windows inherit an explicit Settings draft, but not a main
        // picker's cancellable hover preview. Logs follow live appearance.
        let previewing = cx.windows().into_iter().any(|window| {
            window.downcast::<HerdrWindow>().is_some_and(|window| {
                window.read(cx).map_or(true, |source| {
                    matches!(
                        source.menu.page,
                        Some(crate::menu::Page::Themes | crate::menu::Page::Fonts)
                    ) || source.theme_save_in_flight()
                        || source.native_settings_save_in_flight()
                        || source.font_size_saves.is_busy()
                })
            })
        });
        let mut appearance = crate::app::InitialAppearance {
            config: self.config.clone(),
            theme: self.theme.clone(),
            error: None,
        };
        apply_theme_draft(&mut appearance.config, &mut appearance.theme, cx);
        cx.set_global(appearance);
        if !previewing {
            crate::log_window::set_appearance(&self.config, &self.theme, cx);
        }
        if keys_changed {
            crate::actions::rebind_keys(cx);
        } else if layout_changed {
            crate::menus::install(cx);
        }
    }

    fn watch_config(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn busy(&self) -> bool {
        self.loading || self.saving || self.quitting
    }

    pub(super) fn theme_dirty(&self) -> bool {
        self.theme_intent.is_some()
    }

    /// Shared entry point for the native titlebar and keyboard close actions.
    pub(super) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.closing.is_some() {
            return false;
        }
        if !self.finish_control_size_edit(true, cx) {
            return false;
        }
        if !self.theme_dirty() && self.layout_intent.is_none() && !self.busy() {
            return true;
        }
        self.closing = Some(window.window_handle());
        self.status = Some("Finishing preferences before closing...".into());
        self.theme_load_failed = false;
        self.drive_theme_intent(cx);
        false
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.should_close(window, cx) {
            window.remove_window();
        }
    }

    fn finish_close(&mut self, cx: &mut Context<Self>) {
        if !self.busy()
            && !self.theme_dirty()
            && self.closing.is_some()
            && self.layout_intent.is_some()
        {
            self.save_layout_draft(cx);
            return;
        }
        if !self.busy()
            && !self.theme_dirty()
            && self.layout_intent.is_none()
            && let Some(handle) = self.closing.take()
        {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            });
        }
    }

    fn additional_window_target(&self, cx: &App) -> Option<herdr_client::ConnectTarget> {
        self.source
            .upgrade()
            .and_then(|source| {
                source
                    .read(cx)
                    .endpoints
                    .first()
                    .map(|endpoint| endpoint.connection.target.clone())
            })
            .or_else(|| self.new_window_target.clone())
    }

    fn shutdown(&mut self, cx: &mut Context<Self>) -> Task<crate::Result<()>> {
        #[cfg(test)]
        if let Some(io) = self.size_io.clone() {
            return self.shutdown_with(move |sizes| (io.write)(sizes), cx);
        }
        self.shutdown_with(|sizes| Config::save_font_sizes(&sizes), cx)
    }

    fn shutdown_with(
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

    pub(super) fn select_section(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.finish_control_size_edit(true, cx) {
            self.finish_control_size_edit(false, cx);
        }
        self.dismiss_control_font_picker(window, cx);
        self.section = section;
        self.body_scroll.set_offset(Point::default());
        window.focus(&self.focus, cx);
        if section == Section::Integrations {
            let _ = self
                .source
                .update(cx, |source, cx| source.load_integrations(cx));
        }
        cx.notify();
    }

    fn loader(cx: &App) -> impl FnOnce() -> crate::Result<Loaded> + Send + 'static {
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

    fn apply_loaded(&mut self, loaded: crate::Result<Loaded>, cx: &mut Context<Self>) {
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

    fn reload_with(
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

    fn save_with(
        &mut self,
        operation: impl FnOnce() -> crate::Result<()> + Send + 'static,
        load: impl FnOnce() -> crate::Result<Loaded> + Send + 'static,
        shared: bool,
        cx: &mut Context<Self>,
    ) {
        self.save_with_completion(operation, load, shared, |_| {}, cx);
    }

    fn save_with_completion(
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

    fn navigation(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        div()
            .w(px(184.))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .p(px(12.))
            .gap(px(4.))
            .bg(rgb(mix(theme.surface, theme.background, 25)))
            .border_r_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .px(px(10.))
                    .py(px(20.))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Herdr"),
                    )
                    .child(
                        div()
                            .mt(px(3.))
                            .text_color(rgb(theme.subtext()))
                            .child("Settings"),
                    ),
            )
            .children(
                Section::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, section)| {
                        let selected = self.section == section;
                        div()
                            .id(("settings-section", index))
                            .relative()
                            .map(|row| {
                                #[cfg(all(feature = "integration-test", target_os = "macos"))]
                                let row = row.child(native::probe(index));
                                row
                            })
                            .debug_selector(move || format!("settings-section-{index}"))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(10.))
                            .py(px(11.))
                            .rounded(px(corners::CONTROL))
                            .cursor_pointer()
                            .when(selected, |el| el.bg(rgb(theme.primary_wash())))
                            .hover(|el| el.bg(rgb(theme.active)))
                            .child(
                                svg()
                                    .path(section.icon())
                                    .size(px(17.))
                                    .flex_none()
                                    .text_color(rgb(if selected {
                                        theme.primary()
                                    } else {
                                        theme.subtext()
                                    })),
                            )
                            .child(section.label())
                            .when(
                                section == Section::Integrations && self.integration_updates,
                                |row| row.child(update_badge(theme)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_section(section, window, cx)
                            }))
                    }),
            )
    }
}

/// Marks the Integrations section while the daemon reports outdated assets.
fn update_badge(theme: &Theme) -> Div {
    div()
        .debug_selector(|| "settings-integrations-badge".into())
        .ml_auto()
        .flex_none()
        .px(px(6.))
        .rounded_full()
        .bg(rgb(theme.primary()))
        .text_color(rgb(theme.background))
        .text_size(px(10.))
        .line_height(px(16.))
        .child("Update")
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.section {
            Section::Appearance => self.render_appearance(window, cx),
            Section::Integrations => self.render_integration_controls(cx),
            Section::Plugins => self.render_plugin_controls(cx),
            _ => self.render_controls(window, cx),
        };
        let navigation = self.navigation(cx);
        let font_picker = self.render_control_font_picker(window, cx);
        let theme = &self.theme;
        div()
            .key_context("SettingsWindow")
            .relative()
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, action: &crate::RunCommand, window, cx| {
                match action.command {
                    crate::controls::Command::Settings => window.activate_window(),
                    crate::controls::Command::NewWindow => {
                        if let Some(target) = this.additional_window_target(cx) {
                            crate::app::open_additional_window(target, cx);
                        }
                    }
                    _ => {}
                }
                // Session commands never cross from this independent root to
                // the integration source, even when it is still alive.
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                cx.stop_propagation();
                this.close(window, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_font(&self.config.ui)
            .text_size(px(12.))
            .line_height(px(18.))
            .bg(rgb(theme.background))
            .text_color(rgb(theme.foreground))
            .map(|root| {
                #[cfg(target_os = "macos")]
                let root = root.child(crate::titlebar::render(theme.surface, None));
                root
            })
            .child(
                div().flex_1().min_h_0().flex().child(navigation).child(
                    div()
                        .id("settings-body")
                        .debug_selector(|| "settings-body".into())
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.body_scroll)
                        .p(px(28.))
                        .child(
                            div()
                                .text_size(px(28.))
                                .line_height(px(36.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.section.label()),
                        )
                        .child(
                            div()
                                .mt(px(5.))
                                .mb(px(24.))
                                .text_color(rgb(theme.subtext()))
                                .child(self.section.description()),
                        )
                        .child(content),
                ),
            )
            .child(
                div()
                    .flex_none()
                    .min_h(px(36.))
                    .px(px(16.))
                    .py(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_size(px(11.))
                    .child(
                        div().debug_selector(|| "settings-footer-status".into()).flex_1().min_w_0().child(
                            self.error
                                .clone()
                                .or_else(|| {
                                    if self.loading {
                                        Some("Loading preferences...".into())
                                    } else {
                                        self.status.clone()
                                    }
                                })
                                .unwrap_or_else(|| {
                                    if self.section == Section::Appearance {
                                        "Themes and layouts change live; saved on Settings close or app quit."
                                    } else {
                                        "Changes are saved automatically."
                                    }
                                    .into()
                                }),
                        ),
                    )
                    .child(
                        self.control_choice("settings-footer-reload", "Reload", false, !self.busy())
                            .debug_selector(|| "settings-footer-reload".into())
                            .flex_none()
                            .px(px(10.))
                            .py(px(3.))
                            .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                    ),
            )
            .children(font_picker)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
