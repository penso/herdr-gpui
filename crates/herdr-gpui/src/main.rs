// objc 0.2's selectors expand a legacy cargo-clippy cfg in the native test adapter.
#![cfg_attr(feature = "integration-test", allow(unexpected_cfgs))]
mod app_icon;
mod cli;
mod connection;
mod controls;
mod input;
mod menu;
#[cfg(feature = "integration-test")]
mod performance;
mod sidebar;
#[cfg(feature = "integration-test")]
mod smoke;
mod state;
mod terminal;
mod terminal_painter;
mod terminal_view;

use connection::ConnectionBridge;
use controls::Command;
use gpui::{prelude::*, *};
use herdr_client::{ConnectOptions, ConnectTarget, protocol::*};
use state::LiveState;
use std::sync::Arc;
use std::time::Duration;
use terminal::*;

actions!(
    herdr,
    [
        Quit,
        Reconnect,
        NewWorkspace,
        NewTab,
        SplitRight,
        SplitDown,
        NextTab,
        PreviousTab
    ]
);

enum NavigationTarget<'a> {
    Workspace(&'a str),
    Tab(&'a str),
    Pane(&'a str),
}

struct HerdrWindow {
    connection: ConnectionBridge,
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
    marked: String,
    local_error: Option<String>,
    menu: menu::MenuState,
    collapsed_repos: std::collections::HashSet<String>,
    wheel: WheelAccumulator,
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
        let focus = cx.focus_handle();
        window.focus(&focus);
        let timer = cx.background_executor().clone();
        let poll = cx.spawn(async move |this, cx| {
            loop {
                timer.timer(Duration::from_millis(16)).await;
                if this
                    .update(cx, |this, cx| {
                        let next = this.connection.take_update();
                        if let Some(next) = next {
                            if !next.status.is_connected() {
                                this.local_error = None;
                            }
                            if this.live.snapshot.as_ref().map(|s| &s.focused_pane_id)
                                != next.snapshot.as_ref().map(|s| &s.focused_pane_id)
                            {
                                this.marked.clear();
                            }
                            this.set_surface(next.surface.clone(), cx);
                            this.live = next;
                            cx.notify();
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
        let mut this = Self {
            connection: ConnectionBridge::new(target),
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
            marked: String::new(),
            local_error: None,
            menu: menu::MenuState::new(cx),
            collapsed_repos: Default::default(),
            wheel: WheelAccumulator::default(),
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
            return this;
        }
        this.reconnect(cx);
        this
    }

    fn set_surface(&mut self, surface: Option<Arc<PaneSurfaceFrame>>, cx: &mut Context<Self>) {
        // Invalidate before drawing, not during the parent's render/layout phase.
        self.terminal_view
            .update(cx, |view, cx| view.set_surface(surface.clone(), cx));
        self.live.surface = surface;
    }

    fn reconnect(&mut self, cx: &mut Context<Self>) {
        self.local_error = None;
        self.marked.clear();
        self.last_queued_options = None;
        self.wheel = WheelAccumulator::default();
        self.sent_focus = None;
        self.connection.reconnect(self.options, self.active);
        self.live = self.connection.take_update().unwrap_or_default();
        self.set_surface(self.live.surface.clone(), cx);
    }

    fn resize(&mut self) {
        if self.last_queued_options == Some(self.options) {
            return;
        }
        if let (Some(handle), Some(snapshot)) = (&self.connection.handle, &self.live.snapshot) {
            match handle.resize(&snapshot.boot_id, self.options) {
                Ok(()) => self.last_queued_options = Some(self.options),
                Err(error) => self.local_error = Some(format!("Resize: {error}")),
            }
        }
    }

    fn report_focus(&mut self) {
        // Update the authoritative event inbox, not just the rendered clone.
        if let Ok(mut state) = self.connection.inbox.try_lock() {
            state.set_outer_focus(self.active);
        }
        if self.sent_focus == Some(self.active) {
            return;
        }
        if let (Some(handle), Some(snapshot)) = (&self.connection.handle, &self.live.snapshot)
            && handle.set_focus(&snapshot.boot_id, self.active).is_ok()
        {
            self.sent_focus = Some(self.active);
        }
    }

    fn send(&mut self, event: ClientPaneInputEvent, cx: &mut Context<Self>) {
        if self.menu.page.is_some() {
            return;
        }
        if let (Some(handle), Some(snapshot), Some(surface)) = (
            &self.connection.handle,
            &self.live.snapshot,
            &self.live.surface,
        ) {
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

    fn navigate(&mut self, target: NavigationTarget<'_>, cx: &mut Context<Self>) {
        if let (Some(handle), Some(snapshot)) = (&self.connection.handle, &self.live.snapshot) {
            let result = match target {
                NavigationTarget::Workspace(id) => handle.focus_workspace(&snapshot.boot_id, id),
                NavigationTarget::Tab(id) => handle.focus_tab(&snapshot.boot_id, id),
                NavigationTarget::Pane(id) => handle.focus_pane(&snapshot.boot_id, id),
            };
            if let Err(error) = result {
                self.local_error = Some(error.to_string());
            }
        }
        self.marked.clear();
        cx.notify();
    }

    fn command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.page.is_some() {
            return;
        }
        #[cfg(feature = "integration-test")]
        {
            self.input_probe.actions += 1;
        }
        if let (Some(handle), Some(snapshot)) = (&self.connection.handle, &self.live.snapshot)
            && let Some((method, params)) = controls::request(command, snapshot)
        {
            self.local_error = handle
                .request(&snapshot.boot_id, method, params)
                .err()
                .map(|error| format!("{method}: {error}"));
            self.marked.clear();
        }
        window.focus(&self.focus);
        cx.notify();
    }

    fn scroll_wheel(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.menu.page.is_some() {
            return;
        }
        let (Some(handle), Some(snapshot), Some(surface)) = (
            &self.connection.handle,
            &self.live.snapshot,
            &self.live.surface,
        ) else {
            return;
        };
        let x = (event.position.x - self.bounds.origin.x).to_f64() as f32;
        let y = (event.position.y - self.bounds.origin.y).to_f64() as f32;
        let Some(target) = wheel_target(surface, x, y, self.cell_width) else {
            self.wheel = WheelAccumulator::default();
            return;
        };
        let lines = self.wheel.lines(&target.target, event);
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
            let client = herdr_client::connect(view.connection.target.clone(), view.options)
                .unwrap_or_else(|error| panic!("cannot create test client: {error}"));
            client.handle.disconnect();
            view.connection.handle = Some(client.handle);
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
        let font = font("Menlo");
        self.cell_width = self.painter.borrow_mut().cell_width(&font, window, cx);
        let sidebar = self.render_sidebar(cx);
        let mut tabs = div()
            .id("tabs")
            .flex()
            .flex_none()
            .h(px(40.))
            .overflow_x_scroll()
            .bg(rgb(sidebar::BACKGROUND))
            .text_color(rgb(sidebar::FOREGROUND))
            .items_center();
        if let Some(snapshot) = &self.live.snapshot {
            for tab in snapshot
                .tabs
                .iter()
                .filter(|t| Some(&t.workspace_id) == snapshot.focused_workspace_id.as_ref())
            {
                let id = tab.tab_id.clone();
                tabs = tabs.child(
                    div()
                        .id(SharedString::from(format!("tab-{id}")))
                        .px_4()
                        .py_2()
                        .flex_none()
                        .cursor_pointer()
                        .bg(rgb(if tab.focused {
                            sidebar::ACTIVE
                        } else {
                            sidebar::BACKGROUND
                        }))
                        .child(tab.label.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate(NavigationTarget::Tab(&id), cx);
                            window.focus(&this.focus);
                        })),
                );
            }
        }
        let surface = self.live.surface.clone();
        let snapshot = self.live.snapshot.clone();
        let inbox = self.connection.inbox.clone();
        let entity = cx.entity();
        let paint_entity = entity.clone();
        let focus = self.focus.clone();
        let cell_width = self.cell_width;
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
            .bg(rgb(BACKGROUND))
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
                            / CELL_HEIGHT as f64)
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
                                ),
                                cell_width_px: cell_width.round().max(1.) as u32,
                                cell_height_px: CELL_HEIGHT as u32,
                            };
                            this.resize();
                        });
                    },
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, paint_entity.clone()),
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
                            cx.defer(move |cx| match inbox.try_lock() {
                                Ok(mut state) => {
                                    state.acknowledge_presented_surface(&snapshot, &surface, true);
                                }
                                Err(std::sync::TryLockError::WouldBlock) => {
                                    paint_entity.update(cx, |_, cx| cx.notify());
                                }
                                Err(std::sync::TryLockError::Poisoned(_)) => {}
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
            .on_action(cx.listener(|this, _: &Reconnect, window, cx| {
                if this.menu.page.is_some() {
                    return;
                }
                this.reconnect(cx);
                window.focus(&this.focus);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NewWorkspace, window, cx| {
                this.command(Command::Workspace, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &NewTab, window, cx| this.command(Command::Tab, window, cx)),
            )
            .on_action(cx.listener(|this, _: &SplitRight, window, cx| {
                this.command(Command::SplitRight, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitDown, window, cx| {
                this.command(Command::SplitDown, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.command(Command::NextTab, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| {
                this.command(Command::PreviousTab, window, cx)
            }))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(rgb(BACKGROUND))
            .text_color(rgb(FOREGROUND))
            .font_family(".SystemUIFont")
            .text_sm()
            .child(
                div().flex().flex_1().min_h_0().child(sidebar).child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .flex()
                                .flex_none()
                                .bg(rgb(sidebar::BACKGROUND))
                                .text_color(rgb(sidebar::FOREGROUND))
                                .child(tabs.flex_1().min_w_0())
                                .child(
                                    div()
                                        .id("new-tab")
                                        .px_4()
                                        .flex()
                                        .items_center()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(sidebar::ACTIVE)))
                                        .child("+")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.command(Command::Tab, window, cx)
                                        })),
                                ),
                        )
                        .child(terminal),
                ),
            )
            .child(
                div()
                    .id("connection-status")
                    .debug_selector(|| "connection-status".into())
                    .flex()
                    .flex_none()
                    .h(px(22.))
                    .overflow_hidden()
                    .items_center()
                    .gap(px(6.))
                    .px_3()
                    .bg(rgb(sidebar::BACKGROUND))
                    .text_color(rgb(sidebar::FOREGROUND))
                    .child(div().size(px(6.)).flex_none().rounded_full().bg(rgb(
                        if self.live.status.is_connected() {
                            0x78c998
                        } else {
                            0xe27c7c
                        },
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_xs()
                            .child(status),
                    )
                    .when(!self.marked.is_empty(), |d| {
                        d.child(format!("Composing: {}", self.marked))
                    }),
            )
            .when(self.menu.page.is_some(), |root| {
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
            "herdr-gpui [--socket CLIENT_SOCKET | --session NAME [--dev]]\nConnects to an existing local Herdr daemon; never starts or stops it."
        );
        #[cfg(feature = "integration-test")]
        println!(
            "  --integration-test  Run native GUI checks (requires explicit --socket)\n  --sidebar-test      Run native sidebar fixtures without connecting to a daemon\n  --performance-test  Measure native dense-terminal hover/scroll without a daemon (macOS)"
        );
        return std::process::ExitCode::SUCCESS;
    }
    #[cfg(feature = "integration-test")]
    let integration_test = mode == LaunchMode::Integration;
    #[cfg(feature = "integration-test")]
    let sidebar_test = mode == LaunchMode::Sidebar;
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
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-n", NewWorkspace, None),
            KeyBinding::new("cmd-t", NewTab, None),
            KeyBinding::new("cmd-d", SplitRight, None),
            KeyBinding::new("cmd-shift-d", SplitDown, None),
            KeyBinding::new("cmd-shift-]", NextTab, None),
            KeyBinding::new("cmd-shift-[", PreviousTab, None),
        ]);
        cx.set_menus(vec![
            Menu {
                name: "Herdr".into(),
                items: vec![MenuItem::action("Quit Herdr", Quit)],
            },
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("New Workspace", NewWorkspace),
                    MenuItem::action("New Tab", NewTab),
                ],
            },
            Menu {
                name: "Terminal".into(),
                items: vec![
                    MenuItem::action("Split Vertically (Right)", SplitRight),
                    MenuItem::action("Split Horizontally (Down)", SplitDown),
                    MenuItem::separator(),
                    MenuItem::action("Next Tab", NextTab),
                    MenuItem::action("Previous Tab", PreviousTab),
                    MenuItem::separator(),
                    MenuItem::action("Reconnect", Reconnect),
                ],
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
                            sidebar_test || performance_test
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
