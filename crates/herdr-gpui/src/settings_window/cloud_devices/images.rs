//! The Images section of a provider's card: each project's sandbox image
//! there, a build running now with its progress, and **Log**, **Build now**,
//! and **Reset**. A watcher follows the shared build registry, so a build
//! started by a dispatch or another window shows here as it runs.
// i18n: migrated

use super::super::SettingsWindow;
use crate::cloud::{
    CloudProvider,
    image::{
        builds::{self, Running},
        catalog::{self, Entry, Outcome},
    },
    worker::{self, Worker},
};
use crate::{fonts::StyledFont, i18n::fl};
use gpui::{prelude::*, *};
use std::time::Duration;

/// How long the watcher waits on the registry before checking whether
/// Settings closed.
const WATCH: Duration = Duration::from_secs(2);
/// The least time between two redraws for builds' progress.
const COALESCE: Duration = Duration::from_millis(200);
/// Log lines the open log shows.
const LOG_SHOWN: usize = 40;

#[derive(Default)]
pub(in crate::settings_window) struct SandboxImages {
    projects: Vec<Entry>,
    running: Vec<Running>,
    /// The row whose log is open: its provider and project.
    log: Option<(CloudProvider, String)>,
    /// The row whose **Reset** waits for a second click.
    confirm: Option<(CloudProvider, String)>,
    message: Option<String>,
    watcher: Option<Worker>,
    reset: Option<Worker>,
    /// Off in tests unless one turns it on, so a test's own rows are not
    /// replaced by the real registry's.
    #[cfg(test)]
    follows: bool,
}

enum Update {
    Changed {
        /// The catalog, when it changed.
        projects: Option<Vec<Entry>>,
        running: Vec<Running>,
    },
    Reset(crate::Result<()>),
}

impl SettingsWindow {
    /// Follow the build registry and the catalog while Settings is open.
    pub(super) fn watch_sandbox_images(&mut self, cx: &mut Context<Self>) {
        if self.sandbox_images.watcher.is_some() {
            return;
        }
        #[cfg(test)]
        if !self.sandbox_images.follows {
            return;
        }
        let watcher = worker::spawn(
            "herdr-image-watch",
            cx,
            |cancelled, send| {
                let mut revision = None;
                let mut read = None;
                while !cancelled() {
                    let (now, running) = match revision {
                        None => builds::snapshot(),
                        Some(revision) => builds::watch(revision, WATCH),
                    };
                    if revision == Some(now) {
                        continue;
                    }
                    revision = Some(now);
                    // Most changes are a build's output lines: the catalog
                    // is reread only when an image was recorded or reset.
                    let catalog = builds::catalog_revision();
                    let projects = (read != Some(catalog)).then(|| {
                        read = Some(catalog);
                        catalog::load().unwrap_or_else(|error| {
                            tracing::warn!(category = "sandbox_image", %error, "Could not read sandbox images");
                            Vec::new()
                        })
                    });
                    send(Update::Changed { projects, running });
                    // A busy build would otherwise redraw for every line.
                    std::thread::sleep(COALESCE);
                }
            },
            |this: &mut Self, update, cx| this.apply_sandbox_images(update, cx),
        );
        match watcher {
            Ok(watcher) => self.sandbox_images.watcher = Some(watcher),
            Err(error) => {
                tracing::warn!(category = "sandbox_image", %error, "Could not follow sandbox image builds");
            }
        }
    }

    fn apply_sandbox_images(&mut self, update: Update, cx: &mut Context<Self>) {
        let images = &mut self.sandbox_images;
        match update {
            Update::Changed { projects, running } => {
                if let Some(projects) = projects {
                    images.projects = projects;
                }
                images.running = running;
            }
            Update::Reset(result) => {
                images.reset = None;
                images.message = result.err().map(|error| error.to_string());
            }
        }
        cx.notify();
    }

    fn build_sandbox_image_now(
        &mut self,
        provider: CloudProvider,
        project_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .sandbox_images
            .projects
            .iter()
            .find(|entry| entry.project.id == project_id)
            .cloned()
        else {
            return;
        };
        let started = self.source.update(cx, |window, cx| {
            window.build_sandbox_image(
                provider,
                &entry.origin.endpoint_id,
                entry.origin.path.clone(),
                entry.project.label.clone(),
                cx,
            )
        });
        self.sandbox_images.message = match started {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(fl!(
                "settings-cloud-image-build-error",
                project = entry.project.label.clone(),
                error = error.to_string(),
                path = entry.origin.path.clone()
            )),
            Err(_) => Some(fl!("settings-cloud-image-no-window")),
        };
        cx.notify();
    }

    fn reset_sandbox_image(
        &mut self,
        provider: CloudProvider,
        project_id: String,
        cx: &mut Context<Self>,
    ) {
        let images = &mut self.sandbox_images;
        if images.confirm.as_ref() != Some(&(provider, project_id.clone())) {
            images.confirm = Some((provider, project_id));
            cx.notify();
            return;
        }
        images.confirm = None;
        let Some(support) = crate::cloud::image_support(provider) else {
            return;
        };
        let reset = worker::spawn(
            "herdr-image-reset",
            cx,
            move |_, send| {
                send(Update::Reset(
                    builds::reset(provider, support, &project_id).map_err(Into::into),
                ))
            },
            |this: &mut Self, update, cx| this.apply_sandbox_images(update, cx),
        );
        match reset {
            Ok(reset) => self.sandbox_images.reset = Some(reset),
            Err(_) => {
                self.sandbox_images.message =
                    Some(crate::cloud::Error::Worker("image reset").to_string()); // i18n: data
            }
        }
        cx.notify();
    }

    /// The Images card for `provider`'s tab.
    pub(super) fn render_sandbox_images(
        &self,
        provider: CloudProvider,
        cx: &mut Context<Self>,
    ) -> Div {
        let images = &self.sandbox_images;
        let mut card = self
            .cloud_card(fl!("settings-cloud-images"))
            .debug_selector(|| "cloud-images".into())
            .child(self.control_note(fl!("settings-cloud-images-note", provider = provider.key())));
        let mut rows: Vec<(String, String)> = images
            .projects
            .iter()
            .filter(|entry| entry.images.iter().any(|image| image.provider == provider))
            .map(|entry| (entry.project.id.clone(), entry.project.label.clone()))
            .collect();
        for running in images
            .running
            .iter()
            .filter(|running| running.provider == provider)
        {
            if !rows.iter().any(|(id, _)| *id == running.project.id) {
                rows.push((running.project.id.clone(), running.project.label.clone()));
            }
        }
        if rows.is_empty() {
            card = card.child(self.control_note(fl!("settings-cloud-none-yet")));
        }
        let busy = images.reset.is_some();
        for (index, (project_id, label)) in rows.into_iter().enumerate() {
            card = card.child(self.sandbox_image_row(index, provider, project_id, label, busy, cx));
        }
        if let Some(message) = &images.message {
            card = card.child(div().min_w_0().child(message.clone()));
        }
        card
    }

    fn sandbox_image_row(
        &self,
        index: usize,
        provider: CloudProvider,
        project_id: String,
        label: String,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let images = &self.sandbox_images;
        let running = images
            .running
            .iter()
            .find(|running| running.provider == provider && running.project.id == project_id);
        let image = images
            .projects
            .iter()
            .find(|entry| entry.project.id == project_id)
            .and_then(|entry| entry.images.iter().find(|image| image.provider == provider));
        let (status, log): (String, Vec<String>) = match (running, image) {
            (Some(running), _) => (
                progress_text(&running.progress),
                running.log.iter().cloned().collect(),
            ),
            (None, Some(image)) => match &image.outcome {
                Outcome::Ready { reference } => (
                    fl!(
                        "settings-cloud-image-ready",
                        image = short(reference),
                        built = ago(image.finished_at)
                    ),
                    Vec::new(),
                ),
                Outcome::Failed { log } => (
                    fl!(
                        "settings-cloud-image-failed",
                        built = ago(image.finished_at)
                    ),
                    log.clone(),
                ),
            },
            (None, None) => (String::new(), Vec::new()),
        };
        let key = (provider, project_id.clone());
        let log_open = images.log.as_ref() == Some(&key);
        let confirming = images.confirm.as_ref() == Some(&key);
        let idle = running.is_none() && !busy;
        let id = project_id.clone();
        let log_button = self
            .cloud_button(
                SharedString::from(format!("cloud-image-log-{index}")),
                if log_open {
                    fl!("settings-cloud-image-hide-log")
                } else {
                    fl!("settings-cloud-image-log")
                },
                !log.is_empty(),
            )
            .debug_selector(move || format!("cloud-image-log-{index}"))
            .when(!log.is_empty(), |button| {
                let key = key.clone();
                button.on_click(cx.listener(move |this, _, _, cx| {
                    let open = &mut this.sandbox_images.log;
                    *open = (open.as_ref() != Some(&key)).then(|| key.clone());
                    cx.notify();
                }))
            });
        let build = self
            .cloud_button(
                SharedString::from(format!("cloud-image-build-{index}")),
                fl!("settings-cloud-image-build"),
                idle,
            )
            .debug_selector(move || format!("cloud-image-build-{index}"))
            .when(idle, |button| {
                let id = id.clone();
                button.on_click(cx.listener(move |this, _, _, cx| {
                    this.build_sandbox_image_now(provider, &id, cx);
                }))
            });
        let reset = self
            .cloud_button(
                SharedString::from(format!("cloud-image-reset-{index}")),
                if confirming {
                    fl!("settings-cloud-image-delete")
                } else {
                    fl!("settings-cloud-image-reset")
                },
                idle && image.is_some(),
            )
            .debug_selector(move || format!("cloud-image-reset-{index}"))
            .when(idle && image.is_some(), |button| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    this.reset_sandbox_image(provider, project_id.clone(), cx);
                }))
            });
        let mut row = div().flex().flex_col().gap(px(6.)).min_w_0().child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().child(label))
                        .child(
                            div()
                                .truncate()
                                .text_color(rgb(self.theme.muted))
                                .child(status),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(log_button)
                        .child(build)
                        .child(reset),
                ),
        );
        if let Some(running) = running {
            let progress = match running.progress.fraction() {
                Some(fraction) => crate::progress::Progress::Working(fraction),
                None => crate::progress::Progress::Busy,
            };
            row = row.child(crate::progress::bar(
                "cloud-image-progress",
                progress,
                crate::menu::accent(&self.theme).into(),
                rgb(self.theme.active).into(),
            ));
        }
        if confirming {
            row = row.child(self.control_note(fl!("settings-cloud-image-reset-confirm")));
        }
        if log_open {
            let shown = log.len().saturating_sub(LOG_SHOWN);
            row = row.child(
                div()
                    .debug_selector(move || format!("cloud-image-log-text-{index}"))
                    .p(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(self.theme.active))
                    .text_font(&self.config.terminal)
                    .text_size(px(self.config.ui.size * 0.85))
                    .children(
                        log[shown..]
                            .iter()
                            .map(|line| div().min_w_0().child(line.clone())),
                    ),
            );
        }
        row
    }
}

/// The part of an image reference worth showing: its tag or short name.
fn short(reference: &str) -> &str {
    reference.rsplit([':', '-']).next().unwrap_or(reference)
}

/// A build's status line: the step, then what the builder said, which is
/// its own output and shown as it is.
fn progress_text(progress: &crate::cloud::image::Progress) -> String {
    match progress.step {
        Some((step, total)) => fl!(
            "settings-cloud-image-step",
            step = step,
            total = total,
            status = progress.status.clone()
        ),
        None => progress.status.clone(),
    }
}

/// When something happened, in words: "just now", "5 min ago".
fn ago(seconds: u64) -> String {
    let elapsed = catalog::now().saturating_sub(seconds);
    match elapsed {
        0..60 => fl!("settings-cloud-ago-now"),
        60..3600 => fl!("settings-cloud-ago-minutes", count = elapsed / 60),
        3600..86_400 => fl!("settings-cloud-ago-hours", count = elapsed / 3600),
        _ => fl!("settings-cloud-ago-days", count = elapsed / 86_400),
    }
}

#[cfg(test)]
mod tests;
