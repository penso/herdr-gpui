//! Asks before pairing a phone with a machine the desktop did not allow
//! when it allowed the phone: the phone asked for it later, from its list
//! of this computer's machines. Closing the question without an answer is a
//! no, so the phone hears back at once.

use super::Job;
use crate::{
    HerdrWindow,
    i18n::fl,
    menu::Page,
    notifications::safe_text,
    remote::{
        dialog::{Tone, button},
        supervisor::{Client, Supervisor},
    },
};
use gpui::{prelude::*, *};
use herdr_remote_control::control::{Request, hosts::HostOutcome};

/// The question on screen. Dropping it unanswered, as closing the dialog
/// does, declines.
pub(crate) struct HostAsk {
    job: Option<Job>,
    host_name: String,
    client: Option<Client>,
}

impl Drop for HostAsk {
    fn drop(&mut self) {
        if let (Some(job), Some(client)) = (self.job.take(), &self.client) {
            client.notify(Request::HostPaired {
                device: job.pairing.device,
                host: job.pairing.host,
                public_key: Some(job.pairing.public_key),
                outcome: HostOutcome::Denied,
            });
        }
    }
}

impl HerdrWindow {
    /// Tells the supervisor which machines this window connects to, and
    /// asks about a phone's request for one when this window is in front
    /// and free.
    pub(crate) fn sync_remote_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let known = super::known(self.endpoints.iter().map(|endpoint| {
            (
                endpoint.id.as_str(),
                endpoint.label.as_str(),
                &endpoint.connection.target,
                endpoint.enabled,
            )
        }));
        Supervisor::set_hosts(cx, known);
        if self.menu.page.is_some() || !window.is_window_active() {
            return;
        }
        let Some(job) = Supervisor::take_host_ask(cx) else {
            return;
        };
        let host_name = self
            .endpoints
            .iter()
            .find(|endpoint| endpoint.id == job.pairing.host.0)
            .map_or_else(
                || job.pairing.host.0.clone(),
                |endpoint| endpoint.label.clone(),
            );
        let ask = HostAsk {
            job: Some(job),
            host_name,
            client: Supervisor::client(cx),
        };
        if !self.open_menu(window, cx) {
            // Dropped unanswered: declined, and the phone may ask again.
            return;
        }
        self.menu.remote_host_ask = Some(ask);
        self.menu.page = Some(Page::RemoteHostAsk);
        cx.notify();
    }

    fn answer_host_ask(&mut self, allow: bool, window: &mut Window, cx: &mut Context<Self>) {
        let job = self
            .menu
            .remote_host_ask
            .as_mut()
            .and_then(|ask| ask.job.take());
        self.dismiss_menu(window, cx);
        if let Some(job) = job {
            Supervisor::answer_host(cx, job, allow);
        }
    }

    pub(crate) fn render_host_ask(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(ask) = &self.menu.remote_host_ask else {
            return div();
        };
        let Some(job) = &ask.job else {
            return div();
        };
        let phone = safe_text(&job.pairing.name, 64);
        let host = safe_text(&ask.host_name, 64);
        div()
            .debug_selector(|| "remote-host-ask".into())
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(12.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(fl!(
                "remote-links-ask-title",
                phone = phone.clone(),
                host = host.clone()
            )))
            .child(div().text_color(rgb(theme.subtext())).child(fl!(
                "remote-links-ask-detail",
                phone = phone,
                host = host
            )))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(
                            theme,
                            "remote-host-ask-deny",
                            fl!("remote-links-ask-deny"),
                            Tone::Plain,
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.answer_host_ask(false, window, cx);
                        })),
                    )
                    .child(
                        button(
                            theme,
                            "remote-host-ask-allow",
                            fl!("remote-links-ask-allow"),
                            Tone::Primary,
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.answer_host_ask(true, window, cx);
                        })),
                    ),
            )
    }
}
