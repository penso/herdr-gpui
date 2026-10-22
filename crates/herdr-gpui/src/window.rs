//! The window entity: the state one GPUI window owns for one client of one
//! daemon, and the poll task that folds worker results into it. Behavior is
//! split by responsibility across the submodules below; the fields live here
//! because every one of them describes this window's own presentation state.

mod annotate;
mod announcement;
mod backdrop;
pub(crate) mod chrome_focus;
mod clipboard;
#[cfg(target_os = "macos")]
pub(crate) use clipboard::macos::{
    Error as ClipboardHelperError, run_helper as run_clipboard_helper,
};
mod commands;
mod config_diagnostic;
mod construct;
mod copy_mode;
mod device_themes;
mod double_shift;
mod file_drop;
mod file_links;
#[cfg(test)]
pub(crate) use file_links::opened;
pub(crate) use file_links::{opened_url, start_browser};
mod find;
mod flash;
pub(crate) use commands::run_window_command;
pub(crate) use flash::Flash;
mod function_keys;
mod image_source;
mod images;
mod input;
mod layout_save;
mod lifecycle;
pub(crate) use lifecycle::reasserts;
mod links;
mod monitors;
pub(crate) use links::PressedLink;
mod mouse;
mod pending_input;
mod plugin_selection;
pub(crate) mod prefix;
mod reconnecting;
mod regions;
mod remote_paths;
pub(crate) use remote_paths::RemotePath;
mod render;
mod report_issue;
mod select_hint;
mod selection;
mod server_keys;
mod sound;
mod status_bar;
pub(crate) mod system_notifications;
mod tab_drag;
#[cfg(target_os = "macos")]
mod tab_float;
mod tab_ghost;
mod tab_strip;
mod tab_tear;
mod toasts;
mod transfers;
mod wsl_drop;
mod zen;
#[cfg(test)]
pub(crate) use transfers::tests::Peer as MockPeer;

#[cfg(test)]
mod agent_model_tests;
#[cfg(test)]
mod font_size_tests;
#[cfg(all(test, feature = "integration-test"))]
mod idle_reassert_tests;
#[cfg(test)]
mod key_action_tests;
#[cfg(all(test, feature = "integration-test"))]
mod resize_tests;
#[cfg(test)]
mod shortcut_tests;
#[cfg(test)]
mod status_bar_tests;
#[cfg(test)]
mod tests;

#[cfg(feature = "integration-test")]
use crate::smoke;
use crate::{
    avatars, config, endpoint, git, menu,
    navigation::OwnedNavigationTarget,
    preferences,
    presentation::Presentation,
    sessions, sidebar,
    state::LiveState,
    terminal::{Selection, WheelAccumulator},
    terminal_painter, updater,
};
use gpui::{prelude::*, *};
use herdr_client::ConnectOptions;

pub(crate) use server_keys::ActiveServerKeymap;

pub(crate) struct HerdrWindow {
    // keep-sorted start
    pub(crate) _activation: Subscription,
    pub(crate) _appearance: Subscription,
    pub(crate) _browser_tabs: Subscription,
    pub(crate) _host_theme: Subscription,
    pub(crate) _poll: Task<()>,
    pub(crate) _prefix_interceptor: Subscription,
    pub(crate) _sidebar_invalidation: Subscription,
    /// Clears `chrome_region` when the terminal takes focus back.
    pub(crate) _terminal_focus: Subscription,
    /// Lists again the tabs carried off to a window that closed.
    pub(crate) _window_closed: Subscription,
    /// Another window may edit a note this one shows.
    pub(crate) _worktree_notes: Subscription,
    pub(crate) activation_deadline: Option<std::time::Instant>,
    pub(crate) active: bool,
    /// The activity bar's panel and filter.
    pub(crate) activity: sidebar::Activity,
    pub(crate) agent_sort: preferences::AgentSort,
    /// Whether the user chose the sort with the panel toggle, now or in a
    /// stored session. Until then the daemon's `ui.agent_panel_sort` decides,
    /// including after a config reload, as upstream's manual override does.
    pub(crate) agent_sort_modified: bool,
    /// The App Updates dialog's own notes cache, so scrolling and redraws do not
    /// re-parse the release body.
    pub(crate) app_update_notes: crate::release_notes::Prepared,
    /// Where the App Updates notes are scrolled, read back to draw their bar.
    pub(crate) app_update_notes_scroll: ScrollHandle,
    /// A tab torn off another window, waiting for its host to be listed.
    pub(crate) arrival: Option<tab_tear::TornTab>,
    pub(crate) avatars: Option<avatars::Avatars>,
    /// The picture and platform appearance `[backdrop]` asks for.
    pub(crate) backdrop: backdrop::State,
    pub(crate) bell: crate::bell::Bell,
    pub(crate) bounds: Bounds<Pixels>,
    /// Browser tabs this window shows, and its pages for them.
    pub(crate) browser: crate::browser::Browser,
    pub(crate) catalog: endpoint::Catalog,
    pub(crate) cell_width: f32,
    /// Snapshots of checkouts taken at agent turns, and the dialog listing them.
    pub(crate) checkpoints: crate::checkpoint::Checkpoints,
    /// The chrome region whose controls take Tab; see `chrome_focus`.
    pub(crate) chrome_region: Option<chrome_focus::Region>,
    /// Cloud machines being added; see `cloud::Jobs`.
    #[cfg(feature = "cloud")]
    pub(crate) cloud_jobs: crate::cloud::Jobs,
    /// A new cloud device's setup script whose tab is being opened.
    #[cfg(feature = "cloud")]
    pub(crate) cloud_setup: Option<crate::cloud_setup::Running>,
    /// Checkouts indexed for Go to Symbol and Go to File.
    pub(crate) code_indexes: crate::code_search::Indexes,
    /// Code tabs' views, by tab.
    pub(crate) code_views:
        std::collections::HashMap<crate::browser::TabId, crate::code_view::CodeView>,
    pub(crate) collapsed_repos: std::collections::HashSet<String>,
    pub(crate) config: config::Config,
    pub(crate) config_load: Option<Task<()>>,
    pub(crate) config_load_revision: u64,
    pub(crate) config_watch: Option<Task<()>>,
    /// The terminal size the last loaded config asked for. Increase/decrease
    /// write straight to `config.terminal.size`, so this is what Reset Font
    /// Size restores; a session adjustment never reaches disk.
    pub(crate) configured_terminal_size: f32,
    /// Keyboard copy mode, when it holds the keyboard.
    pub(crate) copy_mode: Option<copy_mode::CopyModeState>,
    /// Prepared daemon announcement and release notes text.
    pub(crate) daemon_text: crate::release_notes::DaemonText,
    /// Starts the default browser on a page with a query or fragment, which
    /// the system opener would drop.
    pub(crate) default_browser: file_links::StartBrowser,
    /// Notes waiting for their agents to be ready for them.
    pub(crate) deliveries: crate::agent_notes::Deliveries,
    /// A press on a device's header that may lift it for reordering.
    pub(crate) device_drag: Option<sidebar::DeviceDrag>,
    pub(crate) device_filter: Option<String>,
    /// The device IDs in the order the user dragged them into; see
    /// `endpoint::order`.
    pub(crate) device_order: Vec<String>,
    /// Each saved device's terminal theme, read off the UI thread.
    pub(crate) device_themes: device_themes::DeviceThemes,
    /// The Devices overview's activity history and open tabs.
    pub(crate) devices_overview: crate::devices_overview::Overview,
    /// A worktree or workspace being created on another host.
    pub(crate) dispatch_job: Option<crate::dispatch::Job>,
    /// Dispatched worktrees' setup scripts, waiting for their hosts in order.
    pub(crate) dispatch_setups: std::collections::VecDeque<crate::dispatch::Setup>,
    /// The editor pane being opened, if any.
    pub(crate) editor_open: Option<crate::editor::Job>,
    /// Editor panes whose Neovim later files open in.
    pub(crate) editor_panes: Vec<crate::editor::EditorPane>,
    pub(crate) endpoints: Vec<endpoint::Endpoint>,
    /// A prompt fanned out to several agents; once launched it outlives its
    /// dialog so the lanes can be compared later.
    pub(crate) fan_out: Option<crate::fan_out::FanOut>,
    pub(crate) file_transfer: Option<transfers::FileTransfer>,
    /// The find bar, over the pane it searches.
    pub(crate) find: Option<find::FindBar>,
    /// How the find bar was left over each pane, to bring it back there.
    pub(crate) find_memory: crate::find::memory::Memory,
    /// The brief message over the terminal, and when it stops showing.
    pub(crate) flash: Option<(Flash, std::time::Instant)>,
    pub(crate) focus: FocusHandle,
    pub(crate) git: git::Git,
    /// Unknown keys in the GUI config, ignored but reported; follows `config`.
    pub(crate) gui_config_diagnostic: crate::config_diagnostic::ConfigDiagnostic,
    pub(crate) held_keys: crate::terminal::HeldKeys,
    /// The sidebar row the pointer is resting on, waiting to open its menu.
    pub(crate) hover: Option<sidebar::HoverRest>,
    /// The menu that resting opened, which the pointer closes by leaving it.
    pub(crate) hover_menu: Option<sidebar::HoverMenu>,
    pub(crate) hovered_terminal_link: bool,
    /// Names the missing icon font once a pane draws an icon it lacks.
    pub(crate) icon_font_notice: crate::icon_font_notice::IconFontNotice,
    #[cfg(feature = "integration-test")]
    pub(crate) input_probe: smoke::InputProbe,
    pub(crate) install_warning_shown: bool,
    pub(crate) integrations: crate::integrations::Integrations,
    /// What a key code types in the active layout, to restore a dropped Shift.
    pub(crate) key_layout: fn(u16) -> Option<prefix::KeyChars>,
    /// The key code and Shift of the key being handled, from AppKit's event.
    pub(crate) key_press: fn() -> Option<(u16, bool)>,
    /// When this window last forwarded user input; gates re-claiming a frame another client sized.
    pub(crate) last_input: Option<std::time::Instant>,
    /// The last pointer position over the panes, so a move event that did not move is not use.
    pub(crate) last_pointer: Option<Point<Pixels>>,
    pub(crate) last_queued_options: Option<ConnectOptions>,
    pub(crate) links: links::DaemonLinks,
    pub(crate) listening_ports: crate::listening_ports::ListeningPorts,
    pub(crate) live: LiveState,
    pub(crate) local_error: Option<String>,
    /// Draws the moving status marks the sidebar and the tab strip reserve
    /// room for; see `mark_layer`.
    pub(crate) mark_layer: Entity<crate::mark_layer::MarkLayer>,
    pub(crate) marked: String,
    /// Where the IME's caret or converted clause is within `marked`, in
    /// UTF-16; `None` puts the caret after it.
    pub(crate) marked_selection: Option<std::ops::Range<usize>>,
    pub(crate) menu: menu::MenuState,
    /// Removing worktrees whose pull request is merged, when enabled.
    pub(crate) merged_cleanup: crate::merged_cleanup::Cleanup,
    /// The focus changes asked of the daemon, for the native GUI harness.
    #[cfg(feature = "integration-test")]
    pub(crate) navigations: smoke::Navigations,
    /// The notes panel beside a review or an annotated page; both share it.
    /// The revision of the app-wide network reading this window last drew.
    pub(crate) network_quality_seen: u64,
    /// The last orchestrator view id handed out.
    pub(crate) next_orchestrator: u64,
    pub(crate) notes_width: crate::panel_resize::PanelWidth,
    /// Which notice cards had lines too long for their row when last laid out.
    pub(crate) notice_overflow: std::rc::Rc<config_diagnostic::Overflow>,
    pub(crate) options: ConnectOptions,
    /// Orchestrator views' requests, acted on at the next tick.
    pub(crate) orchestrator_events: Vec<(crate::orchestrator::ViewId, crate::orchestrator::Event)>,
    pub(crate) orchestrator_live: crate::orchestrator::LiveCache,
    /// Whether an orchestrator's dispatch dialog ranks hosts by their load.
    pub(crate) orchestrator_sampling: bool,
    /// The orchestrator views the sections made, by id.
    pub(crate) orchestrators:
        std::collections::HashMap<crate::orchestrator::ViewId, crate::orchestrator::Orchestrator>,
    /// The activity bar's Overview page.
    pub(crate) overview: crate::overview::Page,
    pub(crate) painter: std::rc::Rc<std::cell::RefCell<terminal_painter::TerminalPainter>>,
    /// A pane moving to another device; a move outlives its menu.
    pub(crate) pane_carry: Option<crate::pane_menu::carry::Carry>,
    pub(crate) pending_images: Vec<images::PendingImage>,
    pub(crate) pending_input: pending_input::PendingInput,
    pub(crate) pending_navigation: Option<OwnedNavigationTarget>,
    pub(crate) pending_releases: Vec<endpoint::Release>,
    pub(crate) pending_resize: Option<(ConnectOptions, std::time::Instant)>,
    pub(crate) pending_toast: Option<u64>,
    /// Remote ports forwarded to this machine; they end with the window.
    pub(crate) port_forwards: crate::port_forward::PortForwards,
    /// Comment, merge, and review reads for the focused branch's open PR.
    pub(crate) pr_actions: crate::pr_actions::Actions,
    /// The daemon's prefix was typed, so the next keystroke completes a chord.
    pub(crate) prefix_armed: bool,
    /// The frame on screen, kept across the gap between two projections.
    pub(crate) presentation: Presentation,
    pub(crate) pressed_terminal_link: Option<PressedLink>,
    /// The terminal grid's cached regions; see `regions`.
    pub(crate) regions: Vec<regions::RegionLayers>,
    /// Pages remote agents asked for, waiting for their port's forward.
    pub(crate) remote_browser: Vec<crate::remote_browser::Forwarding>,
    /// Copies of files on SSH hosts that clicks fetched; see `remote_files`.
    pub(crate) remote_files: crate::remote_files::RemoteFiles,
    /// A `worktree.remove` queued after its dialog closed.
    pub(crate) removal: Option<menu::Removal>,
    /// Herdr's resize mode: direction keys resize the focused pane until
    /// Escape, Enter, or the mode's own shortcut ends it.
    pub(crate) resize_mode: bool,
    /// The review's list of changed files.
    pub(crate) review_files_width: crate::panel_resize::PanelWidth,
    /// Each review tab's state, by its tab.
    pub(crate) reviews: std::collections::HashMap<crate::browser::TabId, crate::review::Review>,
    pub(crate) scrollbar_drag: Option<mouse::ScrollbarDrag>,
    /// The activity bar's Security section: Dependabot alerts.
    pub(crate) security: crate::security::Security,
    /// The Security section's search field and pills.
    pub(crate) security_search: crate::security::SecuritySearch,
    pub(crate) selected_endpoint: usize,
    pub(crate) selected_generation: u64,
    /// The terminal cells the pointer is choosing. A release copies them and
    /// clears this, so a highlight only ever belongs to a drag in progress.
    pub(crate) selection: Option<Selection>,
    pub(crate) selection_epoch: u64,
    pub(crate) selection_follow: selection::Follow,
    pub(crate) sent_focus: Option<bool>,
    /// The selected device's server keymap, when it opted into one.
    pub(crate) server_keys: Option<server_keys::ServerKeymap>,
    /// Local sessions on this machine, scanned on a worker while the popup is open.
    pub(crate) sessions: sessions::Sessions,
    /// Where the footer's sessions button last painted. The window owns it so a
    /// shortcut and a click anchor the list at the same place.
    pub(crate) sessions_anchor: std::rc::Rc<std::cell::Cell<Point<Pixels>>>,
    pub(crate) settings: crate::settings_panel::SettingsPanel,
    pub(super) shift_taps: double_shift::ShiftTaps,
    pub(crate) sidebar_drag: Option<sidebar::SidebarDrag>,
    /// Whether the last spaces list render was folded to its headers for a
    /// lifted device, whose geometry the next frame must not pin from.
    pub(crate) sidebar_folded: std::cell::Cell<bool>,
    pub(crate) sidebar_modified: bool,
    pub(crate) sidebar_nested_reveal: sidebar::NestedReveal,
    /// A spaces row revealed last frame, which the next render moves out from
    /// under the pinned host header if it landed there.
    pub(crate) sidebar_pin_reveal: std::cell::Cell<Option<usize>>,
    pub(crate) sidebar_preferences: Option<preferences::Preferences>,
    /// The row each list has scrolled into view, so a new selection is revealed
    /// while the user's own scrolling of an unchanged one is left alone.
    pub(crate) sidebar_revealed: [std::cell::Cell<Option<usize>>; 2],
    /// Spaces and agents lists, in that order.
    pub(crate) sidebar_scroll: [ScrollHandle; 2],
    pub(crate) sidebar_search: sidebar::SidebarSearch,
    pub(crate) sidebar_split: Option<f32>,
    pub(crate) sidebar_split_modified: bool,
    /// Herdr's `ui.sidebar_start_collapsed` still applies: no shared settings
    /// have loaded yet and the user has not toggled the sidebar since startup.
    pub(crate) sidebar_start_pending: bool,
    /// The sidebar as a cached view; see `sidebar::SidebarView`.
    pub(crate) sidebar_view: Entity<sidebar::SidebarView>,
    /// Expanded; collapsed leaves the rail or nothing, as Herdr's
    /// `ui.sidebar_collapsed_mode` chooses (see `sidebar_mode`).
    pub(crate) sidebar_visible: bool,
    pub(crate) sidebar_width: Option<f32>,
    /// Shared by every window; see `app::shared_sound`.
    pub(crate) sound: std::rc::Rc<crate::sound::Service>,
    /// The resize cursor of the pane border under the pointer, if any.
    pub(crate) split_cursor: Option<CursorStyle>,
    pub(crate) split_drag: Option<mouse::SplitDrag>,
    /// Starts as `config.status_bar.show`; Toggle Status Bar flips it for the
    /// session, and a reload that changes the setting applies it again.
    pub(crate) status_bar_visible: bool,
    /// Whether the leftmost strip gave Back and Forward their full width
    /// when it was last laid out; see `HerdrWindow::strip_leading`.
    pub(crate) strip_navigation_fits: bool,
    /// Notified in place of this view by a surface-only update, which redraws
    /// the window while the cached sidebar keeps its layout.
    pub(crate) surface_signal: Entity<SurfaceSignal>,
    pub(crate) system_load: crate::system_load::SystemLoad,
    /// A press on a tab that may lift it for reordering.
    pub(crate) tab_drag: Option<tab_drag::TabDrag>,
    /// A teleport being set up or under way; a move outlives its dialog.
    pub(crate) teleport: Option<crate::teleport::Teleport>,
    /// The workspace a finished teleport keeps steering to until focused.
    pub(crate) teleport_follow: Option<crate::teleport::Follow>,
    /// Checkouts this client teleported away from, marked in the sidebar.
    pub(crate) teleport_marks: crate::teleport::Marks,
    pub(crate) terminal_mouse: Option<mouse::Gesture>,
    /// Notes on selected terminal text, written or queued.
    pub(crate) terminal_notes: annotate::TerminalNotes,
    pub(crate) theme: config::Theme,
    /// The system appearance `theme` was loaded for, which is what Herdr is
    /// told. It trails the system while the theme for a new appearance
    /// loads, so Herdr never pairs the new appearance with the old colors.
    pub(crate) theme_light: bool,
    /// Last title pushed to the OS, so the window is renamed only when it changes.
    pub(crate) title: String,
    pub(crate) toasts_hidden: bool,
    /// SSH tunnels to remote ports that listen on their host's loopback only.
    pub(crate) tunnels: crate::listening_ports::Tunnels,
    pub(crate) update_preview: Option<updater::State>,
    pub(crate) updater: updater::Updater,
    pub(crate) usage: crate::usage::Usage,
    /// Whether the selected endpoint's refused handshake was announced. Reset
    /// once it has none, so a later refusal is announced again.
    pub(crate) version_notice_shown: bool,
    /// The window's width at its last render, which caps side panels.
    pub(crate) viewport_width: f32,
    /// Tells a sleep from the clocks, so connections are checked on waking.
    pub(crate) wake: endpoint::WakeClock,
    pub(crate) wheel: WheelAccumulator,
    /// A press on a workspace row that may lift it for reordering.
    pub(crate) workspace_drag: Option<sidebar::WorkspaceDrag>,
    /// The worktree script being located, read, trusted, or opened.
    pub(crate) worktree_script: Option<crate::worktree_scripts::Job>,
    /// Files dropped on a WSL pane whose paths are still being mapped; input
    /// typed meanwhile is held so it lands after them.
    pub(crate) wsl_paste: Option<wsl_drop::WslPaste>,
    /// Hides the chrome over the status bar and sidebar flags; see `zen`.
    pub(crate) zen: zen::ZenState,
    // keep-sorted end
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
        cx.observe_self(|this, cx| {
            let indicators = sidebar::Indicators::new(
                this.settings.shared.as_ref(),
                this.config.status_icons,
                this.config.edge_rail,
                matches!(
                    cx.window_appearance(),
                    WindowAppearance::Light | WindowAppearance::VibrantLight
                ),
                &this.theme,
            );
            this.sidebar_view.update(cx, |view, cx| {
                view.set_indicators(indicators, cx);
                cx.notify();
            });
        })
    }

    /// Another window may edit a note: redraw the sidebar, and rebuild an
    /// open palette's rows without waiting for the next tick.
    pub(crate) fn observe_worktree_notes(window: &Window, cx: &mut Context<Self>) -> Subscription {
        cx.observe_global_in::<crate::worktree_notes::Notes>(window, |this, window, cx| {
            this.refresh_palette(window, cx);
            cx.notify();
        })
    }

    /// Theme and appearance changes all notify this view, so each one reaches
    /// the daemon without every place that sets a theme having to report it.
    pub(crate) fn observe_host_theme(cx: &mut Context<Self>) -> Subscription {
        cx.observe_self(|this, _| this.sync_host_theme())
    }

    /// Tell every connection the terminal theme. Only queues, never waits:
    /// each handle skips a theme it already queued.
    pub(crate) fn sync_host_theme(&self) {
        let theme = crate::connection::host_theme(&self.theme, self.theme_light);
        for endpoint in &self.endpoints {
            endpoint.connection.sync_host_theme(&endpoint.live, &theme);
        }
        self.sync_group_host_theme(&theme);
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
                    || view.group_terminals_updated()
                {
                    view.tick(window, cx);
                }
            });
            if alive.is_ok() {
                Self::poll_on_frame(this, window);
            }
        });
    }

    /// Stored chrome yields to anything changed before it arrived. A stored
    /// sort is a past toggle, so it overrides the daemon's from then on.
    pub(crate) fn apply_stored_chrome(&mut self, chrome: preferences::Chrome) {
        let resave = self
            .activity
            .restore(chrome.activity_panel, &chrome.activity_last_opened);
        if !self.sidebar_modified {
            self.sidebar_width = chrome.sidebar_width;
        }
        if !self.sidebar_split_modified {
            self.sidebar_split = chrome.sidebar_split;
        }
        if self.notes_width.chosen().is_none() {
            self.notes_width.restore(chrome.notes_width);
        }
        if self.review_files_width.chosen().is_none() {
            self.review_files_width.restore(chrome.review_files_width);
        }
        // A device dragged before the stored order loaded keeps its place.
        if self.device_order.is_empty() {
            self.device_order = chrome.device_order;
            self.arrange_devices();
        }
        if !self.agent_sort_modified
            && let Some(sort) = chrome.agent_sort
        {
            self.agent_sort = sort;
            self.agent_sort_modified = true;
        }
        // Saved once the whole chrome is merged, so the save keeps it all.
        if resave {
            self.save_chrome();
        }
    }

    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.updater.set_channel(self.config.updates.channel);
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
        if self
            .avatars
            .as_mut()
            .is_some_and(|avatars| avatars.poll(std::time::Instant::now()))
        {
            cx.notify();
        }
        if let Some(chrome) = self.sidebar_preferences.as_mut().and_then(|p| p.loaded()) {
            self.apply_stored_chrome(chrome);
            cx.notify();
        }
        let old_pane = self
            .live
            .snapshot
            .as_ref()
            .and_then(|s| s.focused_pane_id.clone());
        let focused_tab = |live: &LiveState| {
            live.snapshot
                .as_ref()
                .map(|s| (s.focused_workspace_id.clone(), s.focused_tab_id.clone()))
        };
        let old_tab = focused_tab(&self.live);
        self.poll_endpoints(cx);
        if self
            .icon_font_notice
            .observe(self.config.icon_font_missing, self.live.surface.as_deref())
        {
            cx.notify();
        }
        self.post_system_notifications(window, cx);
        self.ring_bell(window);
        self.poll_integrations(cx);
        self.poll_links(window, cx);
        self.poll_remote_files(window, cx);
        if self.settings.task.is_none() {
            let mut reload = false;
            for endpoint in &self.endpoints {
                reload |= endpoint.connection.take_settings_reload();
            }
            if reload {
                self.load_shared_settings(cx);
            }
        }
        // Switching Herdr tabs, from a shortcut, an agent, or the sidebar,
        // moves the group holding the window's connection to the new tab, or
        // brings the terminal back from behind a page. Arriving in another
        // workspace does the same for that workspace's group, so its tab is
        // the one just navigated to.
        if let (Some((old_workspace, old_tab)), Some((workspace, Some(tab)))) =
            (old_tab, focused_tab(&self.live))
            && (old_workspace != workspace || old_tab.as_ref() != Some(&tab))
        {
            self.terminal_focus_moved(&tab, old_workspace != workspace, cx);
        }
        // After the focus check: a swap replaces `live` with another
        // connection's, which is not a move of this one's focus.
        if self.poll_group_terminals() {
            self.redraw_terminal(cx);
        }
        self.reconcile_group_terminals(cx);
        // After polling: a connection that just got its first snapshot, or a
        // reconnect, is told the theme without waiting for it to change.
        self.sync_host_theme();
        self.save_group_layouts(cx);
        self.poll_browser(window, cx);
        self.poll_remote_browser(window, cx);
        self.offer_browser_skill(window, cx);
        self.poll_sessions(cx);
        self.flush_scrollbar(cx);
        self.flush_split(cx);
        self.poll_find(window, cx);
        self.follow_terminal_note(window, cx);
        self.follow_selection(cx);
        self.poll_copy_mode(window, cx);
        #[cfg(windows)]
        crate::taskbar::button::track(window);
        #[cfg(any(target_os = "macos", windows))]
        crate::app_badge::sync(window.window_handle().window_id(), &self.endpoints, cx);
        self.observe_agent_work(window, cx);
        self.cancel_stale_image();
        self.poll_file_transfer(cx);
        self.update_workspace_dialog(window, cx);
        self.poll_teleport(window, cx);
        self.poll_carry(window, cx);
        crate::link::follow::poll(self, window, cx);
        #[cfg(windows)]
        crate::jump_list::JumpList::observe(window.window_handle().window_id(), self, cx);
        #[cfg(windows)]
        crate::taskbar::button::set_progress(
            window.window_handle().window_id(),
            crate::taskbar::button::Bar::from_progress(self.taskbar_progress()),
        );
        self.poll_fan_out(window, cx);
        self.poll_dispatch(window, cx);
        self.poll_device_setup(window, cx);
        self.poll_worktree_script(window, cx);
        #[cfg(feature = "cloud")]
        {
            self.poll_cloud_setup(std::time::Instant::now(), cx);
            self.poll_cloud_setup_tab(cx);
        }
        self.poll_editor_open(cx);
        self.poll_worktree_source(cx);
        self.poll_hover_menu(std::time::Instant::now(), window, cx);
        if self.tick_flash(std::time::Instant::now()) {
            cx.notify();
        }
        self.poll_tab_rename(window, cx);
        self.poll_pane_rename(window, cx);
        self.poll_pane_processes(cx);
        if old_pane
            != self
                .live
                .snapshot
                .as_ref()
                .and_then(|s| s.focused_pane_id.clone())
        {
            self.discard_composition(cx);
        }
        self.poll_github(window, cx);
        if self.update_workspace_pr() {
            cx.notify();
        }
        self.update_merged_cleanup(window, cx);
        if self.update_git() {
            cx.notify();
        }
        if self.update_usage(cx) {
            cx.notify();
        }
        self.poll_devices_overview(cx);
        self.poll_overview(cx);
        self.poll_security(cx);
        if self.update_system_load() {
            cx.notify();
        }
        let network_quality = crate::network_quality::poll(self.config.show_network_quality, cx);
        if network_quality != self.network_quality_seen {
            self.network_quality_seen = network_quality;
            cx.notify();
        }
        self.update_checkpoints(cx);
        self.update_port_forwards(cx);
        if self.update_listening_ports() {
            cx.notify();
        }
        #[cfg(feature = "azure")]
        self.update_cloud_activity();
        if self.live.missing_installation && !self.install_warning_shown {
            self.install_warning_shown = true;
            self.show_install_modal(window, cx);
        }
        self.announce_version_mismatch(window, cx);
        self.resize();
        self.refresh_palette(window, cx);
        self.report_focus();
        self.sync_window_title(window);
    }
}
