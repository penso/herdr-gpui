//! Asks before creating the cloud machines `herdr-gpui cloud create` asked
//! for. Each one is billed by its provider until it is destroyed, so nothing
//! is created until the user confirms here, once for the whole batch. The
//! batch it answers is cancelled if the question closes unanswered.

// i18n: migrated
use super::Page;
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    cloud::{
        Shape,
        batch::{
            Registry,
            create::{self, Creation},
        },
    },
    control::batch::{BatchInfo, BatchKind, DeviceState},
};
use gpui::{prelude::*, *};
use std::time::Duration;

/// How often a running batch is looked at, to report how it ended.
const FOLLOW: Duration = Duration::from_secs(1);

impl HerdrWindow {
    /// Asks to create what `creation` describes, unless another page holds
    /// the window; the batch the answer goes to.
    pub(crate) fn ask_to_create_cloud(
        &mut self,
        creation: Creation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return None;
        }
        let devices = creation.devices();
        let pending = cx
            .default_global::<Registry>()
            .open(BatchKind::Create, devices, creation);
        let id = pending.id();
        self.menu.cloud_create = Some(pending);
        self.menu.page = Some(Page::CreateCloud);
        Some(id)
    }

    /// Starts the batch and closes the question.
    pub(in crate::menu) fn confirm_create_cloud(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pending = self.menu.cloud_create.take();
        self.dismiss_menu(window, cx);
        let Some((registry, id, creation)) = pending.and_then(|pending| pending.confirm()) else {
            return;
        };
        tracing::info!(
            category = "cloud_batch",
            batch = id,
            provider = creation.provider.key(),
            count = creation.count,
            "Creating cloud devices"
        );
        let count = creation.count as usize;
        create::start(&registry, id, count, move |report| {
            create::create(&creation, report)
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FOLLOW).await;
                let Some(batch) = registry.get(id) else {
                    return;
                };
                if !batch.phase.settled() {
                    continue;
                }
                let _ = this.update(cx, |this, cx| this.cloud_created(&batch, cx));
                return;
            }
        })
        .detach();
        cx.notify();
    }

    fn cloud_created(&mut self, batch: &BatchInfo, cx: &mut Context<Self>) {
        let ready = batch
            .devices
            .iter()
            .filter(|device| device.state == DeviceState::Ready)
            .count();
        let failed: Vec<_> = batch
            .devices
            .iter()
            .filter(|device| device.state == DeviceState::Failed)
            .collect();
        let total = batch.devices.len();
        let body = match failed.first() {
            None => fl!("cloud-create-done", ready = ready, total = total),
            Some(first) => fl!(
                "cloud-create-done-failed",
                ready = ready,
                total = total,
                failed = failed.len(),
                device = first.label.as_str(),
                error = first
                    .error
                    .clone()
                    .unwrap_or_else(|| fl!("cloud-create-unknown-error")),
            ),
        };
        self.local_transfer_notice(&fl!("cloud-create-done-title"), body, cx);
    }

    pub(in crate::menu) fn render_create_cloud(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(creation) = self.menu.cloud_create.as_ref().and_then(|p| p.work()) else {
            return div();
        };
        let line = |id: &'static str, text: String| {
            div()
                .debug_selector(move || id.into())
                .text_color(rgb(theme.subtext()))
                .child(text)
        };
        let provider = crate::cloud::name(creation.provider);
        let count = creation.count;
        div()
            .debug_selector(|| "create-cloud".into())
            .flex()
            .flex_col()
            .p(px(8.))
            .gap(px(8.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title(creation)),
            )
            .children(
                choices(creation)
                    .into_iter()
                    .map(|text| line("create-cloud-choice", text)),
            )
            .child(line(
                "create-cloud-cost",
                fl!("cloud-create-cost", provider = provider),
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .pt(px(4.))
                    .child(
                        div()
                            .id("create-cloud-cancel")
                            .debug_selector(|| "create-cloud-cancel".into())
                            .p(px(6.))
                            .cursor_pointer()
                            .child(fl!("cloud-create-cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .id("create-cloud-submit")
                            .debug_selector(|| "create-cloud-submit".into())
                            .p(px(6.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .cursor_pointer()
                            .child(fl!("cloud-create-submit", count = count))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.confirm_create_cloud(window, cx);
                            })),
                    ),
            )
    }
}

/// "Create 3 Coder workspaces?", "Create 1 Azure sandbox?".
fn title(creation: &Creation) -> String {
    let kind = match creation.spec.shape {
        // The selector the message chooses the noun by.
        Shape::Sandbox => "sandbox", // i18n: data
        Shape::Machine => crate::cloud::noun(creation.provider),
    };
    fl!(
        "cloud-create-title",
        count = creation.count,
        provider = crate::cloud::name(creation.provider),
        kind = kind,
    )
}

/// What the user is agreeing to besides the count, one line each.
fn choices(creation: &Creation) -> Vec<String> {
    let spec = &creation.spec;
    let mut lines = Vec::new();
    #[cfg(feature = "coder")]
    if creation.provider == crate::cloud::CloudProvider::Coder {
        lines.push(match (spec.template.as_deref(), spec.preset.as_deref()) {
            (Some(template), Some(preset)) => fl!(
                "cloud-create-template-preset",
                template = template,
                preset = preset
            ),
            (Some(template), None) => {
                fl!("cloud-create-template-default-preset", template = template)
            }
            (None, Some(preset)) => fl!("cloud-create-only-template-preset", preset = preset),
            (None, None) => fl!("cloud-create-only-template-default-preset"),
        });
    }
    lines.push(if spec.install {
        fl!("cloud-create-install")
    } else {
        fl!("cloud-create-no-install")
    });
    lines.push(if creation.setup {
        fl!("cloud-create-setup")
    } else {
        fl!("cloud-create-no-setup")
    });
    lines
}

#[cfg(test)]
mod tests;
