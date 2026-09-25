//! Device scope is presentation state; connection ownership stays in `endpoint`.
mod setup;

use super::{Page, dialog_buttons, error_alert, listener, submit};
use crate::{Command, HerdrWindow};
use gpui_kit::{
    component::{
        ActiveTheme as _, Disableable as _, IconName,
        button::{Button, ButtonVariants as _},
        dialog::Dialog,
        form::{Field, Form},
        input::{Input, InputState},
        menu::PopupMenuItem,
        v_flex,
    },
    prelude::*,
    *,
};
use herdr_client::ConnectTarget;

/// Clear space between the device picker and the menu it opens above itself.
const MENU_GAP: f32 = 8.;

pub(super) struct Setup {
    fields: [Entity<InputState>; 3],
    launching: bool,
    launched: bool,
    task: Option<Task<()>>,
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
        use gpui_kit::component::{
            ActiveTheme, Icon, IconName, Sizable,
            badge::Badge,
            button::{Button, ButtonVariants},
            sidebar::SidebarFooter,
        };
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
        let label = self
            .device_filter
            .as_ref()
            .and_then(|id| self.endpoints.iter().find(|endpoint| &endpoint.id == id))
            .map_or("All Devices", |endpoint| endpoint.label.as_str())
            .to_owned();
        let connected = self.endpoints.iter().any(|endpoint| {
            self.device_visible(&endpoint.id) && endpoint.live.status.is_connected()
        });
        let theme = cx.theme();
        let dot = if connected {
            theme.success
        } else {
            theme.muted_foreground
        };
        let muted = theme.muted_foreground;
        div().px_2().pb_2().child(
            SidebarFooter::new()
                .debug_selector(|| "device-footer".into())
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .child(
                            Button::new("device-picker")
                                .debug_selector(|| "device-picker".into())
                                .ghost()
                                .small()
                                .w_full()
                                .child(
                                    Badge::new()
                                        .dot()
                                        .color(dot)
                                        .child(Icon::empty().path("icons/devices.svg").size_4()),
                                )
                                .child(div().flex_1().min_w_0().truncate().child(label))
                                .child(Icon::new(IconName::ChevronUp).xsmall().text_color(muted))
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    // Anchor to the control, not the pointer: every
                                    // click position leaves the same clear gap above
                                    // the button.
                                    this.open_devices_menu(button_bounds.get().origin, window, cx);
                                })),
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
                        ),
                )
                .child(
                    Button::new("device-settings")
                        .debug_selector(|| "device-settings".into())
                        .ghost()
                        .small()
                        .icon(IconName::Settings)
                        .tooltip(hint)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.command(Command::Settings, window, cx);
                        })),
                ),
        )
    }

    /// The device picker: every endpoint, the combined view, and device
    /// setup, opening upward from `anchor` (the picker's top-left corner).
    pub(crate) fn open_devices_menu(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.begin_menu(window, cx) {
            return;
        }
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
        let setup = self.device_setup_unavailable();
        let weak = cx.weak_entity();
        let add = self.endpoints.len() + 1;
        self.show_popup(
            Page::Devices,
            anchor - point(px(0.), px(MENU_GAP)),
            window,
            cx,
            move |menu, _, _| {
                let menu = rows.into_iter().enumerate().fold(
                    menu.min_w(px(280.)).label("Devices"),
                    |menu, (index, (label, detail, checked, enabled))| {
                        menu.item(
                            PopupMenuItem::element(move |_, cx| {
                                v_flex()
                                    .debug_selector(move || format!("device-row-{index}"))
                                    .min_w_0()
                                    .child(div().truncate().child(label.clone()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(detail.clone()),
                                    )
                            })
                            .checked(checked)
                            .disabled(!enabled)
                            .on_click(listener(&weak, move |this, window, cx| {
                                this.choose_device(index, window, cx)
                            })),
                        )
                    },
                );
                menu.separator().item(
                    PopupMenuItem::new(setup.unwrap_or("Add Device…"))
                        .icon(IconName::Plus)
                        .disabled(setup.is_some())
                        .on_click(listener(&weak, move |this, window, cx| {
                            this.choose_device(add, window, cx)
                        })),
                )
            },
        );
        if let Some(popup) = &mut self.menu.popup {
            popup.corner = Anchor::BottomLeft;
        }
    }

    fn choose_device(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.endpoints.len() + 1 {
            if self.device_setup_unavailable().is_some() {
                return;
            }
            let fields = std::array::from_fn(|index| {
                cx.new(|cx| {
                    InputState::new(window, cx).placeholder(
                        ["user@hostname or SSH alias", "Device name", "default"][index],
                    )
                })
            });
            let first = fields[0].clone();
            self.menu.device_setup = Some(Setup {
                fields,
                launching: false,
                launched: false,
                task: None,
            });
            self.show_dialog(Page::AddDevice, window, cx, |this, dialog, weak, _, cx| {
                this.add_device_dialog(dialog, weak, cx)
            });
            first.update(cx, |field, cx| field.focus(window, cx));
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

    fn add_device_dialog(
        &self,
        dialog: Dialog,
        weak: &WeakEntity<HerdrWindow>,
        cx: &App,
    ) -> Dialog {
        let Some(setup) = &self.menu.device_setup else {
            return dialog;
        };
        let ready = !setup.launching && !setup.launched;
        let form = ["SSH target", "Label", "Remote session (optional)"]
            .into_iter()
            .zip(&setup.fields)
            .fold(Form::new(), |form, (label, field)| {
                form.child(Field::new().label(label).child(Input::new(field)))
            });
        dialog
            .title("Add Device")
            .child(
                v_flex()
                    .debug_selector(|| "device-setup-body".into())
                    .gap_3()
                    .child(div().text_color(cx.theme().muted_foreground).child(
                        "Herdr will prepare the remote server and save the device. Complete SSH prompts and installation approval in your terminal.",
                    ))
                    .child(form)
                    .children(error_alert("device-setup-error", self.menu.error.as_ref()))
                    .when(setup.launched, |body| {
                        body.child(div().text_color(cx.theme().muted_foreground).child(
                            "Continue setup in the terminal. Once saved, the device appears automatically in the device picker.",
                        ))
                    }),
            )
            .footer(dialog_buttons(
                weak,
                Some(
                    Button::new("device-setup-submit")
                        .primary()
                        .loading(setup.launching)
                        .disabled(!ready)
                        .label(if setup.launching {
                            "Opening terminal…"
                        } else if setup.launched {
                            "Setup opened"
                        } else {
                            "Add device"
                        })
                        .on_click(listener(weak, |this, _, cx| this.submit_device_setup(cx))),
                ),
            ))
            .on_ok(submit(weak, |this, _, cx| this.submit_device_setup(cx)))
    }

    fn submit_device_setup(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        if form.launching || form.launched {
            return;
        }
        let request = setup::Request::new(
            &form.fields[0].read(cx).value(),
            &form.fields[1].read(cx).value(),
            &form.fields[2].read(cx).value(),
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
}
