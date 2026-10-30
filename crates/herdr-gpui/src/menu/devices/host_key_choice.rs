//! The Add Device form's opt-in to connect without verifying the host's key,
//! for hosts that keep no key to verify, such as `coder config-ssh`'s
//! `*.coder` aliases. It starts off for every form and is saved with the
//! device (see `config::ssh_hosts`).

use super::add_device::{Setup, Step};
use crate::HerdrWindow;
use gpui::{prelude::*, *};
use herdr_client::HostKeys;

impl HerdrWindow {
    pub(super) fn render_host_key_choice(
        &self,
        setup: &Setup,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = &self.theme;
        let skip = setup.host_keys == HostKeys::Skip;
        // A submitted request keeps the choice it was checked with.
        let editable = matches!(setup.step, Step::Form);
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .id("device-setup-skip-host-key")
                    .debug_selector(|| "device-setup-skip-host-key".into())
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(editable, |row| row.cursor_pointer())
                    .when(!editable, |row| row.opacity(0.6))
                    .child(crate::toggles::checkbox(
                        theme,
                        self.config.ui.size + 2.,
                        skip,
                    ))
                    .child("Skip host key verification (dangerous)")
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_host_key_choice(cx))),
            )
            .when(skip, |choice| {
                choice.child(div().text_color(crate::menu::danger(theme)).child(
                    "Herdr will accept any key this host offers, now and on every \
                         connection. Anyone who can intercept the connection can \
                         impersonate the host and read your terminals. Use it only for \
                         hosts that keep no key, such as Coder workspaces.",
                ))
            })
    }

    pub(super) fn toggle_host_key_choice(&mut self, cx: &mut Context<Self>) {
        let Some(setup) = &mut self.menu.device_setup else {
            return;
        };
        if !matches!(setup.step, Step::Form) {
            return;
        }
        setup.host_keys = match setup.host_keys {
            HostKeys::Verify => HostKeys::Skip,
            HostKeys::Skip => HostKeys::Verify,
        };
        cx.notify();
    }
}
