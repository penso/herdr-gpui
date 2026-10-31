//! Add Device's **Add provider** pane. The rail lists only the cloud providers
//! in use here, set up in Settings or holding a device, so a build with many
//! providers stays short; the others wait here as tiles that each open their
//! Settings tab.

use super::super::Page;
use crate::i18n::fl;
use crate::{HerdrWindow, cloud::CloudProvider};
use gpui::{prelude::*, *};
use herdr_client::ConnectTarget;

impl HerdrWindow {
    /// The providers the rail lists: configured here, or with a saved device.
    pub(super) fn cloud_listed(&self) -> Vec<CloudProvider> {
        let configured = self.cloud_providers();
        CloudProvider::ALL
            .iter()
            .copied()
            .filter(|provider| configured.contains(provider) || self.has_cloud_device(*provider))
            .collect()
    }

    /// The providers only the Add provider pane offers.
    pub(super) fn cloud_unlisted(&self) -> Vec<CloudProvider> {
        let listed = self.cloud_listed();
        CloudProvider::ALL
            .iter()
            .copied()
            .filter(|provider| !listed.contains(provider))
            .collect()
    }

    fn has_cloud_device(&self, provider: CloudProvider) -> bool {
        self.endpoints.iter().any(|endpoint| {
            matches!(&endpoint.connection.target,
                ConnectTarget::Cloud { provider: from, .. } if *from == provider)
        })
    }

    pub(super) fn open_add_provider(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.page = Some(Page::AddProvider);
        window.focus(&self.menu.focus.clone(), cx);
    }

    /// Settings > Cloud devices on its first tab, for a provider the user has
    /// not picked yet.
    fn open_cloud_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_menu(window, cx);
        #[cfg(not(test))]
        crate::settings_window::open_cloud(cx.weak_entity(), None, cx);
        #[cfg(test)]
        {
            self.menu.cloud_settings_opened = true;
        }
    }

    pub(super) fn render_add_provider(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let providers = self.cloud_unlisted();
        let mut list = div()
            .id("add-provider-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().pb(px(4.)).text_color(rgb(theme.subtext())).child(
                if providers.is_empty() {
                    fl!("add-device-providers-all-set")
                } else {
                    fl!("add-device-providers-lead")
                },
            ));
        let per_row = if compact { 1 } else { 2 };
        for (row, chunk) in providers.chunks(per_row).enumerate() {
            let mut tiles = div().flex().gap(px(8.));
            for (column, &provider) in chunk.iter().enumerate() {
                tiles = tiles.child(self.provider_tile(provider, row * per_row + column, cx));
            }
            for _ in chunk.len()..per_row {
                tiles = tiles.child(div().flex_1());
            }
            list = list.child(tiles);
        }
        let pane = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(list)
            .child(
                div()
                    .flex_none()
                    .p(px(16.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .flex()
                    .justify_end()
                    .child(
                        self.settings_button("add-provider-settings")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_cloud_settings(window, cx);
                            })),
                    ),
            );
        self.add_device_frame(
            "add-provider-dialog",
            "add-provider-close",
            compact,
            pane,
            cx,
        )
    }

    fn provider_tile(
        &self,
        provider: CloudProvider,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(("add-provider", index))
            .debug_selector(move || format!("add-provider-{}", provider.key()))
            .flex_1()
            .min_w_0()
            .p(px(10.))
            .flex()
            .gap(px(10.))
            .rounded(px(crate::config::corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(theme.active)))
            .child(
                div()
                    .flex_none()
                    .size(px(24.))
                    .rounded(px(crate::config::corners::SMALL))
                    .bg(rgb(theme.active))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .path(crate::cloud::icon(provider))
                            .size(px(14.))
                            .text_color(rgb(theme.foreground)),
                    ),
            )
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::cloud::name(provider)),
                    )
                    .child(
                        div()
                            .text_size(px(self.config.ui.size * 0.85))
                            .text_color(rgb(theme.subtext()))
                            .child(crate::cloud::offer(provider)),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_provider_settings(provider, window, cx);
            }))
    }

    /// Enter opens Settings > Cloud devices; Escape closes the dialog.
    pub(in crate::menu) fn add_provider_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => self.open_cloud_settings(window, cx),
            "escape" => self.dismiss_menu(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}
