//! Remote control in the title bar, before the GitHub account. With no
//! phone paired it is a "Pair phone" button; once one is, it is a phone
//! with the number connected now, and opens a list of the phones: each can
//! be disconnected (revoked, after asking), and another paired.

use super::supervisor::{Health, Supervisor};
use crate::{HerdrWindow, menu::Page};
use gpui::{prelude::*, *};
use herdr_remote_control::control::Device;

/// The popover's width.
pub(crate) const MENU_WIDTH: f32 = 320.;

impl HerdrWindow {
    /// The title bar's remote control button, while remote control runs.
    pub(crate) fn remote_indicator(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !self.config.remote.enabled || !matches!(Supervisor::health(cx), Health::Running(_)) {
            return None;
        }
        let theme = &self.theme;
        let paired = Supervisor::devices(cx).is_none_or(|devices| !devices.is_empty());
        let connected = Supervisor::connected(cx);
        let label: SharedString = match (paired, connected) {
            (false, _) => "Pair a phone for remote control".into(),
            (true, 0) => "Remote control phones, none connected".into(),
            (true, 1) => "Remote control phones, 1 connected".into(),
            (true, count) => format!("Remote control phones, {count} connected").into(),
        };
        let background = rgb(theme.surface).blend(rgba(0xffffff1a));
        // Where the button painted, so its list hangs under it wherever the
        // title bar sits: lower on Linux, under the window's own frame.
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let painted = bounds.clone();
        let button = div()
            .id("titlebar-remote")
            .debug_selector(|| "titlebar-remote".into())
            .map(|control| self.chrome_stop(crate::window::chrome_focus::Region::TitleBar, control))
            .role(Role::Button)
            .aria_label(label)
            .flex()
            .relative()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .h(px(24.))
            .px(px(6.))
            .mr(px(2.))
            .rounded(px(crate::config::corners::CONTROL))
            .text_size(px(self.config.ui.size))
            .cursor_pointer()
            .hover(|link| link.bg(background.blend(rgba((theme.foreground << 8) | 0x14))))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                if paired {
                    let bounds = bounds.get();
                    let anchor = point(bounds.right(), bounds.bottom() + px(6.));
                    this.open_remote_menu(anchor, window, cx);
                } else if !this.open_remote_pairing(window, cx) {
                    this.dismiss_menu(window, cx);
                }
            }))
            .child(
                canvas(|_, _, _| (), move |bounds, _, _, _| painted.set(bounds))
                    .absolute()
                    .inset_0()
                    .size_full(),
            );
        let phone = svg()
            .path("icons/phone.svg")
            .size(px(14.))
            .flex_none()
            .text_color(rgb(if connected > 0 || !paired {
                theme.foreground
            } else {
                theme.muted
            }));
        Some(if paired {
            button
                .child(div().relative().child(phone).when(connected > 0, |icon| {
                    icon.child(
                        div()
                            .debug_selector(|| "titlebar-remote-live".into())
                            .absolute()
                            .top(px(-2.))
                            .right(px(-3.))
                            .size(px(7.))
                            .rounded_full()
                            .bg(rgb(crate::menu::online(theme))),
                    )
                }))
                .when(connected > 0, |button| {
                    button.child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(connected.to_string()),
                    )
                })
        } else {
            button
                .border_1()
                .border_color(rgb(theme.active))
                .text_color(rgb(theme.subtext()))
                .child(phone)
                .child("Pair phone")
        })
    }

    /// Opens the list of phones under the title bar, or closes the menu
    /// that is open.
    pub(crate) fn open_remote_menu(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            self.dismiss_menu(window, cx);
            return;
        }
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::RemoteDevices);
        Supervisor::refresh(cx);
        cx.notify();
    }

    pub(crate) fn render_remote_menu(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let devices = Supervisor::devices(cx)
            .map(<[Device]>::to_vec)
            .unwrap_or_default();
        let now = super::now_ms();
        let rule = || div().h(px(1.)).my(px(4.)).bg(rgb(theme.active));
        div()
            .debug_selector(|| "remote-menu".into())
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(6.))
            .child(
                div()
                    .px(px(6.))
                    .pb(px(2.))
                    .text_size(px(self.config.ui.size - 1.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.muted))
                    .child("PHONES"),
            )
            .children(
                devices
                    .iter()
                    .enumerate()
                    .map(|(index, device)| self.remote_menu_row(index, device, now, cx)),
            )
            .child(rule())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(6.))
                    .py(px(2.))
                    .child(
                        self.remote_menu_button("remote-menu-pair", "Pair a phone\u{2026}", true)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_menu(window, cx);
                                this.open_remote_pairing(window, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        self.remote_menu_button("remote-menu-settings", "Settings", false)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dismiss_menu(window, cx);
                                crate::settings_window::open_remote(cx.entity().downgrade(), cx);
                            })),
                    ),
            )
    }

    fn remote_menu_row(
        &self,
        index: usize,
        device: &Device,
        now_ms: u64,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let red = theme.ink(theme.palette[1]);
        let target = device.clone();
        div()
            .debug_selector(move || format!("remote-menu-device-{index}"))
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(6.))
            .py(px(5.))
            .child(
                div()
                    .relative()
                    .flex_none()
                    .child(svg().path("icons/phone.svg").size(px(18.)).text_color(rgb(
                        if device.connected {
                            theme.foreground
                        } else {
                            theme.muted
                        },
                    )))
                    .when(device.connected, |icon| {
                        icon.child(
                            div()
                                .absolute()
                                .bottom(px(-1.))
                                .right(px(-2.))
                                .size(px(7.))
                                .rounded_full()
                                .bg(rgb(crate::menu::online(theme))),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .truncate()
                            .child(crate::notifications::safe_text(&device.name, 64)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.config.ui.size - 1.))
                            .text_color(rgb(theme.muted))
                            .child(super::device_line(device, now_ms)),
                    ),
            )
            .child(
                div()
                    .id(("remote-menu-disconnect", index))
                    .debug_selector(move || format!("remote-menu-device-{index}-disconnect"))
                    .role(Role::Button)
                    .aria_label(format!(
                        "Disconnect {}",
                        crate::notifications::safe_text(&device.name, 64)
                    ))
                    .flex_none()
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(crate::config::corners::SMALL))
                    .border_1()
                    .border_color(rgb(crate::config::mix(theme.surface, red, 45)))
                    .text_size(px(self.config.ui.size - 1.))
                    .text_color(rgb(red))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(crate::config::mix(theme.surface, red, 12))))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dismiss_menu(window, cx);
                        this.ask_to_revoke(target.clone(), window, cx);
                    }))
                    .child("Disconnect"),
            )
    }

    fn remote_menu_button(
        &self,
        id: &'static str,
        label: &'static str,
        primary: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let fill = if primary {
            theme.primary()
        } else {
            theme.active
        };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .role(Role::Button)
            .aria_label(label)
            .flex_none()
            .px(px(10.))
            .py(px(4.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(fill))
            .text_color(rgb(theme.text_on(fill)))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .child(label)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
