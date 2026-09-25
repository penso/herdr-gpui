//! Device scope is presentation state; connection ownership stays in `endpoint`.
mod setup;

use super::Page;
use crate::{Command, HerdrWindow, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_client::ConnectTarget;

pub(super) const MENU_GAP: f32 = 12.;
/// The dot that marks a connected device, here and in the collapsed sidebar.
pub(crate) const CONNECTED: u32 = 0x63c68b;
pub(super) const MENU_WIDTH: f32 = 280.;

pub(super) struct Setup {
    fields: [Entity<SearchInput>; 3],
    launching: bool,
    launched: bool,
    task: Option<Task<()>>,
}

/// A one-line tooltip in the theme's colors, for controls that show an icon
/// where a label would not fit.
pub(crate) struct Hint {
    pub(crate) text: SharedString,
    pub(crate) foreground: u32,
    pub(crate) surface: u32,
}

impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(crate::config::corners::CONTROL))
            .shadow_md()
            .text_size(px(12.))
            .text_color(rgb(self.foreground))
            .bg(rgb(self.surface))
            .child(self.text.clone())
    }
}

impl HerdrWindow {
    pub(crate) fn device_visible(&self, id: &str) -> bool {
        self.device_filter
            .as_deref()
            .is_none_or(|filter| filter == id)
    }

    fn device_setup_unavailable(&self) -> Option<&'static str> {
        match &self.endpoints[0].connection.target {
            ConnectTarget::Socket(_) => {
                Some("Device setup is unavailable with an explicit socket.")
            }
            ConnectTarget::Session {
                development: true, ..
            } => Some("Device setup is unavailable with a development catalog."),
            _ if cfg!(windows) => Some("Saved SSH devices are unavailable on Windows."),
            _ => None,
        }
    }

    pub(crate) fn render_device_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let button_bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let painted_bounds = button_bounds.clone();
        let hint: SharedString = self
            .config
            .keybindings
            .shortcuts(Command::Settings)
            .first()
            .map_or_else(
                || "Settings".to_owned(),
                |shortcut| format!("Settings ({shortcut})"),
            )
            .into();
        let foreground = self.theme.foreground;
        let surface = self.theme.surface;
        let label = self
            .device_filter
            .as_ref()
            .and_then(|id| self.endpoints.iter().find(|endpoint| &endpoint.id == id))
            .map_or("All Devices", |endpoint| endpoint.label.as_str());
        let connected = self.endpoints.iter().any(|endpoint| {
            self.device_visible(&endpoint.id) && endpoint.live.status.is_connected()
        });
        div()
            .id("device-footer")
            .debug_selector(|| "device-footer".into())
            .h(px(crate::sidebar::DEVICE_FOOTER_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .border_t_1()
            .border_color(rgb(self.theme.active))
            .child(
                div()
                    .id("device-picker")
                    .relative()
                    .debug_selector(|| "device-picker".into())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .p(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .child(
                        svg()
                            .path("icons/devices.svg")
                            .size(px(16.))
                            .flex_none()
                            .text_color(rgb(self.theme.foreground)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label.to_owned()))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(if connected {
                                CONNECTED
                            } else {
                                self.theme.muted
                            })),
                    )
                    .child(
                        svg()
                            .path("icons/chevron-up.svg")
                            .size(px(12.))
                            .flex_none()
                            .text_color(rgb(self.theme.muted)),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, _, _| {
                                painted_bounds.set(bounds);
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        if this.open_menu(window, cx) {
                            // Anchor to the control, not the pointer: every click
                            // position leaves the same clear gap above the button.
                            this.menu.anchor = button_bounds.get().origin;
                            this.menu.page = Some(Page::Devices);
                            this.menu.selected = Some(0);
                        }
                    })),
            )
            .child(
                div()
                    .id("device-settings")
                    .debug_selector(|| "device-settings".into())
                    .size(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .tooltip(move |_, cx| {
                        cx.new(|_| Hint {
                            text: hint.clone(),
                            foreground,
                            surface,
                        })
                        .into()
                    })
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .child(
                        svg()
                            .path("icons/settings.svg")
                            .size(px(18.))
                            .text_color(rgb(self.theme.foreground)),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.command(Command::Settings, window, cx);
                    })),
            )
    }

    pub(super) fn render_devices(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self
            .endpoints
            .iter()
            .filter(|e| e.live.status.is_connected())
            .count();
        let count = self.endpoints.len();
        let mut rows = vec![(
            "All Devices".to_owned(),
            format!(
                "{count} {} · {connected} connected",
                if count == 1 { "device" } else { "devices" }
            ),
            self.device_filter.is_none(),
            true,
        )];
        rows.extend(self.endpoints.iter().map(|endpoint| {
            let detail = match &endpoint.connection.target {
                ConnectTarget::Ssh { target, session } => format!("{target} · {session}"),
                ConnectTarget::Socket(path) => path.display().to_string(),
                ConnectTarget::Session { name, .. } => format!("This device · {name}"),
                ConnectTarget::Local => "This device".into(),
            };
            (
                endpoint.label.clone(),
                format!("{detail} · {}", endpoint.status()),
                self.device_filter.as_ref() == Some(&endpoint.id),
                endpoint.enabled,
            )
        }));
        rows.push((
            "Add Device…".into(),
            self.device_setup_unavailable()
                .unwrap_or("Set up a remote host over SSH")
                .into(),
            false,
            self.device_setup_unavailable().is_none(),
        ));
        let chrome = crate::titlebar::HEIGHT
            + crate::worktree_banner::reserved(env!("HERDR_BUILD_WORKTREE") == "1");
        let mut view = div()
            .id("devices-list")
            .max_h(
                (self.menu.anchor.y - px(chrome + MENU_GAP + super::MENU_MARGIN + 12.))
                    .max(px(48.))
                    .min(px(420.)),
            )
            .overflow_y_scroll()
            .track_scroll(&self.menu.devices_scroll)
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .p(px(8.))
                    .text_color(rgb(self.theme.muted))
                    .child("DEVICES"),
            );
        for (index, (label, detail, checked, enabled)) in rows.into_iter().enumerate() {
            let endpoint = index.checked_sub(1).and_then(|i| self.endpoints.get(i));
            view = view.child(
                div()
                    .id(("device-row", index))
                    .debug_selector(move || format!("device-row-{index}"))
                    .p(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(self.menu.selected == Some(index), |s| {
                        s.bg(rgb(self.theme.active))
                    })
                    .when(enabled, |s| s.cursor_pointer())
                    .text_color(rgb(if enabled {
                        self.theme.foreground
                    } else {
                        self.theme.muted
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.))
                                    .when(index == 0, |row| {
                                        row.child(
                                            svg()
                                                .path("icons/devices.svg")
                                                .size(px(16.))
                                                .flex_none()
                                                .text_color(rgb(self.theme.foreground)),
                                        )
                                    })
                                    .when(index == self.endpoints.len() + 1, |row| {
                                        row.child(
                                            svg()
                                                .path("icons/plus.svg")
                                                .size(px(16.))
                                                .flex_none()
                                                .text_color(rgb(if enabled {
                                                    self.theme.foreground
                                                } else {
                                                    self.theme.muted
                                                })),
                                        )
                                    })
                                    .child(div().flex_1().min_w_0().truncate().child(label)),
                            )
                            .child(
                                div()
                                    .text_size(px(self.config.ui.size * 0.85))
                                    .text_color(rgb(self.theme.muted))
                                    .child(detail),
                            ),
                    )
                    .when_some(endpoint, |row, endpoint| {
                        row.child(
                            div()
                                .debug_selector(move || format!("device-dot-{index}"))
                                .size(px(7.))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(if endpoint.live.status.is_connected() {
                                    CONNECTED
                                } else {
                                    self.theme.muted
                                })),
                        )
                    })
                    .when(checked, |row| {
                        row.child(
                            div()
                                .debug_selector(move || format!("device-check-{index}"))
                                .flex_none()
                                .child("✓"),
                        )
                    })
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            this.menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_device(index, window, cx);
                    })),
            );
        }
        view
    }

    fn choose_device(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.endpoints.len() + 1 {
            if self.device_setup_unavailable().is_some() {
                return;
            }
            let fields = std::array::from_fn(|index| {
                let input = cx.new(SearchInput::new);
                input.update(cx, |input, cx| {
                    input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
                    input.set_placeholder(
                        ["user@hostname or SSH alias", "Device name", "default"][index],
                        cx,
                    );
                });
                input
            });
            window.focus(&fields[0].read(cx).focus.clone(), cx);
            self.menu.device_setup = Some(Setup {
                fields,
                launching: false,
                launched: false,
                task: None,
            });
            self.menu.page = Some(Page::AddDevice);
        } else {
            let filter = if index == 0 {
                None
            } else {
                let Some(endpoint) = self.endpoints.get(index - 1).filter(|e| e.enabled) else {
                    return;
                };
                Some(endpoint.id.clone())
            };
            self.dismiss_menu(window, cx);
            if let Some(id) = &filter
                && !self.select_endpoint(id, cx)
            {
                return;
            }
            self.device_filter = filter;
            for scroll in &self.sidebar_scroll {
                scroll.set_offset(Point::default());
            }
            for revealed in &self.sidebar_revealed {
                revealed.set(None);
            }
        }
        cx.notify();
    }

    pub(super) fn render_add_device(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let view = div()
            .debug_selector(|| "device-setup-dialog".into())
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .debug_selector(|| "device-setup-header".into())
                    .flex_none()
                    .p(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(self.config.ui.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Add Device"),
                    )
                    .child(
                        div()
                            .id("device-setup-close")
                            .debug_selector(|| "device-setup-close".into())
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded(px(crate::config::corners::CONTROL))
                            .hover(|s| s.bg(rgb(theme.active)))
                            .child("Close")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx)),
                            ),
                    ),
            );
        let Some(setup) = &self.menu.device_setup else {
            return view;
        };
        let mut body = div().id("device-setup-body").debug_selector(|| "device-setup-body".into())
            .min_h_0().overflow_y_scroll().p(px(16.)).flex().flex_col().gap(px(12.))
            .child(div().flex_none().text_color(rgb(theme.muted))
                .child("Herdr will prepare the remote server and save the device. Complete SSH prompts and installation approval in your terminal."));
        for (label, field) in ["SSH target", "Label", "Remote session (optional)"]
            .into_iter()
            .zip(&setup.fields)
        {
            body = body.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label)
                    .child(field.clone()),
            );
        }
        if let Some(error) = &self.menu.error {
            body = body.child(
                div()
                    .flex_none()
                    .text_color(super::danger(theme))
                    .child(error.clone()),
            );
        }
        if setup.launched {
            body = body.child(div().flex_none().text_color(rgb(theme.muted)).child("Continue setup in the terminal. Once saved, the device appears automatically in the device picker."));
        }
        let ready = !setup.launching && !setup.launched;
        view.child(body).child(
            div()
                .debug_selector(|| "device-setup-footer".into())
                .flex_none()
                .p(px(16.))
                .border_t_1()
                .border_color(rgb(theme.active))
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(
                    div()
                        .id("device-setup-submit")
                        .debug_selector(|| "device-setup-submit".into())
                        .p(px(8.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(self.theme.active))
                        .when(ready, |button| {
                            button.cursor_pointer().hover(|s| {
                                s.bg(rgb(theme.active).blend(rgba((theme.foreground << 8) | 0x20)))
                            })
                        })
                        .when(!ready, |button| button.text_color(rgb(theme.muted)))
                        .child(if setup.launching {
                            "Opening terminal…"
                        } else if setup.launched {
                            "Setup opened"
                        } else {
                            "Add device"
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.submit_device_setup(cx))),
                ),
        )
    }

    fn submit_device_setup(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        if form.launching || form.launched {
            return;
        }
        let request = setup::Request::new(
            form.fields[0].read(cx).text(),
            form.fields[1].read(cx).text(),
            form.fields[2].read(cx).text(),
        );
        let request = match request {
            Ok(request) => request,
            Err(error) => {
                self.menu.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        form.launching = true;
        self.menu.error = None;
        let background = cx
            .background_executor()
            .spawn(async move { setup::launch(request) });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(form) = &mut this.menu.device_setup {
                    form.launching = false;
                    form.launched = result.is_ok();
                    this.menu.error = result.err().map(|error| error.to_string());
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub(super) fn devices_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if self.menu.page == Some(Page::AddDevice) {
            let Some(form) = &self.menu.device_setup else {
                return;
            };
            if form
                .fields
                .iter()
                .any(|field| field.read(cx).is_composing())
            {
                return;
            }
            match key {
                "tab" => {
                    let index = form
                        .fields
                        .iter()
                        .position(|field| field.read(cx).focus.is_focused(window))
                        .unwrap_or(0);
                    let next = (index
                        + if event.keystroke.modifiers.shift {
                            2
                        } else {
                            1
                        })
                        % 3;
                    window.focus(&form.fields[next].read(cx).focus.clone(), cx);
                }
                "enter" => self.submit_device_setup(cx),
                "escape" => self.dismiss_menu(window, cx),
                _ => return,
            }
        } else {
            let count = self.endpoints.len() + 2;
            match key {
                "up" | "down" => {
                    let index = self.menu.selected.unwrap_or(0).min(count - 1);
                    self.menu.selected =
                        Some((index + if key == "up" { count - 1 } else { 1 }) % count);
                    if let Some(index) = self.menu.selected {
                        self.menu.devices_scroll.scroll_to_item(index + 1);
                    }
                    cx.notify();
                }
                "enter" => self.choose_device(self.menu.selected.unwrap_or(0), window, cx),
                "escape" => self.dismiss_menu(window, cx),
                _ => {}
            }
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}
