//! Asks before revoking a paired device, as `herdr-gpui remote revoke`
//! does: revoking ends its sessions and drops its notifications at once, so
//! nothing is sent until the user confirms here.

use super::{
    dialog::{Tone, button},
    supervisor::Supervisor,
};
use crate::{HerdrWindow, menu::Page};
use gpui::{prelude::*, *};
use herdr_remote_control::control::Device;

/// The device the revoke question is about.
pub(crate) struct Revoke {
    pub device: Device,
}

impl HerdrWindow {
    /// Asks to revoke `device`, unless another page holds the window.
    pub(crate) fn ask_to_revoke(
        &mut self,
        device: Device,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return false;
        }
        self.menu.remote_revoke = Some(Revoke { device });
        self.menu.page = Some(Page::RevokeDevice);
        cx.notify();
        true
    }

    /// Sends the revoke and closes the question; a failure shows in the
    /// status bar.
    pub(crate) fn confirm_revoke(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let revoke = self.menu.remote_revoke.take();
        self.dismiss_menu(window, cx);
        let Some(Revoke { device }) = revoke else {
            return;
        };
        let this = cx.entity().downgrade();
        let name = crate::notifications::safe_text(&device.name, 64);
        Supervisor::revoke(cx, device.id, move |result, cx| {
            let Err(error) = result else {
                return;
            };
            let _ = this.update(cx, |view, cx| {
                view.local_error = Some(format!("Could not revoke {name}: {error}"));
                cx.notify();
            });
        });
    }

    pub(super) fn render_revoke(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(Revoke { device }) = &self.menu.remote_revoke else {
            return div();
        };
        let name = crate::notifications::safe_text(&device.name, 64);
        div()
            .debug_selector(|| "remote-revoke".into())
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Revoke {name}?")),
            )
            .child(div().text_color(rgb(theme.subtext())).child(
                "It loses remote control of your chats at once: its sessions end and it gets no \
                 more notifications. Pairing it again needs a new code.",
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(theme, "remote-revoke-cancel", "Cancel", Tone::Plain, true)
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    .child(
                        button(theme, "remote-revoke-submit", "Revoke", Tone::Danger, true)
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.confirm_revoke(window, cx);
                            })),
                    ),
            )
    }
}
