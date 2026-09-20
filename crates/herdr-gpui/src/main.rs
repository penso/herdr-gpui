// objc 0.2's selectors expand a legacy cargo-clippy cfg in the native test adapter.
#![cfg_attr(feature = "integration-test", allow(unexpected_cfgs))]
mod agent_mode;
#[cfg(feature = "integration-test")]
mod agent_smoke;
mod agent_view;
mod app_icon;
mod avatars;
mod cli;
mod close_modal;
mod composer;
mod config;
mod connection;
mod controls;
mod daemon;
mod endpoint;
mod input;
mod input_guard;
mod menu;
mod palette;
#[cfg(feature = "integration-test")]
mod performance;
mod preferences;
mod search_input;
mod sidebar;
#[cfg(feature = "integration-test")]
mod smoke;
mod state;
mod terminal;
mod terminal_painter;
mod terminal_view;
mod theme_picker;

use connection::ConnectionBridge;
use controls::Command;
use gpui::{prelude::*, *};
use herdr_client::{ConnectOptions, ConnectTarget, protocol::*};
use state::{ConnectionStatus, LiveState};
#[cfg(feature = "integration-test")]
use std::sync::Arc;
use std::time::Duration;
use terminal::*;

actions!(herdr, [Quit, ShowHerdrNotDetected]);

#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
struct RunCommand {
    command: Command,
}

fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.bind_keys(
        controls::COMMANDS
            .iter()
            .filter(|info| !info.shortcut.is_empty() && info.command != Command::Quit)
            .map(|info| {
                KeyBinding::new(
                    info.shortcut,
                    RunCommand {
                        command: info.command,
                    },
                    None,
                )
            }),
    );
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum NavigationTarget<T> {
    Workspace(T),
    Tab(T),
    Pane(T),
}

type OwnedNavigationTarget = NavigationTarget<String>;

impl<T: AsRef<str>> NavigationTarget<T> {
    fn as_ref(&self) -> NavigationTarget<&str> {
        match self {
            Self::Workspace(id) => NavigationTarget::Workspace(id.as_ref()),
            Self::Tab(id) => NavigationTarget::Tab(id.as_ref()),
            Self::Pane(id) => NavigationTarget::Pane(id.as_ref()),
        }
    }

    fn to_owned(&self) -> OwnedNavigationTarget {
        match self.as_ref() {
            NavigationTarget::Workspace(id) => NavigationTarget::Workspace(id.to_owned()),
            NavigationTarget::Tab(id) => NavigationTarget::Tab(id.to_owned()),
            NavigationTarget::Pane(id) => NavigationTarget::Pane(id.to_owned()),
        }
    }
}

struct HerdrWindow {
    config: config::Config,
    theme: config::Theme,
    endpoints: Vec<endpoint::Endpoint>,
    selected_endpoint: usize,
    selection_epoch: u64,
    catalog: endpoint::Catalog,
    activation_deadline: Option<std::time::Instant>,
    pending_navigation: Option<OwnedNavigationTarget>,
    pending_releases: Vec<endpoint::Release>,
    selected_generation: u64,
    live: LiveState,
    focus: FocusHandle,
    options: ConnectOptions,
    last_queued_options: Option<ConnectOptions>,
    active: bool,
    sent_focus: Option<bool>,
    bounds: Bounds<Pixels>,
    cell_width: f32,
    painter: std::rc::Rc<std::cell::RefCell<terminal_painter::TerminalPainter>>,
    terminal_view: Entity<terminal_view::TerminalView>,
    agent_modes: agent_mode::AgentModes,
    composer: Entity<composer::Composer>,
    composer_target: Option<agent_mode::PaneTarget>,
    drafts: std::collections::HashMap<agent_mode::PaneTarget, composer::Draft>,
    composer_notice: Option<String>,
    navigation_fence: Option<agent_view::NavigationFence>,
    marked: String,
    terminal_input_epoch: u64,
    local_error: Option<String>,
    menu: menu::MenuState,
    install_warning_shown: bool,
    collapsed_repos: std::collections::HashSet<String>,
    sidebar_visible: bool,
    wheel: WheelAccumulator,
    sidebar_width: Option<f32>,
    sidebar_drag: Option<(f32, f32)>,
    sidebar_preferences: Option<preferences::Preferences>,
    sidebar_modified: bool,
    avatars: Option<avatars::Avatars>,
    #[cfg(feature = "integration-test")]
    input_probe: smoke::InputProbe,
    #[cfg(feature = "integration-test")]
    sidebar_scroll: [ScrollHandle; 2],
    _poll: Task<()>,
    _activation: Subscription,
}

impl HerdrWindow {
    fn new(
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
        window.focus(&focus);
        let timer = cx.background_executor().clone();
        let poll = cx.spawn_in(window, async move |this, cx| {
            loop {
                timer.timer(Duration::from_millis(16)).await;
                if this
                    .update_in(cx, |this, window, cx| {
                        if this.avatars.as_mut().is_some_and(|avatars| avatars.poll()) {
                            cx.notify();
                        }
                        if let Some(width) =
                            this.sidebar_preferences.as_mut().and_then(|p| p.loaded())
                            && !this.sidebar_modified
                        {
                            this.sidebar_width = width;
                            cx.notify();
                        }
                        let old_pane = this
                            .live
                            .snapshot
                            .as_ref()
                            .and_then(|s| s.focused_pane_id.clone());
                        this.poll_endpoints(cx);
                        this.sync_composer(cx);
                        this.sync_terminal(cx);
                        if old_pane
                            != this
                                .live
                                .snapshot
                                .as_ref()
                                .and_then(|s| s.focused_pane_id.clone())
                        {
                            this.marked.clear();
                        }
                        if this.live.missing_installation && !this.install_warning_shown {
                            this.install_warning_shown = true;
                            this.show_install_modal(window, cx);
                        }
                        this.resize();
                        this.report_focus();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let painter = Default::default();
        let terminal_view =
            cx.new(|_| terminal_view::TerminalView::new(std::rc::Rc::clone(&painter)));
        let composer = cx.new(composer::Composer::new);
        cx.subscribe(&composer, |this, _, event: &composer::Submit, cx| {
            if this.composer.read(cx).revision() == event.revision {
                this.submit_composer(cx);
            }
        })
        .detach();
        cx.on_blur(&focus, window, |this, _, cx| {
            this.invalidate_terminal_input();
            cx.notify();
        })
        .detach();
        cx.on_blur(&composer.focus_handle(cx), window, |this, _, cx| {
            this.composer
                .update(cx, |editor, cx| editor.invalidate_input_session(cx));
        })
        .detach();
        let fixture = false;
        #[cfg(feature = "integration-test")]
        let fixture = fixture || sidebar_test;
        let loaded = if fixture {
            Ok(config::Config::default())
        } else {
            config::Config::load()
        };
        let (config, theme, config_error) =
            match loaded.and_then(|config| config.theme().map(|theme| (config, theme))) {
                Ok((config, theme)) => (config, theme, None),
                Err(error) => (
                    config::Config::default(),
                    config::Theme::default(),
                    Some(error),
                ),
            };
        let mut this = Self {
            config,
            theme,
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
            pending_releases: Vec::new(),
            selected_generation: 0,
            live: LiveState::default(),
            focus,
            options: ConnectOptions::default(),
            last_queued_options: None,
            active: window.is_window_active(),
            sent_focus: None,
            bounds: Bounds::default(),
            cell_width: 9.,
            painter,
            terminal_view,
            agent_modes: Default::default(),
            composer,
            composer_target: None,
            drafts: Default::default(),
            composer_notice: None,
            navigation_fence: None,
            marked: String::new(),
            terminal_input_epoch: 0,
            local_error: None,
            menu: menu::MenuState::new(cx),
            install_warning_shown: false,
            collapsed_repos: Default::default(),
            sidebar_visible: true,
            wheel: WheelAccumulator::default(),
            sidebar_width: None,
            sidebar_drag: None,
            sidebar_preferences: None,
            sidebar_modified: false,
            avatars: None,
            #[cfg(feature = "integration-test")]
            input_probe: smoke::InputProbe::default(),
            #[cfg(feature = "integration-test")]
            sidebar_scroll: Default::default(),
            _poll: poll,
            _activation: cx.observe_window_activation(window, |this, window, cx| {
                this.active = window.is_window_active();
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
        this.sidebar_preferences = this.endpoints[0]
            .connection
            .target
            .socket_path()
            .ok()
            .map(|path| preferences::Preferences::new(&path));
        this.avatars = Some(avatars::Avatars::new());
        this.reconnect(cx);
        if config_error.is_some() {
            this.local_error = config_error;
        }
        this
    }

    #[cfg(feature = "integration-test")]
    fn set_surface(&mut self, surface: Option<Arc<PaneSurfaceFrame>>, cx: &mut Context<Self>) {
        self.live.surface = surface;
        self.sync_terminal(cx);
    }

    pub(crate) fn sync_terminal(&mut self, cx: &mut Context<Self>) {
        self.terminal_view.update(cx, |view, cx| {
            view.set_appearance(&self.config.terminal, &self.theme, cx);
            view.set_surface(
                self.live
                    .surface
                    .clone()
                    .filter(|_| self.live.surface_ready()),
                cx,
            );
        });
    }

    fn resize(&mut self) {
        if self.last_queued_options == Some(self.options) {
            return;
        }
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) {
            match handle.resize(&snapshot.boot_id, self.options) {
                Ok(()) => self.last_queued_options = Some(self.options),
                Err(error) => self.local_error = Some(format!("Resize: {error}")),
            }
        }
    }

    fn report_focus(&mut self) {
        let focused = self.active && self.endpoints[self.selected_endpoint].surface_requested();
        // Update the authoritative event inbox, not just the rendered clone.
        if let Ok(mut state) = self.endpoints[self.selected_endpoint]
            .connection
            .inbox
            .try_lock()
        {
            state.set_outer_focus(self.active && self.input_ready());
        }
        if self.sent_focus == Some(focused) {
            return;
        }
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) && handle.set_focus(&snapshot.boot_id, focused).is_ok()
        {
            self.sent_focus = Some(focused);
        }
    }

    fn send(&mut self, event: ClientPaneInputEvent, cx: &mut Context<Self>) {
        if self.menu.is_open() || !self.input_ready() {
            return;
        }
        if self.navigation_fence.is_some() {
            self.local_error = Some("Navigation pending; input was not sent.".into());
            cx.notify();
            return;
        }
        if let (Some(handle), Some(snapshot), Some(surface)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
            &self.live.surface,
        ) {
            let endpoint = &self.endpoints[self.selected_endpoint];
            let Some(binding) =
                input::TerminalBinding::new(&endpoint.id, Some(snapshot), Some(surface))
            else {
                return;
            };
            let Ok(inbox) = endpoint.connection.inbox.try_lock() else {
                self.local_error = Some("Herdr state is updating; input was not sent.".into());
                cx.notify();
                return;
            };
            if !inbox.status.is_connected()
                || !inbox.surface_ready()
                || input::TerminalBinding::new(
                    &endpoint.id,
                    inbox.snapshot.as_deref(),
                    inbox.surface.as_deref(),
                ) != Some(binding)
            {
                self.local_error = Some("Terminal target changed; input was not sent.".into());
                cx.notify();
                return;
            }
            let target = if let Some(popup) = &surface.popup {
                InputTarget::Popup(popup.terminal_id.clone())
            } else if let Some(pane) = &snapshot.focused_pane_id {
                InputTarget::Pane(pane.clone())
            } else {
                return;
            };
            if let Err(error) =
                ConnectionBridge::send_input(handle, &snapshot.boot_id, &target, event)
            {
                self.local_error = Some(format!("Input not sent: {error}"));
                cx.notify();
            }
        }
    }

    fn navigate(&mut self, target: NavigationTarget<&str>, cx: &mut Context<Self>) {
        if self.menu.is_open() || !self.input_ready() || self.navigation_fence.is_some() {
            return;
        }
        self.request_focus_change(
            "Navigate",
            Some(target.to_owned()),
            |handle, boot| match target {
                NavigationTarget::Workspace(id) => handle.focus_workspace(boot, id),
                NavigationTarget::Tab(id) => handle.focus_tab(boot, id),
                NavigationTarget::Pane(id) => handle.focus_pane(boot, id),
            },
            cx,
        );
        self.marked.clear();
        self.sync_composer(cx);
        cx.notify();
    }

    fn request_focus_change(
        &mut self,
        method: &str,
        focus: Option<OwnedNavigationTarget>,
        enqueue: impl FnOnce(
            &herdr_client::ClientHandle,
            &str,
        ) -> Result<String, herdr_client::SendError>,
        cx: &mut Context<Self>,
    ) {
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) {
            let Ok(mut state) = self.endpoints[self.selected_endpoint]
                .connection
                .inbox
                .try_lock()
            else {
                self.local_error = Some(format!(
                    "{method}: Herdr state is updating; request was not queued."
                ));
                cx.notify();
                return;
            };
            if !state.status.is_connected()
                || !state.snapshot.as_ref().is_some_and(|latest| {
                    latest.boot_id == snapshot.boot_id && latest.revision == snapshot.revision
                })
            {
                self.local_error = Some(format!(
                    "{method}: Herdr target changed; request was not queued."
                ));
                cx.notify();
                return;
            }
            let request_id = match enqueue(handle, &snapshot.boot_id) {
                Ok(id) => id,
                Err(error) => {
                    self.local_error = Some(format!("{method}: {error}"));
                    cx.notify();
                    return;
                }
            };
            state.track_request(request_id.clone());
            self.navigation_fence = Some(agent_view::NavigationFence::new(
                snapshot,
                request_id,
                focus.clone(),
            ));
            self.local_error = None;
            if self.live.supports_surface {
                // An ordered surface barrier prevents input hitting the previous
                // pane while navigation/creation and its projection are in flight.
                // Hold the inbox lock until both requests and the fence are set.
                let (request, failed) = match handle.set_surface_active(&snapshot.boot_id, true) {
                    Ok(request) => (request, false),
                    Err(error) => {
                        self.local_error = Some(error.to_string());
                        (String::new(), true)
                    }
                };
                state.activation = Some(state::SurfaceActivation {
                    request,
                    boot: snapshot.boot_id.clone(),
                    revision: None,
                    failed,
                    focus,
                    active: true,
                });
                state.surface = None;
                state.dirty = true;
                self.live = state.clone();
                self.activation_deadline = Some(
                    std::time::Instant::now()
                        + if failed {
                            Duration::ZERO
                        } else {
                            Duration::from_secs(5)
                        },
                );
            }
        }
        self.sync_terminal(cx);
        self.sync_composer(cx);
    }

    fn command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_open() {
            return;
        }
        #[cfg(feature = "integration-test")]
        {
            self.input_probe.actions += 1;
        }
        match command {
            Command::ClosePane | Command::CloseTab => {
                self.open_close_confirmation(command, window, cx);
                return;
            }
            Command::Palette | Command::WorkspacePicker => {
                self.open_palette(command == Command::WorkspacePicker, window, cx);
                return;
            }
            Command::Keybinds => {
                self.open_keybinds(window, cx);
                return;
            }
            Command::Themes => {
                self.open_theme_picker(window, cx);
                return;
            }
            Command::Settings => {
                self.open_preferences(window, cx);
                return;
            }
            Command::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            Command::Reconnect => self.reconnect(cx),
            Command::Quit => {
                cx.quit();
                return;
            }
            _ => {}
        }
        if self.activation_deadline.is_some()
            || !self.endpoints[self.selected_endpoint].surface_requested()
        {
            return;
        }
        if let Some(snapshot) = &self.live.snapshot
            && let Some((method, params)) = controls::request(command, snapshot)
        {
            let goal = if method == "tab.focus" {
                params
                    .get("tab_id")
                    .and_then(serde_json::Value::as_str)
                    .map(|id| NavigationTarget::Tab(id.into()))
            } else {
                None
            };
            self.request_focus_change(
                method,
                goal,
                |handle, boot| handle.request(boot, method, params),
                cx,
            );
            self.marked.clear();
        }
        self.sync_composer(cx);
        window.focus(&self.focus);
        cx.notify();
    }

    fn command_action(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if !self.composer.focus_handle(cx).is_focused(window) {
            self.command(command, window, cx);
        }
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_open() || !self.input_ready() || self.navigation_fence.is_some() {
            return;
        }
        let (Some(handle), Some(snapshot), Some(surface)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
            &self.live.surface,
        ) else {
            return;
        };
        let endpoint = &self.endpoints[self.selected_endpoint];
        let Some(binding) =
            input::TerminalBinding::new(&endpoint.id, Some(snapshot), Some(surface))
        else {
            return;
        };
        let Ok(inbox) = endpoint.connection.inbox.try_lock() else {
            self.local_error = Some("Herdr state is updating; wheel input was not sent.".into());
            cx.notify();
            return;
        };
        if !inbox.status.is_connected()
            || !inbox.surface_ready()
            || input::TerminalBinding::new(
                &endpoint.id,
                inbox.snapshot.as_deref(),
                inbox.surface.as_deref(),
            ) != Some(binding)
        {
            self.local_error = Some("Terminal target changed; wheel input was not sent.".into());
            cx.notify();
            return;
        }
        let x = (event.position.x - self.bounds.origin.x).to_f64() as f32;
        let y = (event.position.y - self.bounds.origin.y).to_f64() as f32;
        let cell_height = self.config.terminal.line_height();
        let Some(target) = wheel_target(surface, x, y, self.cell_width, cell_height) else {
            self.wheel = WheelAccumulator::default();
            return;
        };
        let lines = self.wheel.lines(&target.target, event, cell_height);
        cx.stop_propagation();
        if lines == 0 {
            return;
        }
        let input = target.event(lines, event.modifiers);
        let result = ConnectionBridge::send_input(handle, &snapshot.boot_id, &target.target, input);
        if let Err(error) = result {
            self.local_error = Some(format!("Wheel input not sent: {error}"));
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.menu.is_open() {
            return;
        }
        #[cfg(feature = "integration-test")]
        {
            self.input_probe.keys += 1;
        }
        if event.keystroke.modifiers.platform && event.keystroke.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                self.send(ClientPaneInputEvent::Paste(text), cx);
            }
            cx.stop_propagation();
            window.prevent_default();
        } else if self.marked.is_empty()
            && let Some(input) = key_input(event)
        {
            self.send(input, cx);
            cx.stop_propagation();
            window.prevent_default();
        }
    }
}

#[cfg(all(test, feature = "integration-test"))]
mod tests {
    use super::{ConnectTarget, HerdrWindow};

    #[gpui::test]
    fn resize_tracks_cell_metrics_and_retries_failed_options(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            HerdrWindow::new(
                ConnectTarget::Socket("/unused-resize-test.sock".into()),
                window,
                cx,
                true,
            )
        });
        view.update(cx, |view, _| {
            let client = herdr_client::connect(
                view.endpoints[view.selected_endpoint]
                    .connection
                    .target
                    .clone(),
                view.options,
            )
            .unwrap_or_else(|error| panic!("cannot create test client: {error}"));
            client.handle.disconnect();
            view.endpoints[view.selected_endpoint].connection.handle = Some(client.handle);
            let queued = view.options;
            view.last_queued_options = Some(queued);
            view.resize();
            assert!(
                view.local_error.is_none(),
                "identical options are not resent"
            );
            view.options.cell_width_px += 1;
            view.resize();
            assert!(
                view.local_error.is_some(),
                "cell metrics alone trigger a send"
            );
            assert_eq!(view.last_queued_options, Some(queued));
            view.local_error = None;
            view.resize();
            assert!(view.local_error.is_some(), "failed options are retried");
        });
    }
}

impl Render for HerdrWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_composer(cx);
        self.sync_terminal(cx);
        self.composer.update(cx, |editor, cx| {
            editor.set_appearance(&self.config.ui, &self.theme, cx);
        });
        let font = font(self.config.terminal.family.clone());
        let cell_height = self.config.terminal.line_height();
        self.painter.borrow_mut().set_appearance(
            self.config.terminal.size,
            cell_height,
            self.theme.clone(),
        );
        self.cell_width = self.painter.borrow_mut().cell_width(&font, window, cx);
        let sidebar = self.render_sidebar(window, cx);
        let mut tabs = div()
            .id("tabs")
            .flex()
            .flex_none()
            .h(px((self.config.tabs.size * 1.5 + 16.).max(40.)))
            .font_family(self.config.tabs.family.clone())
            .text_size(px(self.config.tabs.size))
            .overflow_x_scroll()
            .bg(rgb(self.theme.surface))
            .text_color(rgb(self.theme.foreground))
            .items_center();
        if let Some(snapshot) = &self.live.snapshot {
            for tab in snapshot
                .tabs
                .iter()
                .filter(|t| Some(&t.workspace_id) == snapshot.focused_workspace_id.as_ref())
            {
                let id = tab.tab_id.clone();
                let menu_target = agent_mode::TabTarget {
                    endpoint_id: self.endpoints[self.selected_endpoint].id.clone(),
                    boot_id: snapshot.boot_id.clone(),
                    tab_id: id.clone(),
                };
                tabs = tabs.child(
                    div()
                        .id(SharedString::from(format!("tab-{id}")))
                        .debug_selector({
                            let id = id.clone();
                            move || format!("tab-{id}")
                        })
                        .px_4()
                        .py_2()
                        .flex_none()
                        .cursor_pointer()
                        .bg(rgb(if tab.focused {
                            self.theme.active
                        } else {
                            self.theme.surface
                        }))
                        .child(tab.label.clone())
                        .when(
                            self.agent_modes.mode(
                                &self.endpoints[self.selected_endpoint].id,
                                &snapshot.boot_id,
                                &id,
                            ) == agent_mode::ViewMode::Agent,
                            |tab| {
                                tab.child(
                                    div()
                                        .ml_2()
                                        .text_xs()
                                        .text_color(rgb(self.theme.muted))
                                        .child("Agent"),
                                )
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                window.prevent_default();
                                this.open_tab_menu(menu_target.clone(), event.position, window, cx);
                            }),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate(NavigationTarget::Tab(&id), cx);
                            window.focus(&this.focus);
                        })),
                );
            }
        }
        let surface = self
            .live
            .surface
            .clone()
            .filter(|_| self.live.surface_ready());
        let snapshot = self.live.snapshot.clone();
        let inbox = self.endpoints[self.selected_endpoint]
            .connection
            .inbox
            .clone();
        let paint_epoch = self.selection_epoch;
        let paint_generation = self.endpoints[self.selected_endpoint].generation;
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let focus = self.focus.clone();
        let cell_width = self.cell_width;
        let input_binding = input::TerminalBinding::new(
            &self.endpoints[self.selected_endpoint].id,
            snapshot.as_deref(),
            surface.as_deref(),
        );
        let input_epoch = self.terminal_input_epoch;
        let mut pixels = AnyView::from(self.terminal_view.clone());
        #[cfg(feature = "integration-test")]
        let retained = std::env::var_os("HERDR_PERF_NO_RETAIN").is_none();
        #[cfg(not(feature = "integration-test"))]
        let retained = true;
        if retained {
            pixels = pixels.cached(StyleRefinement {
                size: SizeRefinement {
                    width: Some(relative(1.).into()),
                    height: Some(relative(1.).into()),
                },
                ..Default::default()
            });
        }
        let terminal = div()
            .id("terminal")
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(rgb(self.theme.background))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    if let Some(surface) = &this.live.surface
                        && surface.popup.is_none()
                    {
                        let col = ((event.position.x - this.bounds.origin.x).to_f64()
                            / this.cell_width as f64)
                            .floor() as u16;
                        let row = ((event.position.y - this.bounds.origin.y).to_f64()
                            / this.config.terminal.line_height() as f64)
                            .floor() as u16;
                        let pane = surface
                            .panes
                            .iter()
                            .find(|p| {
                                col >= p.rect.x
                                    && col < p.rect.x.saturating_add(p.rect.width)
                                    && row >= p.rect.y
                                    && row < p.rect.y.saturating_add(p.rect.height)
                            })
                            .map(|p| p.pane_id.clone());
                        if let Some(id) = pane {
                            this.navigate(NavigationTarget::Pane(&id), cx);
                        }
                    }
                }),
            )
            // Retain pixels only; input, geometry and acknowledgements must stay live.
            .child(pixels)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |this, _| {
                            this.bounds = bounds;
                            this.options = ConnectOptions {
                                surface_size: viewport(
                                    bounds.size.width.to_f64() as f32,
                                    bounds.size.height.to_f64() as f32,
                                    cell_width,
                                    cell_height,
                                ),
                                cell_width_px: cell_width.round().max(1.) as u32,
                                cell_height_px: cell_height.round().max(1.) as u32,
                            };
                            this.resize();
                        });
                    },
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            input::terminal_handler(
                                bounds,
                                paint_entity.clone(),
                                input_binding,
                                input_epoch,
                                paint_epoch,
                                paint_generation,
                            ),
                            cx,
                        );
                        if let Some(surface) = &surface
                            && window.is_window_active()
                            && let Some(snapshot) = &snapshot
                        {
                            let snapshot = snapshot.clone();
                            let surface = surface.clone();
                            // Defer projection/COW work until after paint. On contention,
                            // retry via another draw, never by acknowledging inbox cells.
                            cx.defer(move |cx| {
                                let owned = paint_entity.read(cx).owns_paint(
                                    paint_epoch,
                                    paint_generation,
                                    &inbox,
                                );
                                if !owned {
                                    return;
                                }
                                match inbox.try_lock() {
                                    Ok(mut state) => {
                                        state.acknowledge_presented_surface(
                                            &snapshot, &surface, true,
                                        );
                                    }
                                    Err(std::sync::TryLockError::WouldBlock) => {
                                        paint_entity.update(cx, |_, cx| cx.notify());
                                    }
                                    Err(std::sync::TryLockError::Poisoned(_)) => {}
                                }
                            });
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        let status = self.live.status_text(self.local_error.as_deref());
        div()
            .on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                this.command_action(action.command, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ShowHerdrNotDetected, window, cx| {
                this.show_install_modal(window, cx);
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(self.theme.background))
            .text_color(rgb(self.theme.foreground))
            .font_family(self.config.ui.family.clone())
            .text_size(px(self.config.ui.size))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.sidebar_visible, |row| row.child(sidebar))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .bg(rgb(self.theme.surface))
                                    .text_color(rgb(self.theme.foreground))
                                    .child(tabs.flex_1().min_w_0())
                                    .child(
                                        div()
                                            .id("new-tab")
                                            .px_4()
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(self.theme.active)))
                                            .child("+")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.command(Command::Tab, window, cx)
                                            })),
                                    ),
                            )
                            .child(terminal)
                            .when(self.agent_tab_active(), |column| {
                                column.child(self.render_composer(cx))
                            }),
                    ),
            )
            .child(
                div()
                    .id("connection-status")
                    .debug_selector(|| "connection-status".into())
                    .flex()
                    .flex_none()
                    .h(px((self.config.ui.size * 1.5 + 4.).max(22.)))
                    .overflow_hidden()
                    .items_center()
                    .gap(px(6.))
                    .px_3()
                    .bg(rgb(self.theme.surface))
                    .text_color(rgb(self.theme.foreground))
                    .child(
                        if matches!(self.live.status, ConnectionStatus::StartingDaemon) {
                            div()
                                .size(px(8.))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(self.theme.palette[3]))
                                .with_animation(
                                    "daemon-starting-loader",
                                    Animation::new(Duration::from_secs(1)).repeat(),
                                    |dot, delta| {
                                        dot.opacity(
                                            0.3 + 0.7 * (delta * std::f32::consts::PI).sin(),
                                        )
                                    },
                                )
                                .into_any_element()
                        } else {
                            div()
                                .size(px(6.))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(if self.live.status.is_connected() {
                                    self.theme.palette[2]
                                } else {
                                    self.theme.palette[1]
                                }))
                                .into_any_element()
                        },
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(status),
                    )
                    .when(!self.marked.is_empty(), |d| {
                        d.child(
                            div()
                                .min_w_0()
                                .max_w(px(160.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(format!("Composing: {}", self.marked)),
                        )
                    })
                    .child(
                        div()
                            .id("status-theme")
                            .debug_selector(|| "status-theme".into())
                            .flex_none()
                            .px_2()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .child("Theme")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_theme_picker(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("status-keybinds")
                            .debug_selector(|| "status-keybinds".into())
                            .flex_none()
                            .px_2()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .child("? Keybinds")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_keybinds(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("report-issue")
                            .debug_selector(|| "report-issue".into())
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .px_2()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .child(
                                div()
                                    .size(px(12.))
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .border_1()
                                    .border_color(rgb(self.theme.foreground))
                                    .child(
                                        div()
                                            .size(px(3.))
                                            .rounded_full()
                                            .bg(rgb(self.theme.foreground)),
                                    ),
                            )
                            .child("Report issue")
                            .on_click(|_, _, cx| {
                                cx.open_url(
                                    "https://github.com/penso/herdr-gpui/issues/new/choose",
                                );
                            }),
                    ),
            )
            .when(self.menu.is_open(), |root| {
                root.child(self.render_menu(window, cx))
            })
    }
}

fn main() -> std::process::ExitCode {
    let exit = run();
    if exit != std::process::ExitCode::SUCCESS {
        return exit;
    }
    #[cfg(feature = "integration-test")]
    return std::process::ExitCode::from(
        smoke::EXIT_CODE.load(std::sync::atomic::Ordering::SeqCst),
    );
    #[cfg(not(feature = "integration-test"))]
    std::process::ExitCode::SUCCESS
}

fn run() -> std::process::ExitCode {
    use cli::{LaunchMode, LaunchOptions};
    let LaunchOptions { target, mode } = match LaunchOptions::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!(
                "{error}\nUsage: herdr-gpui [--socket CLIENT_SOCKET | --session NAME [--dev]]"
            );
            return std::process::ExitCode::from(2);
        }
    };
    if mode == LaunchMode::Help {
        println!(
            "herdr-gpui [--socket CLIENT_SOCKET | --session NAME [--dev]]\nConnects to Local and saved SSH hosts; never installs remote software.\nStarts the local Herdr daemon if needed; never stops it.\nExplicit --socket and --dev targets are attach-only; --socket isolates the GUI to one existing daemon."
        );
        #[cfg(feature = "integration-test")]
        println!(
            "  --integration-test  Run native GUI checks (requires explicit --socket)\n  --sidebar-test      Run native sidebar fixtures without connecting to a daemon\n  --agent-test        Run native agent composer fixtures without a daemon\n  --performance-test  Measure native dense-terminal hover/scroll without a daemon (macOS)"
        );
        return std::process::ExitCode::SUCCESS;
    }
    #[cfg(feature = "integration-test")]
    let integration_test = mode == LaunchMode::Integration;
    #[cfg(feature = "integration-test")]
    let sidebar_test = mode == LaunchMode::Sidebar;
    #[cfg(feature = "integration-test")]
    let agent_test = mode == LaunchMode::Agent;
    #[cfg(feature = "integration-test")]
    let performance_test = mode == LaunchMode::Performance;
    #[cfg(feature = "integration-test")]
    if mode != LaunchMode::Normal {
        smoke::EXIT_CODE.store(1, std::sync::atomic::Ordering::SeqCst);
    }
    let startup_failed = std::rc::Rc::new(std::cell::Cell::new(false));
    let failed = startup_failed.clone();
    Application::new().run(move |cx| {
        app_icon::install();
        cx.on_action(|_: &Quit, cx| cx.quit());
        bind_keys(cx);
        cx.set_menus(vec![
            Menu {
                name: "Herdr".into(),
                items: vec![
                    MenuItem::action(
                        "Command Palette",
                        RunCommand {
                            command: Command::Palette,
                        },
                    ),
                    MenuItem::action(
                        "Settings",
                        RunCommand {
                            command: Command::Settings,
                        },
                    ),
                    MenuItem::action(
                        "Keybinds",
                        RunCommand {
                            command: Command::Keybinds,
                        },
                    ),
                    MenuItem::action("Quit Herdr", Quit),
                ],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action(
                        "New Workspace",
                        RunCommand {
                            command: Command::Workspace,
                        },
                    ),
                    MenuItem::action(
                        "New Tab",
                        RunCommand {
                            command: Command::Tab,
                        },
                    ),
                    MenuItem::action(
                        "Switch Workspace",
                        RunCommand {
                            command: Command::WorkspacePicker,
                        },
                    ),
                    MenuItem::separator(),
                    MenuItem::action(
                        "Close Pane...",
                        RunCommand {
                            command: Command::ClosePane,
                        },
                    ),
                    MenuItem::action(
                        "Close Tab...",
                        RunCommand {
                            command: Command::CloseTab,
                        },
                    ),
                ],
            },
            Menu {
                name: "Terminal".into(),
                items: vec![
                    MenuItem::action(
                        "Split Vertically (Right)",
                        RunCommand {
                            command: Command::SplitRight,
                        },
                    ),
                    MenuItem::action(
                        "Split Horizontally (Down)",
                        RunCommand {
                            command: Command::SplitDown,
                        },
                    ),
                    MenuItem::separator(),
                    MenuItem::action(
                        "Next Tab",
                        RunCommand {
                            command: Command::NextTab,
                        },
                    ),
                    MenuItem::action(
                        "Previous Tab",
                        RunCommand {
                            command: Command::PreviousTab,
                        },
                    ),
                    MenuItem::action(
                        "Toggle Pane Zoom",
                        RunCommand {
                            command: Command::Zoom,
                        },
                    ),
                    MenuItem::action(
                        "Toggle Sidebar",
                        RunCommand {
                            command: Command::ToggleSidebar,
                        },
                    ),
                    MenuItem::separator(),
                    MenuItem::action(
                        "Reconnect",
                        RunCommand {
                            command: Command::Reconnect,
                        },
                    ),
                ],
            },
            Menu {
                name: "QA".into(),
                items: vec![MenuItem::action(
                    "Show herdr non-detected modal",
                    ShowHerdrNotDetected,
                )],
            },
        ]);
        cx.on_window_closed(move |cx| {
            if cx.windows().is_empty() {
                #[cfg(feature = "integration-test")]
                if performance_test {
                    std::process::exit(1);
                }
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1200.), px(780.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.), px(400.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Herdr".into()),
                    ..Default::default()
                }),
                app_id: Some("so.pen.herdr-gpui".into()),
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    HerdrWindow::new(
                        target,
                        window,
                        cx,
                        #[cfg(feature = "integration-test")]
                        {
                            sidebar_test || performance_test || agent_test
                        },
                    )
                })
            },
        );
        match opened {
            Ok(_window) => {
                #[cfg(feature = "integration-test")]
                if performance_test {
                    performance::start(_window, cx);
                }
                #[cfg(feature = "integration-test")]
                if integration_test {
                    smoke::start(_window, cx);
                }
                #[cfg(feature = "integration-test")]
                if sidebar_test {
                    smoke::start_sidebar(_window, cx);
                }
                #[cfg(feature = "integration-test")]
                if agent_test {
                    agent_smoke::start(_window, cx);
                }
            }
            Err(error) => {
                eprintln!("Unable to open Herdr window: {error}");
                failed.set(true);
                #[cfg(feature = "integration-test")]
                if performance_test {
                    std::process::exit(1);
                }
                cx.quit();
            }
        }
        cx.activate(true);
    });
    if startup_failed.get() {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
