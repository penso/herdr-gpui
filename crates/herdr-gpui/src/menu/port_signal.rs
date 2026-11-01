//! Asks before stopping or pausing processes that listen on this computer's
//! ports, from the Ports section or `herdr-gpui ports stop|pause`. Nothing
//! is signalled until the user confirms here, once for the whole batch, and
//! the worker checks again that each pid still names the process shown.
// i18n: migrated

use super::Page;
use crate::{
    HerdrWindow,
    i18n::{Localize, fl},
    local_ports::Pending,
};
use gpui::{prelude::*, *};
use herdr_control_protocol::protocol::hosts::PortAction;

impl HerdrWindow {
    /// Shows the question for `pending`, unless another page holds the
    /// window; whether it is shown.
    pub(crate) fn ask_to_signal_ports(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if pending.listeners.is_empty()
            || pending.action == PortAction::Resume
            || self.menu.page.is_some()
            || !self.open_menu(window, cx)
        {
            return false;
        }
        self.menu.port_signal = Some(pending);
        self.menu.page = Some(Page::PortSignal);
        cx.notify();
        true
    }

    /// Sends the signal the question is about, and closes the question.
    pub(in crate::menu) fn confirm_port_signal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending = self.menu.port_signal.take();
        self.dismiss_menu(window, cx);
        let Some(pending) = pending else {
            return;
        };
        if let Err(error) = self.local_ports.signal(pending.action, pending.victims()) {
            self.local_error = Some(error.localize());
        }
        cx.notify();
    }

    pub(in crate::menu) fn render_port_signal(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(pending) = &self.menu.port_signal else {
            return div();
        };
        let count = pending.victims().len();
        let (title, note, confirm) = match pending.action {
            PortAction::Pause => (
                fl!("local-ports-confirm-pause-title", count = count),
                fl!("local-ports-confirm-pause-note"),
                fl!("local-ports-confirm-pause"),
            ),
            PortAction::Stop | PortAction::Resume => (
                fl!("local-ports-confirm-stop-title", count = count),
                fl!("local-ports-confirm-stop-note"),
                fl!("local-ports-confirm-stop"),
            ),
        };
        let rows = pending
            .listeners
            .iter()
            .enumerate()
            .map(|(index, listener)| {
                div()
                    .debug_selector(move || format!("port-signal-row-{index}"))
                    .truncate()
                    .child(fl!(
                        "local-ports-confirm-row",
                        process = listener.process.as_str(),
                        pid = listener.identity.pid,
                        address = listener.address()
                    ))
            });
        div()
            .debug_selector(|| "port-signal".into())
            .flex()
            .flex_col()
            .min_h_0()
            .p(px(8.))
            .gap(px(12.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .id("port-signal-list")
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(div().text_color(rgb(theme.subtext())).child(note))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("port-signal-cancel")
                            .debug_selector(|| "port-signal-cancel".into())
                            .p(px(6.))
                            .cursor_pointer()
                            .child(fl!("local-ports-confirm-cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("port-signal-confirm")
                            .debug_selector(|| "port-signal-confirm".into())
                            .p(px(6.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .text_color(super::danger(theme))
                            .cursor_pointer()
                            .child(confirm)
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.confirm_port_signal(window, cx);
                            })),
                    ),
            )
    }
}
