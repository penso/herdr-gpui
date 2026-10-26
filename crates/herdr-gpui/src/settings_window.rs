//! Independent native preferences window. Disk work never owns a window or a socket.
mod catalog;
#[cfg(feature = "cloud")]
mod cloud_devices;
#[cfg(feature = "cloud")]
// Add Device records the request instead in unit tests.
#[cfg_attr(test, allow(dead_code, unused_imports))]
pub(crate) use cloud_devices::open as open_cloud;
mod controls;
mod inputs;
#[cfg(windows)]
mod launch_at_login;
mod layouts;
mod navigation;
mod pages;
pub(crate) use layouts::{apply_loaded_layout, layout_load_revision};
pub(crate) use pages::remote_control::open as open_remote;
#[cfg(all(feature = "integration-test", target_os = "macos"))]
mod native;
mod persistence;
mod remote_history;
mod scrollbar;
mod search;
mod section;
use section::{Group, Section};
mod sidebar;
#[cfg(test)]
use persistence::SizeIo;
use persistence::{Loaded, SaveCompletion};
use sidebar::SidebarIntent;
pub(crate) use sidebar::{apply_loaded_sidebar, sidebar_load_revision};
mod themes;
mod ui;
#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(crate) use native::verify_native;
pub(crate) use themes::{
    apply_loaded_theme, apply_theme_draft, theme_load_revision, theme_pending,
};

use crate::{
    HerdrWindow,
    config::{Config, Theme, corners, mix},
    fonts::StyledFont,
    herdr_settings,
};
use gpui::{prelude::*, *};

actions!(settings_window, [Close]);

pub(crate) fn key_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new(
            if crate::controls::CTRL_SHORTCUTS {
                "ctrl-shift-w"
            } else {
                "cmd-w"
            },
            Close,
            Some("SettingsWindow"),
        ),
        KeyBinding::new("ctrl-w", Close, Some("SettingsWindow")),
    ]
}

#[derive(Default)]
struct SettingsWindowHandle {
    window: Option<WindowHandle<SettingsWindow>>,
    model: Option<WeakEntity<SettingsWindow>>,
}
impl Global for SettingsWindowHandle {}

pub(crate) fn open(source: WeakEntity<HerdrWindow>, cx: &mut App) {
    cx.defer(move |cx| {
        open_with(source, cx, |view, window, cx| {
            view.initialize_theme_browser(window, cx);
            view.initialize_controls(cx);
            #[cfg(windows)]
            view.refresh_launch_at_login(None, cx);
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
            app_owns_titlebar_drag: cfg!(target_os = "macos"),
            ..Default::default()
        },
        move |window, cx| {
            crate::taskbar::mark(window, cx);
            let model = retained.unwrap_or_else(|| {
                cx.new(|cx| {
                    let mut view = SettingsWindow::new(source.clone(), cx);
                    initialize(&mut view, window, cx);
                    view
                })
            });
            model.update(cx, |view, cx| {
                view.retarget_source(source, cx);
                #[cfg(windows)]
                view.watch_launch_at_login(window, cx);
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
    caption: crate::caption::Caption,
    config: Config,
    /// The config as the last successful load read it, which sidebar saves
    /// diff against; previews change only `config`.
    loaded_config: Config,
    theme: Theme,
    shared: Option<herdr_settings::Settings>,
    source: WeakEntity<HerdrWindow>,
    section: Section,
    /// Catalog indices matching the query in `nav_search`; the body shows
    /// them in place of `section` while it is `Some`.
    search_results: Option<Vec<usize>>,
    nav_search: Entity<crate::search_input::SearchInput>,
    themes: themes::ThemeBrowser,
    controls: controls::Controls,
    plugins: controls::plugins::Plugins,
    code: controls::code::CodeSettings,
    px0: controls::px0::Px0Settings,
    chat_env: controls::chat_env::ChatEnvSettings,
    remote: pages::remote_control::RemoteSettings,
    /// The open override dropdown, if any; its list renders inside its button.
    dropdown: Option<ui::Dropdown>,
    /// The number field being typed into, if any.
    number_edit: Option<ui::NumberEdit>,
    /// A slider being dragged, or released and saving, that writes only on
    /// release.
    number_preview: Option<inputs::NumberPreview>,
    /// The host colour being typed as hex, if any.
    hex_edit: Option<sidebar::HexEdit>,
    #[cfg(windows)]
    launch_at_login: launch_at_login::LaunchAtLogin,
    error: Option<String>,
    status: Option<String>,
    focus: FocusHandle,
    body_scroll: ScrollHandle,
    /// The section list scrolls when the window is too short for it.
    navigation_scroll: ScrollHandle,
    /// Where on a scrollbar's thumb the pointer took hold of it.
    thumb_grab: f32,
    /// The section list's width, dragged by its right edge.
    navigation_width: crate::panel_resize::PanelWidth,
    /// The window's width at its last render, which caps the section list.
    viewport_width: f32,
    loading: bool,
    saving: bool,
    quitting: bool,
    save_completion: Option<std::sync::mpsc::Receiver<SaveCompletion>>,
    _quit: Subscription,
    _search: Subscription,
    new_window_target: Option<herdr_client::ConnectTarget>,
    #[cfg(test)]
    size_io: Option<SizeIo>,
    _source: Option<Subscription>,
    _appearance: Option<Subscription>,
    _watch: Option<Task<()>>,
    /// The config files as our last save left them, for the watcher to accept
    /// so the save's own write does not trigger a second reload.
    saved_sample: Option<persistence::Sample>,
    /// Enumerating system fonts takes hundreds of milliseconds, so each
    /// Settings window does it once rather than on every load.
    installed_fonts: std::sync::Arc<std::sync::OnceLock<Vec<String>>>,
    load_revision: u64,
    theme_revision: u64,
    theme_intent: Option<themes::ThemeIntent>,
    theme_saving: bool,
    layout_intent: Option<crate::config::LayoutMode>,
    layout_saving: bool,
    sidebar_intent: Option<SidebarIntent>,
    /// The draft a running save is writing.
    sidebar_saving: Option<SidebarIntent>,
    /// A committed draft waiting for the running save to finish.
    sidebar_queued: bool,
    #[cfg(test)]
    layout_io: Option<layouts::LayoutIo>,
    remote_history: remote_history::RemoteHistory,
    /// The provider whose tab Cloud devices shows.
    #[cfg(feature = "cloud")]
    cloud_tab: crate::cloud::CloudProvider,
    /// The main window's finished cloud jobs when the cards last read them.
    #[cfg(feature = "cloud")]
    cloud_jobs_seen: u64,
    /// The cloud jobs' revision last drawn, so a job's progress redraws
    /// Cloud devices without every change to the main window doing so.
    #[cfg(feature = "cloud")]
    cloud_jobs_drawn: u64,
    /// The main window's cloud devices when the cards last read them, so a
    /// device removed from the sidebar leaves Settings too.
    #[cfg(feature = "cloud")]
    cloud_devices_seen: u64,
    #[cfg(feature = "azure")]
    azure_card: Option<cloud_devices::AzureCard>,
    #[cfg(feature = "boat")]
    boat_card: Option<cloud_devices::BoatCard>,
    #[cfg(feature = "coder")]
    coder_card: Option<cloud_devices::CoderCard>,
    #[cfg(feature = "container")]
    container_card: Option<cloud_devices::ContainerCard>,
    #[cfg(feature = "daytona")]
    daytona_card: Option<cloud_devices::DaytonaCard>,
    #[cfg(feature = "opensandbox")]
    opensandbox_card: Option<cloud_devices::OpenSandboxCard>,
    #[cfg(feature = "railway")]
    railway_card: Option<cloud_devices::RailwayCard>,
    /// Sandbox images, for the cards of providers that hold them.
    #[cfg(any(feature = "container", feature = "daytona"))]
    sandbox_images: cloud_devices::SandboxImages,
    #[cfg(feature = "sprites")]
    sprites_card: Option<cloud_devices::SpritesCard>,
    theme_loading: bool,
    theme_waiting: bool,
    theme_light: bool,
    theme_load_failed: bool,
    closing: Option<AnyWindowHandle>,
    /// True while the footer's "Save now" writes the pending drafts.
    flushing: bool,
    theme_cache: std::collections::VecDeque<(String, Theme)>,
    _theme_guard: Subscription,
    #[cfg(test)]
    theme_io: Option<themes::ThemeIo>,
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
        let nav_search = cx.new(crate::search_input::SearchInput::new);
        nav_search.update(cx, |input, cx| input.set_placeholder("Search settings", cx));
        let search_changed = cx.subscribe(
            &nav_search,
            |this, input, _: &crate::search_input::Changed, cx| {
                let query = input.read(cx).text().to_owned();
                this.set_search_query(&query, cx);
            },
        );
        Self {
            caption: Default::default(),
            loaded_config: config.clone(),
            config,
            theme,
            shared,
            source,
            section: Section::Theme,
            search_results: None,
            _search: search_changed,
            nav_search,
            themes: themes::ThemeBrowser::new(cx),
            controls: controls::Controls::new(cx),
            plugins: controls::plugins::Plugins::new(cx),
            code: Default::default(),
            px0: Default::default(),
            chat_env: Default::default(),
            remote: Default::default(),
            dropdown: None,
            number_edit: None,
            number_preview: None,
            hex_edit: None,
            #[cfg(windows)]
            launch_at_login: Default::default(),
            error: appearance.error,
            status: None,
            focus: cx.focus_handle(),
            body_scroll: ScrollHandle::new(),
            navigation_scroll: ScrollHandle::new(),
            thumb_grab: 0.,
            navigation_width: crate::panel_resize::SETTINGS_NAVIGATION,
            viewport_width: 0.,
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
            saved_sample: None,
            installed_fonts: Default::default(),
            load_revision: 0,
            theme_revision: 0,
            theme_intent: None,
            theme_saving: false,
            layout_intent: None,
            layout_saving: false,
            sidebar_intent: None,
            sidebar_saving: None,
            sidebar_queued: false,
            #[cfg(test)]
            layout_io: None,
            remote_history: Default::default(),
            #[cfg(feature = "cloud")]
            cloud_tab: cloud_devices::first_tab(),
            #[cfg(feature = "cloud")]
            cloud_jobs_seen: 0,
            #[cfg(feature = "cloud")]
            cloud_jobs_drawn: 0,
            #[cfg(feature = "cloud")]
            cloud_devices_seen: 0,
            #[cfg(feature = "azure")]
            azure_card: None,
            #[cfg(feature = "boat")]
            boat_card: None,
            #[cfg(feature = "coder")]
            coder_card: None,
            #[cfg(feature = "container")]
            container_card: None,
            #[cfg(feature = "daytona")]
            daytona_card: None,
            #[cfg(feature = "opensandbox")]
            opensandbox_card: None,
            #[cfg(feature = "railway")]
            railway_card: None,
            #[cfg(any(feature = "container", feature = "daytona"))]
            sandbox_images: Default::default(),
            #[cfg(feature = "sprites")]
            sprites_card: None,
            theme_loading: false,
            theme_waiting: false,
            theme_light: false,
            theme_load_failed: false,
            closing: None,
            flushing: false,
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
                if this.sidebar_intent.is_some()
                    && cx
                        .try_global::<crate::app::InitialAppearance>()
                        .is_some_and(|appearance| {
                            SidebarIntent::of(&appearance.config) != SidebarIntent::of(&this.config)
                        })
                {
                    this.broadcast_sidebar(cx);
                }
            }),
            #[cfg(test)]
            theme_io: None,
        }
    }

    fn source_changed(&mut self, source: Entity<HerdrWindow>, cx: &mut Context<Self>) {
        #[cfg(feature = "cloud")]
        self.cloud_source_changed(&source, cx);
        #[cfg(not(feature = "cloud"))]
        let _ = source;
        if matches!(self.section, Section::Integrations | Section::Plugins) {
            cx.notify();
        }
        if matches!(self.section, Section::General | Section::Tabs) {
            self.sync_remote_history(false, cx);
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
        self.source = source;
        self.new_window_target = owner
            .read(cx)
            .endpoints
            .first()
            .map(|endpoint| endpoint.connection.target.clone());
        if self.section == Section::Integrations {
            owner.update(cx, |source, cx| source.load_integrations(cx));
        }
        if matches!(self.section, Section::General | Section::Tabs) {
            self.sync_remote_history(false, cx);
        }
        cx.notify();
    }

    fn apply_window_appearance(&mut self, cx: &mut Context<Self>) {
        self.follow_system_appearance(cx);
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
                Ok(theme) => self.theme = theme.styled_for(&self.config),
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
        let menus_changed =
            previous.is_some_and(|previous| crate::menus::changed(&previous.config, &self.config));
        // Future windows inherit an explicit Settings draft, but not a main
        // picker's cancellable hover preview. Logs follow live appearance.
        let previewing = cx.windows().into_iter().any(|window| {
            window.downcast::<HerdrWindow>().is_some_and(|window| {
                window.read(cx).map_or(true, |source| {
                    source.menu.page == Some(crate::menu::Page::Themes)
                        || source.theme_save_in_flight()
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
        } else if menus_changed {
            crate::menus::install(cx);
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.loading || self.saving || self.quitting
    }

    pub(super) fn theme_dirty(&self) -> bool {
        self.theme_intent.is_some()
    }

    /// Starts the writes the footer's "Save now" stands for; completions chain
    /// theme, then layout, then sidebar, through `finish_flush`.
    fn save_pending_drafts(&mut self, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        self.theme_load_failed = false;
        self.flushing = true;
        self.finish_flush(cx);
    }

    fn finish_flush(&mut self, cx: &mut Context<Self>) {
        if !self.flushing {
            return;
        }
        if self.closing.is_some() {
            // Closing writes the same drafts and removes the window itself.
            self.flushing = false;
            return;
        }
        if self.busy() {
            return;
        }
        if self.theme_dirty() {
            self.drive_theme_intent(cx);
        } else if self.layout_intent.is_some() {
            self.save_layout_draft(cx);
        } else if self.sidebar_dirty() {
            self.save_sidebar_draft(cx);
        } else {
            self.flushing = false;
            self.status = None;
            cx.notify();
        }
    }

    /// Shared entry point for the native titlebar and keyboard close actions.
    pub(super) fn should_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.hover_theme(None, cx);
        if self.closing.is_some() {
            return false;
        }
        if !self.finish_number_edit(true, cx) || !self.finish_hex_edit(true, cx) {
            return false;
        }
        if !self.finish_code_edit(true, cx) {
            self.finish_code_edit(false, cx);
        }
        self.dropdown = None;
        if !self.theme_dirty()
            && self.layout_intent.is_none()
            && !self.sidebar_dirty()
            && !self.busy()
        {
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
        self.finish_flush(cx);
        if self.sidebar_queued && self.closing.is_none() && !self.busy() {
            self.save_sidebar_draft(cx);
        }
        if !self.busy() && !self.theme_dirty() && self.closing.is_some() {
            if self.layout_intent.is_some() {
                self.save_layout_draft(cx);
                return;
            }
            if self.sidebar_dirty() {
                self.save_sidebar_draft(cx);
                return;
            }
        }
        if !self.busy()
            && !self.theme_dirty()
            && self.layout_intent.is_none()
            && !self.sidebar_dirty()
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

    pub(super) fn select_section(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.hover_theme(None, cx);
        self.drop_number_preview(cx);
        if !self.finish_number_edit(true, cx) {
            self.finish_number_edit(false, cx);
        }
        if !self.finish_hex_edit(true, cx) {
            self.finish_hex_edit(false, cx);
        }
        if !self.finish_code_edit(true, cx) {
            self.finish_code_edit(false, cx);
        }
        self.dropdown = None;
        self.dismiss_control_font_picker(window, cx);
        self.clear_search(cx);
        self.section = section;
        // A page opened from search or elsewhere shows in a short list too.
        self.navigation_scroll.scroll_to_item(section.nav_item());
        if section == Section::Code {
            self.open_code_page(window, cx);
            self.open_px0_settings(cx);
        }
        if section == Section::General {
            self.open_chat_env_settings(cx);
        }
        if section == Section::RemoteControl {
            self.open_remote_control(cx);
        } else {
            self.close_remote_control();
        }
        self.body_scroll.set_offset(Point::default());
        window.focus(&self.focus, cx);
        if section == Section::Integrations {
            let _ = self
                .source
                .update(cx, |source, cx| source.load_integrations(cx));
        }
        if matches!(section, Section::General | Section::Tabs) {
            self.sync_remote_history(false, cx);
        }
        #[cfg(feature = "cloud")]
        if section == Section::CloudDevices {
            self.open_cloud_devices(cx);
        }
        cx.notify();
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.caption.sync(window, &self.theme);
        let searching = self.search_results.clone();
        let content = match &searching {
            Some(_) => self.render_search_results(cx),
            None => match self.section {
                Section::Theme => self.render_theme(window, cx),
                Section::Integrations => self.render_integration_controls(cx),
                Section::Plugins => self.render_plugin_controls(cx),
                Section::Code => self.render_code_controls(window, cx),
                Section::RemoteControl => self.render_remote_control(cx),
                #[cfg(feature = "cloud")]
                Section::CloudDevices => self.render_cloud_devices(cx),
                _ => self.render_controls(window, cx),
            },
        };
        self.viewport_width = f32::from(window.viewport_size().width);
        let navigation = self.navigation(cx);
        let font_picker = self.render_control_font_picker(window, cx);
        let this = cx.entity().downgrade();
        let header = crate::titlebar::header(&self.theme, window, move |window, cx| {
            let _ = this.update(cx, |view, cx| view.close(window, cx));
        });
        let theme = &self.theme;
        let root = div()
            .key_context("SettingsWindow")
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::settings_key))
            .on_action(cx.listener(|this, action: &crate::RunCommand, window, cx| {
                match action.command {
                    // Its own close path, which saves pending edits first.
                    crate::controls::Command::CloseWindow => this.close(window, cx),
                    command if crate::window::run_window_command(command, window, cx) => {}
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
            .children(header)
            .child(
                div().flex_1().min_h_0().flex().child(navigation).child(
                    self.scroll_area(
                        scrollbar::Pane::Body,
                        div()
                            .id("settings-body")
                            .debug_selector(|| "settings-body".into())
                            .role(Role::TabPanel)
                            .aria_label(if searching.is_some() {
                                "Results"
                            } else {
                                self.section.label()
                            })
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.body_scroll)
                            .p(px(28.))
                            .child(
                                div()
                                    .text_size(px(18.))
                                    .line_height(px(26.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if searching.is_some() {
                                        "Results"
                                    } else {
                                        self.section.label()
                                    }),
                            )
                            .child(div().mb(px(16.)).when_some(
                                self.section.lead().filter(|_| searching.is_none()),
                                |header, lead| {
                                    header.mt(px(2.)).child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(theme.subtext()))
                                            .child(lead),
                                    )
                                },
                            ))
                            .child(content),
                        cx,
                    ),
                ),
            )
            .child(self.footer(cx))
            .children(font_picker);
        let border = self.theme.active;
        crate::titlebar::frame(window, border, root)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
