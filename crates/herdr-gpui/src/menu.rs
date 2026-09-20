use super::{HerdrWindow, sidebar};
use gpui::{prelude::*, *};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    Menu,
    Preferences,
    Keybinds,
    Update,
}

pub(super) struct MenuState {
    pub page: Option<Page>,
    pub anchor: Point<Pixels>,
    focus: FocusHandle,
    selected: usize,
}

impl MenuState {
    pub fn new(cx: &App) -> Self {
        Self {
            page: None,
            anchor: Point::default(),
            focus: cx.focus_handle(),
            selected: 0,
        }
    }
}

impl HerdrWindow {
    pub(super) fn open_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.page = Some(Page::Menu);
        self.menu.selected = 0;
        self.marked.clear();
        window.focus(&self.menu.focus);
        cx.notify();
    }

    fn dismiss_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.page = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn menu_items(&self) -> Vec<&'static str> {
        let mut items = vec!["settings", "keybinds"];
        if self.live.status.is_connected() {
            items.push("reload config");
        }
        if self
            .live
            .snapshot
            .as_ref()
            .is_some_and(|s| s.update_available.is_some())
        {
            items.push("update ready");
        }
        items.push(if self.connection.handle.is_some() {
            "detach"
        } else {
            "reconnect"
        });
        items
    }

    fn activate_menu(&mut self, item: &str, window: &mut Window, cx: &mut Context<Self>) {
        match item {
            "settings" => self.menu.page = Some(Page::Preferences),
            "keybinds" => self.menu.page = Some(Page::Keybinds),
            "update ready" => self.menu.page = Some(Page::Update),
            "reload config" => {
                if let (Some(handle), Some(snapshot)) =
                    (&self.connection.handle, &self.live.snapshot)
                {
                    self.local_error = handle
                        .request(
                            &snapshot.boot_id,
                            "server.reload_config",
                            serde_json::json!({}),
                        )
                        .err()
                        .map(|error| format!("Reload config: {error}"));
                }
                self.dismiss_menu(window, cx);
            }
            "detach" => {
                self.connection.detach(self.active);
                self.live = self.connection.take_update().unwrap_or_default();
                self.set_surface(self.live.surface.clone(), cx);
                self.local_error = None;
                self.marked.clear();
                self.dismiss_menu(window, cx);
            }
            "reconnect" => {
                self.reconnect(cx);
                self.dismiss_menu(window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn render_menu(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let page = self.menu.page.unwrap_or(Page::Menu);
        let mut panel = div()
            .id("menu-panel")
            .debug_selector(|| "menu-panel".into())
            .absolute()
            .left(px(56.))
            .bottom((window.viewport_size().height - self.menu.anchor.y + px(12.)).max(px(30.)))
            .w(px(if page == Page::Menu { 180. } else { 420. }))
            .max_h(window.viewport_size().height / 2. - px(12.))
            .overflow_y_scroll()
            .p(px(6.))
            .rounded(px(5.))
            .border_1()
            .border_color(rgb(sidebar::ACTIVE))
            .bg(rgb(sidebar::BACKGROUND))
            .text_color(rgb(sidebar::FOREGROUND))
            .font_family("Menlo")
            .text_size(px(12.))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation());
        if page == Page::Menu {
            for (index, item) in self.menu_items().into_iter().enumerate() {
                panel = panel.child(
                    div()
                        .id(item)
                        .debug_selector(move || format!("menu-{item}"))
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .when(index == self.menu.selected, |row| {
                            row.bg(rgb(sidebar::ACTIVE))
                        })
                        .hover(|row| row.bg(rgb(sidebar::ACTIVE)))
                        .child(item)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.activate_menu(item, window, cx);
                        })),
                );
            }
        } else {
            let (title, rows) = match page {
                Page::Preferences => ("Preferences (read-only)", vec![
                    format!("Connection: {}", self.live.status),
                    format!("Target: {:?}", self.connection.target),
                    "Terminal font: Menlo, 14 px (default)".into(),
                    "Sidebar font: Menlo, 12 px (default)".into(),
                    "Daemon configuration editing is not exposed by this native client. No configuration path is assumed.".into(),
                    "Reload config asks the connected daemon to reload its own configuration.".into(),
                ]),
                Page::Keybinds => ("Native keybinds", vec![
                    "Cmd-N   New workspace".into(), "Cmd-T   New tab".into(),
                    "Cmd-D   Split right".into(), "Cmd-Shift-D   Split down".into(),
                    "Cmd-Shift-] / [   Next / previous tab".into(),
                    "Cmd-V   Paste into terminal".into(), "Cmd-Q   Quit GUI (daemon stays running)".into(),
                    "Menu: Up / Down, Enter; Escape or outside click to dismiss.".into(),
                    "Daemon/TUI custom keybindings are not native GUI shortcuts.".into(),
                ]),
                _ => {
                    let snapshot = self.live.snapshot.as_ref();
                    ("Update ready", vec![
                        format!("Version: {}", snapshot.and_then(|s| s.update_available.as_deref()).unwrap_or("unavailable")),
                        "Suggested command (review and run yourself):".into(),
                        snapshot.map(|s| s.update_install_command.clone()).filter(|s| !s.trim().is_empty()).unwrap_or("No install command provided by daemon.".into()),
                        "Nothing is installed or executed by this panel.".into(),
                    ])
                }
            };
            panel = panel.child(div().p(px(8.)).child(title));
            for text in rows {
                panel = panel.child(div().p(px(8.)).child(text));
            }
            panel = panel.child(
                div()
                    .id("menu-close")
                    .p(px(8.))
                    .cursor_pointer()
                    .child("close (Escape)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss_menu(window, cx);
                    })),
            );
        }
        div()
            .id("menu-overlay")
            .absolute()
            .inset_0()
            .occlude()
            .track_focus(&self.menu.focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.dismiss_menu(window, cx);
                }),
            )
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                match event.keystroke.key.as_str() {
                    "escape" => this.dismiss_menu(window, cx),
                    "up" | "down" if this.menu.page == Some(Page::Menu) => {
                        let count = this.menu_items().len();
                        this.menu.selected = (this.menu.selected
                            + if event.keystroke.key == "up" {
                                count - 1
                            } else {
                                1
                            })
                            % count;
                        cx.notify();
                    }
                    "enter" if this.menu.page == Some(Page::Menu) => {
                        if let Some(item) = this.menu_items().get(this.menu.selected) {
                            this.activate_menu(item, window, cx);
                        }
                    }
                    _ => {}
                }
            }))
            .child(panel)
    }
}
