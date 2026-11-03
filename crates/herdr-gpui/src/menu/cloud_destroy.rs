//! Asks before destroying saved cloud devices that `herdr-gpui cloud destroy`
//! named. Deleting a machine loses everything on it and cannot be undone, so
//! nothing happens until the user confirms here, once for the whole batch.
//! The batch it answers is cancelled if the question closes unanswered.

// i18n: migrated
use super::Page;
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    cloud::batch::{Registry, Target, destroy},
    control::batch::{BatchInfo, DeviceState},
};
use gpui::{prelude::*, *};
use std::time::Duration;

/// How often a running batch is looked at, to report how it ended.
const FOLLOW: Duration = Duration::from_millis(500);

impl HerdrWindow {
    /// Whether a cloud batch question (create or destroy) is open, for the
    /// menu's layout.
    pub(in crate::menu) fn cloud_batch_open(&self) -> bool {
        matches!(self.menu.page, Some(Page::CreateCloud | Page::DestroyCloud))
    }

    /// The open cloud batch question's body.
    pub(in crate::menu) fn cloud_batch_page(&self, cx: &mut Context<Self>) -> Option<Div> {
        match self.menu.page {
            Some(Page::CreateCloud) => Some(self.render_create_cloud(cx)),
            Some(Page::DestroyCloud) => Some(self.render_destroy_cloud(cx)),
            _ => None,
        }
    }

    /// Asks to destroy `targets`, unless another page holds the window; the
    /// batch the answer goes to.
    pub(crate) fn ask_to_destroy_cloud(
        &mut self,
        targets: &[Target],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return None;
        }
        let pending = cx.default_global::<Registry>().open_destroy(targets);
        let id = pending.id();
        self.menu.cloud_destroy = Some(pending);
        self.menu.page = Some(Page::DestroyCloud);
        Some(id)
    }

    /// Starts the batch and closes the question. Each device's sidebar header
    /// pulses until the catalog drops it; one that fails stops pulsing.
    pub(in crate::menu) fn confirm_destroy_cloud(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending = self.menu.cloud_destroy.take();
        self.dismiss_menu(window, cx);
        let Some((registry, id, targets)) = pending.and_then(|pending| pending.confirm()) else {
            return;
        };
        tracing::info!(
            category = "cloud_batch",
            batch = id,
            devices = targets.len(),
            "Destroying cloud devices"
        );
        self.menu
            .removing_devices
            .extend(targets.iter().map(|target| target.endpoint.clone()));
        destroy::start(&registry, id, targets, destroy::destroy);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FOLLOW).await;
                let Some(batch) = registry.get(id) else {
                    return;
                };
                if !batch.phase.settled() {
                    continue;
                }
                let _ = this.update(cx, |this, cx| this.cloud_destroyed(&batch, cx));
                return;
            }
        })
        .detach();
        cx.notify();
    }

    fn cloud_destroyed(&mut self, batch: &BatchInfo, cx: &mut Context<Self>) {
        let failed: Vec<_> = batch
            .devices
            .iter()
            .filter(|device| device.state == DeviceState::Failed)
            .collect();
        for device in &failed {
            self.menu.removing_devices.remove(&device.endpoint);
        }
        if let Some(first) = failed.first() {
            self.local_error = Some(fl!(
                "cloud-destroy-failed",
                failed = failed.len(),
                total = batch.devices.len(),
                device = first.label.as_str(),
                error = first
                    .error
                    .clone()
                    .unwrap_or_else(|| fl!("cloud-destroy-unknown-error")),
            ));
        }
        cx.notify();
    }

    pub(in crate::menu) fn render_destroy_cloud(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(pending) = &self.menu.cloud_destroy else {
            return div();
        };
        let targets = pending.work().map_or(&[][..], Vec::as_slice);
        let rows = targets.iter().enumerate().map(|(index, target)| {
            div()
                .debug_selector(move || format!("destroy-cloud-device-{index}"))
                .flex()
                .flex_col()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(target.label.clone()),
                )
                .child(
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(consequence(target)),
                )
        });
        let count = targets.len();
        div()
            .debug_selector(|| "destroy-cloud".into())
            .flex()
            .flex_col()
            .min_h_0()
            .p(px(8.))
            .gap(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("cloud-destroy-title", count = count)),
            )
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("cloud-destroy-note")),
            )
            .child(
                div()
                    .id("destroy-cloud-list")
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("destroy-cloud-cancel")
                            .debug_selector(|| "destroy-cloud-cancel".into())
                            .p(px(6.))
                            .cursor_pointer()
                            .child(fl!("cloud-destroy-cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("destroy-cloud-submit")
                            .debug_selector(|| "destroy-cloud-submit".into())
                            .p(px(6.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .text_color(super::danger(theme))
                            .cursor_pointer()
                            .child(fl!("cloud-destroy-submit", count = count))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.confirm_destroy_cloud(window, cx);
                            })),
                    ),
            )
    }
}

/// What confirming does to one device, as its row says it.
fn consequence(target: &Target) -> String {
    let provider = crate::cloud::name(target.provider);
    match crate::cloud::deletion(target.provider) {
        Some(deletion) => fl!(
            "cloud-destroy-delete",
            machine = (deletion.noun)(&target.id),
            provider = provider,
            leftovers = deletion.leftovers,
        ),
        None => fl!(
            "cloud-destroy-forget",
            provider = provider,
            machine = crate::cloud::noun(target.provider),
        ),
    }
}

#[cfg(test)]
mod tests;
