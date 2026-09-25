//! The window entity: the state one GPUI window owns for one client of one
//! daemon, and the poll task that folds worker results into it. Behavior is
//! split by responsibility across the submodules below; the fields live here
//! because every one of them describes this window's own presentation state.

mod chrome;
mod clipboard;
mod commands;
mod file_drop;
mod flash;
pub(crate) use flash::Flash;
mod image_source;
mod images;
mod input;
mod lifecycle;
mod mouse;
mod render;
mod selection;
mod toasts;
mod transfers;

#[cfg(test)]
mod font_size_tests;
#[cfg(all(test, feature = "integration-test"))]
mod resize_tests;
#[cfg(test)]
mod tests;

#[cfg(feature = "integration-test")]
use crate::smoke;
use crate::{
    WINDOW_TITLE, avatars, config, endpoint, git, menu,
    navigation::OwnedNavigationTarget,
    preferences,
    presentation::Presentation,
    sidebar,
    state::LiveState,
    terminal::{Selection, WheelAccumulator},
    terminal_painter, updater,
};
use gpui_kit::{prelude::*, *};
use herdr_client::{ConnectOptions, ConnectTarget};
#[cfg(feature = "integration-test")]
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct HerdrWindow {
    pub(crate) sound: crate::sound::Service,
    pub(crate) updater: updater::Updater,
    pub(crate) update_preview: Option<updater::State>,
    pub(crate) config: config::Config,
    /// The terminal size the last loaded config asked for. Increase/decrease
    /// write straight to `config.terminal.size`, so this is what Reset Font
    /// Size restores; a session adjustment never reaches disk.
    pub(crate) configured_terminal_size: f32,
    pub(crate) theme: config::Theme,
    pub(crate) config_load: Option<Task<()>>,
    pub(crate) config_watch: Option<Task<()>>,
    pub(crate) config_load_revision: u64,
    pub(crate) endpoints: Vec<endpoint::Endpoint>,
    pub(crate) selected_endpoint: usize,
    pub(crate) selection_epoch: u64,
    pub(crate) catalog: endpoint::Catalog,
    pub(crate) activation_deadline: Option<std::time::Instant>,
    pub(crate) pending_navigation: Option<OwnedNavigationTarget>,
    pub(crate) pending_toast: Option<u64>,
    pub(crate) toasts_hidden: bool,
    /// The visible notices currently mirrored as kit notifications.
    pub(crate) toast_cards: Vec<toasts::ToastCard>,
    pub(crate) pending_releases: Vec<endpoint::Release>,
    pub(crate) selected_generation: u64,
    pub(crate) live: LiveState,
    pub(crate) focus: FocusHandle,
    pub(crate) options: ConnectOptions,
    pub(crate) last_queued_options: Option<ConnectOptions>,
    pub(crate) pending_resize: Option<(ConnectOptions, std::time::Instant)>,
    pub(crate) active: bool,
    pub(crate) sent_focus: Option<bool>,
    pub(crate) bounds: Bounds<Pixels>,
    /// Last title pushed to the OS, so the window is renamed only when it changes.
    pub(crate) title: String,
    pub(crate) cell_width: f32,
    pub(crate) hovered_terminal_link: bool,
    pub(crate) pressed_terminal_link: Option<(String, Point<Pixels>)>,
    pub(crate) terminal_mouse: Option<mouse::Gesture>,
    pub(crate) scrollbar_drag: Option<mouse::ScrollbarDrag>,
    pub(crate) split_drag: Option<mouse::SplitDrag>,
    /// The resize cursor of the pane border under the pointer, if any.
    pub(crate) split_cursor: Option<CursorStyle>,
    pub(crate) pending_images: Vec<images::PendingImage>,
    pub(crate) file_transfer: Option<transfers::FileTransfer>,
    /// The terminal cells the pointer is choosing. A release copies them and
    /// clears this, so a highlight only ever belongs to a drag in progress.
    pub(crate) selection: Option<Selection>,
    /// The brief message over the terminal, and when it stops showing.
    pub(crate) flash: Option<(Flash, std::time::Instant)>,
    /// The frame on screen, kept across the gap between two projections.
    pub(crate) presentation: Presentation,
    pub(crate) painter: std::rc::Rc<std::cell::RefCell<terminal_painter::TerminalPainter>>,
    pub(crate) marked: String,
    /// The sidebar row the pointer is resting on, waiting to open its menu.
    pub(crate) hover: Option<sidebar::HoverRest>,
    /// The menu that resting opened, which the pointer closes by leaving it.
    pub(crate) hover_menu: Option<sidebar::HoverMenu>,
    pub(crate) local_error: Option<String>,
    pub(crate) menu: menu::MenuState,
    /// A `worktree.remove` queued after its dialog closed.
    pub(crate) removal: Option<menu::Removal>,
    pub(crate) git: git::Git,
    pub(crate) install_warning_shown: bool,
    pub(crate) collapsed_repos: std::collections::HashSet<String>,
    pub(crate) sidebar_visible: bool,
    pub(crate) device_filter: Option<String>,
    pub(crate) wheel: WheelAccumulator,
    pub(crate) sidebar_width: Option<f32>,
    /// The kit panel groups sizing the sidebar and splitting its lists.
    pub(crate) sidebar_panels: sidebar::Panels,
    /// A press on a workspace row that may lift it for reordering.
    pub(crate) workspace_drag: Option<sidebar::WorkspaceDrag>,
    pub(crate) sidebar_split: Option<f32>,
    pub(crate) sidebar_split_modified: bool,
    pub(crate) sidebar_preferences: Option<preferences::Preferences>,
    pub(crate) sidebar_modified: bool,
    pub(crate) agent_sort: preferences::AgentSort,
    /// Keeps a toggle made before the stored chrome arrives from being undone.
    pub(crate) agent_sort_modified: bool,
    pub(crate) avatars: Option<avatars::Avatars>,
    #[cfg(feature = "integration-test")]
    pub(crate) input_probe: smoke::InputProbe,
    /// Spaces and agents lists, in that order.
    pub(crate) sidebar_scroll: [ScrollHandle; 2],
    /// The row each list has scrolled into view, so a new selection is revealed
    /// while the user's own scrolling of an unchanged one is left alone.
    pub(crate) sidebar_revealed: [std::cell::Cell<Option<usize>>; 2],
    pub(crate) _poll: Task<()>,
    pub(crate) _activation: Subscription,
    /// The sidebar as a cached view; see `sidebar::SidebarView`.
    pub(crate) sidebar_view: Entity<sidebar::SidebarView>,
    /// Notified in place of this view by a surface-only update, which redraws
    /// the window while the cached sidebar keeps its layout.
    pub(crate) surface_signal: Entity<SurfaceSignal>,
    pub(crate) _sidebar_invalidation: Subscription,
}

/// See `HerdrWindow::surface_signal`.
pub(crate) struct SurfaceSignal;

impl HerdrWindow {
    /// Redraws for a surface-only update. Notifying this view instead would
    /// also invalidate the cached sidebar, rebuilding every row for a frame
    /// whose rows did not change.
    pub(crate) fn redraw_terminal(&mut self, cx: &mut Context<Self>) {
        self.surface_signal.update(cx, |_, cx| cx.notify());
    }

    /// Every notification of this view reaches the cached sidebar, so it
    /// redraws exactly when it did as part of this view.
    pub(crate) fn invalidate_sidebar(cx: &mut Context<Self>) -> Subscription {
        cx.observe_self(|this, cx| this.sidebar_view.update(cx, |_, cx| cx.notify()))
    }

    /// Runs every display frame while the window draws, so a new surface is
    /// shown on the refresh it arrives for instead of on the next timer tick.
    fn poll_on_frame(this: WeakEntity<Self>, window: &mut Window) {
        window.on_next_frame(move |window, cx| {
            let alive = this.update(cx, |view, cx| {
                if view
                    .endpoints
                    .iter()
                    .any(|endpoint| endpoint.connection.has_update())
                {
                    view.tick(window, cx);
                }
            });
            if alive.is_ok() {
                Self::poll_on_frame(this, window);
            }
        });
    }

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.updater.poll() {
            match self.updater.commit_restart() {
                Ok(true) => {
                    cx.quit();
                    return;
                }
                Ok(false) => {}
                Err(error) => eprintln!("App update restart failed: {error}"),
            }
            cx.notify();
        }
        if self.avatars.as_mut().is_some_and(|avatars| avatars.poll()) {
            cx.notify();
        }
        if let Some(chrome) = self.sidebar_preferences.as_mut().and_then(|p| p.loaded()) {
            if !self.sidebar_modified {
                self.sidebar_width = chrome.sidebar_width;
            }
            if !self.sidebar_split_modified {
                self.sidebar_split = chrome.sidebar_split;
            }
            if !self.agent_sort_modified {
                self.agent_sort = chrome.agent_sort;
            }
            cx.notify();
        }
        let old_pane = self
            .live
            .snapshot
            .as_ref()
            .and_then(|s| s.focused_pane_id.clone());
        self.poll_endpoints(cx);
        self.flush_scrollbar(cx);
        self.flush_split(cx);
        #[cfg(target_os = "macos")]
        crate::app_badge::sync(window.window_handle().window_id(), &self.endpoints, cx);
        self.cancel_stale_image();
        self.poll_file_transfer(cx);
        self.update_workspace_dialog(window, cx);
        self.poll_worktree_source(cx);
        self.poll_hover_menu(std::time::Instant::now(), window, cx);
        if self.tick_flash(std::time::Instant::now()) {
            cx.notify();
        }
        self.poll_tab_rename(window, cx);
        self.poll_pane_rename(window, cx);
        self.sync_menu_overlay(window, cx);
        self.sync_theme_highlight(window, cx);
        if old_pane
            != self
                .live
                .snapshot
                .as_ref()
                .and_then(|s| s.focused_pane_id.clone())
        {
            self.marked.clear();
        }
        self.poll_github(window, cx);
        if self.update_workspace_pr() {
            cx.notify();
        }
        if self.update_git() {
            cx.notify();
        }
        if self.live.missing_installation && !self.install_warning_shown {
            self.install_warning_shown = true;
            self.show_install_modal(window, cx);
        }
        self.resize();
        self.report_focus();
        self.sync_window_title(window);
    }

    pub(crate) fn new(
        target: ConnectTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
        #[cfg(feature = "integration-test")] sidebar_test: bool,
    ) -> Self {
        #[cfg(feature = "integration-test")]
        let target = if sidebar_test {
            ConnectTarget::Socket("/unused-sidebar-fixture.sock".into())
        } else {
            target
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let weak = cx.weak_entity();
        let sidebar_view = cx.new(|_| sidebar::SidebarView::new(weak));
        let timer = cx.background_executor().clone();
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                timer.timer(Duration::from_millis(16)).await;
                if this
                    .update_in(cx, |this, window, cx| this.tick(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let appearance = cx
            .try_global::<crate::app::InitialAppearance>()
            .cloned()
            .unwrap_or_default();
        let crate::app::InitialAppearance {
            config,
            theme,
            error,
        } = appearance;
        let mut this = Self {
            sound: crate::sound::Service::default(),
            updater: updater::Updater::default(),
            update_preview: None,
            configured_terminal_size: config.terminal.size,
            config,
            theme,
            config_load: None,
            config_watch: None,
            config_load_revision: 0,
            catalog: endpoint::Catalog::new(&target),
            endpoints: vec![endpoint::Endpoint::new(
                endpoint::LOCAL.into(),
                "Local".into(),
                target,
                true,
            )],
            selected_endpoint: 0,
            selection_epoch: 0,
            activation_deadline: None,
            pending_navigation: None,
            pending_toast: None,
            toasts_hidden: false,
            toast_cards: Vec::new(),
            pending_releases: Vec::new(),
            selected_generation: 0,
            live: LiveState::default(),
            focus,
            options: ConnectOptions::default(),
            last_queued_options: None,
            pending_resize: None,
            active: window.is_window_active(),
            sent_focus: None,
            bounds: Bounds::default(),
            title: WINDOW_TITLE.to_owned(),
            cell_width: 9.,
            hovered_terminal_link: false,
            pressed_terminal_link: None,
            terminal_mouse: None,
            scrollbar_drag: None,
            split_drag: None,
            split_cursor: None,
            pending_images: Vec::new(),
            file_transfer: None,
            selection: None,
            flash: None,
            presentation: Default::default(),
            painter: Default::default(),
            marked: String::new(),
            hover: None,
            hover_menu: None,
            local_error: error,
            menu: menu::MenuState::new(cx),
            removal: None,
            git: git::Git::default(),
            install_warning_shown: false,
            collapsed_repos: Default::default(),
            sidebar_visible: true,
            device_filter: None,
            wheel: WheelAccumulator::default(),
            sidebar_width: None,
            sidebar_panels: sidebar::Panels::new(cx),
            workspace_drag: None,
            sidebar_split: None,
            sidebar_split_modified: false,
            sidebar_preferences: None,
            sidebar_modified: false,
            agent_sort: preferences::AgentSort::default(),
            agent_sort_modified: false,
            avatars: None,
            #[cfg(feature = "integration-test")]
            input_probe: smoke::InputProbe::default(),
            sidebar_scroll: Default::default(),
            sidebar_revealed: Default::default(),
            _poll: poll,
            sidebar_view,
            surface_signal: cx.new(|_| SurfaceSignal),
            _sidebar_invalidation: Self::invalidate_sidebar(cx),
            _activation: cx.observe_window_activation(window, |this, window, cx| {
                this.active = window.is_window_active();
                if !this.active {
                    this.cancel_terminal_mouse(cx);
                    this.selection = None;
                    this.pressed_terminal_link = None;
                }
                this.report_focus();
                cx.notify();
            }),
        };
        #[cfg(feature = "integration-test")]
        if sidebar_test {
            this._poll = Task::ready(());
            this.live.snapshot = Some(Arc::new(sidebar::layout_tests::snapshot(40)));
            this.endpoints[0].live = this.live.clone();
            if let Ok(mut inbox) = this.endpoints[0].connection.inbox.lock() {
                *inbox = this.live.clone();
            }
            return this;
        }
        Self::poll_on_frame(cx.entity().downgrade(), window);
        this.sidebar_preferences = this.endpoints[0]
            .connection
            .target
            .socket_path()
            .ok()
            .map(|path| preferences::Preferences::new(&path));
        this.avatars = Some(avatars::Avatars::new());
        this.reconnect();
        this.load_gui_config(cx);
        this.watch_gui_config(cx);
        this
    }
}
