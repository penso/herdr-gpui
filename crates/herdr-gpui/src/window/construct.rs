//! Building a window: its effects run in a fixed order before the fields,
//! which stay sorted so parallel additions land on different lines.

use super::{HerdrWindow, SurfaceSignal, function_keys, prefix};
#[cfg(feature = "integration-test")]
use crate::smoke;
use crate::{
    WINDOW_TITLE, avatars, endpoint, git, log_window, menu, preferences, sessions, sidebar,
    state::LiveState,
    terminal::{Selection, WheelAccumulator},
    updater,
};
use gpui::{prelude::*, *};
use herdr_client::{ConnectOptions, ConnectTarget};
#[cfg(feature = "integration-test")]
use std::sync::Arc;
use std::time::Duration;

impl HerdrWindow {
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
        // Created by the first window; each later one shares its PR answers.
        cx.default_global::<crate::pull_request::Shared>();
        let weak = cx.weak_entity();
        let (initial_theme, initial_icons, initial_rail) = cx
            .try_global::<crate::app::InitialAppearance>()
            .map(|appearance| {
                (
                    appearance.theme.clone(),
                    appearance.config.status_icons,
                    appearance.config.edge_rail,
                )
            })
            .unwrap_or_default();
        let indicators = sidebar::Indicators::new(
            None,
            initial_icons,
            initial_rail,
            matches!(
                cx.window_appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            ),
            &initial_theme,
        );
        let sidebar_view = cx.new(|_| sidebar::SidebarView::new(weak, indicators));
        let hosts = vec![cx.entity_id(), sidebar_view.entity_id()];
        let mark_layer = cx.new(|cx| crate::mark_layer::MarkLayer::new(window, hosts, cx));
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
        // Effects run here in a fixed order, so the fields below can stay sorted.
        let configured_terminal_size = config.terminal.size;
        let status_bar_visible = config.status_bar.show;
        let mut gui_config_diagnostic = crate::config_diagnostic::ConfigDiagnostic::default();
        gui_config_diagnostic.sync(config.diagnostic().as_deref());
        let catalog = endpoint::Catalog::new(&target);
        let menu = menu::MenuState::new(cx);
        // Starts the thread that reads and saves the marks file.
        let teleport_marks = crate::teleport::Marks::start();
        let sidebar_search = sidebar::SidebarSearch::new(cx);
        let surface_signal = cx.new(|_| SurfaceSignal);
        let sidebar_invalidation = Self::invalidate_sidebar(cx);
        let host_theme = Self::observe_host_theme(cx);
        let browser = crate::browser::Browser::new(cx);
        // Another window, or an agent, may open or close a tab.
        let browser_tabs = cx.observe_global::<crate::browser::Store>(|_, cx| cx.notify());
        let worktree_notes = Self::observe_worktree_notes(window, cx);
        let prefix_interceptor = Self::intercept_prefix(window, cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.active = window.is_window_active();
            if this.active {
                this.publish_server_keymap(cx);
            } else {
                this.shift_taps.cancel();
                this.disarm_prefix();
                this.cancel_terminal_mouse(cx);
                // A drag cut short is dropped; a selection kept for an
                // explicit copy outlives a trip to another window.
                if this.selection.as_ref().is_some_and(Selection::dragging) {
                    this.selection = None;
                }
                this.pressed_terminal_link = None;
            }
            this.report_focus();
            cx.notify();
        });
        let appearance_change = cx.observe_window_appearance(window, |this, _, cx| {
            this.apply_shared_theme(cx);
            this.apply_system_theme(cx);
            cx.notify();
        });
        let window_closed = Self::give_back_torn_tabs(cx);
        let mut this = Self {
            // keep-sorted start
            _activation: activation,
            _appearance: appearance_change,
            _browser_tabs: browser_tabs,
            _host_theme: host_theme,
            _poll: poll,
            _prefix_interceptor: prefix_interceptor,
            _sidebar_invalidation: sidebar_invalidation,
            _window_closed: window_closed,
            _worktree_notes: worktree_notes,
            activation_deadline: None,
            active: window.is_window_active(),
            activity: Default::default(),
            agent_sort: preferences::AgentSort::default(),
            agent_sort_modified: false,
            app_update_notes: Default::default(),
            app_update_notes_scroll: ScrollHandle::new(),
            arrival: None,
            avatars: None,
            backdrop: Default::default(),
            bell: crate::bell::Bell::default(),
            bounds: Bounds::default(),
            browser,
            catalog,
            cell_width: 9.,
            checkpoints: Default::default(),
            #[cfg(feature = "cloud")]
            cloud_jobs: Default::default(),
            #[cfg(feature = "cloud")]
            cloud_setup: None,
            code_indexes: Default::default(),
            code_views: Default::default(),
            collapsed_repos: Default::default(),
            config,
            config_load: None,
            config_load_revision: 0,
            config_watch: None,
            configured_terminal_size,
            copy_mode: None,
            daemon_text: Default::default(),
            deliveries: Default::default(),
            device_drag: None,
            device_filter: None,
            device_order: Vec::new(),
            device_themes: Default::default(),
            devices_overview: Default::default(),
            dispatch_job: None,
            dispatch_setups: Default::default(),
            editor_open: None,
            editor_panes: Vec::new(),
            endpoints: vec![endpoint::Endpoint::new(
                endpoint::LOCAL.into(),
                "Local".into(),
                target,
                true,
            )],
            fan_out: None,
            file_transfer: None,
            find: None,
            find_memory: Default::default(),
            flash: None,
            focus,
            git: git::Git::default(),
            gui_config_diagnostic,
            held_keys: Default::default(),
            hover: None,
            hover_menu: None,
            hovered_terminal_link: false,
            icon_font_notice: Default::default(),
            #[cfg(feature = "integration-test")]
            input_probe: smoke::InputProbe::default(),
            install_warning_shown: false,
            integrations: Default::default(),
            key_layout: prefix::current_layout,
            key_press: function_keys::current_press,
            last_input: None,
            last_pointer: None,
            last_queued_options: None,
            links: Default::default(),
            listening_ports: Default::default(),
            live: LiveState::default(),
            local_error: error,
            mark_layer,
            marked: String::new(),
            marked_selection: None,
            menu,
            merged_cleanup: Default::default(),
            network_quality_seen: 0,
            next_orchestrator: 0,
            notes_width: crate::panel_resize::NOTES,
            options: ConnectOptions::default(),
            orchestrator_events: Vec::new(),
            orchestrator_live: Default::default(),
            orchestrator_sampling: false,
            orchestrators: Default::default(),
            overview: Default::default(),
            painter: Default::default(),
            pane_carry: None,
            pending_images: Vec::new(),
            pending_input: Default::default(),
            pending_navigation: None,
            pending_releases: Vec::new(),
            pending_resize: None,
            pending_toast: None,
            port_forwards: Default::default(),
            pr_actions: Default::default(),
            prefix_armed: false,
            presentation: Default::default(),
            pressed_terminal_link: None,
            regions: Vec::new(),
            remote_browser: Vec::new(),
            removal: None,
            resize_mode: false,
            review_files_width: crate::panel_resize::REVIEW_FILES,
            reviews: Default::default(),
            scrollbar_drag: None,
            selected_endpoint: 0,
            selected_generation: 0,
            selection: None,
            selection_epoch: 0,
            selection_follow: Default::default(),
            sent_focus: None,
            server_keys: None,
            sessions: sessions::Sessions::default(),
            sessions_anchor: Default::default(),
            settings: Default::default(),
            shift_taps: Default::default(),
            sidebar_drag: None,
            sidebar_folded: Default::default(),
            sidebar_modified: false,
            sidebar_nested_reveal: Default::default(),
            sidebar_pin_reveal: Default::default(),
            sidebar_preferences: None,
            sidebar_revealed: Default::default(),
            sidebar_scroll: Default::default(),
            sidebar_search,
            sidebar_split: None,
            sidebar_split_modified: false,
            sidebar_start_pending: true,
            sidebar_view,
            sidebar_visible: true,
            sidebar_width: None,
            sound: Default::default(),
            split_cursor: None,
            split_drag: None,
            status_bar_visible,
            strip_navigation_fits: true,
            surface_signal,
            system_load: Default::default(),
            tab_drag: None,
            teleport: None,
            teleport_follow: None,
            teleport_marks,
            terminal_mouse: None,
            terminal_notes: Default::default(),
            theme,
            theme_light: crate::app::light_appearance(cx),
            title: WINDOW_TITLE.to_owned(),
            toasts_hidden: false,
            tunnels: Default::default(),
            update_preview: None,
            updater: updater::Updater::default(),
            usage: Default::default(),
            version_notice_shown: false,
            viewport_width: 0.,
            wake: endpoint::WakeClock::new(std::time::Instant::now(), std::time::SystemTime::now()),
            wheel: WheelAccumulator::default(),
            workspace_drag: None,
            worktree_script: None,
            wsl_paste: None,
            zen: Default::default(),
            // keep-sorted end
        };
        // Quitting need not drop this window, so its SSH children (port
        // forwards and listening-port tunnels) are killed here rather than
        // left forwarding after the app is gone.
        cx.on_app_quit(|this, _| {
            this.port_forwards.stop_all();
            this.tunnels.close_all();
            async {}
        })
        .detach();
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
        this.reconnect(cx);
        log_window::set_appearance(&this.config, &this.theme, cx);
        this.load_gui_config(cx);
        this.load_shared_settings(cx);
        this.watch_gui_config(cx);
        this
    }
}
