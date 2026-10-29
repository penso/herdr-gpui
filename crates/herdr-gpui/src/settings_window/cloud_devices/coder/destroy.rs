//! Destroying a Coder device from Settings: confirm on the device's row, then
//! delete its workspace at Coder on a worker of its own, with a progress bar
//! until Coder reports it gone. Remove, beside it, only forgets the device.
//!
//! The destroy has its own worker so the card's refreshes, which replace the
//! card's job, never cancel it. Closing Settings does cancel it; the device
//! stays saved until the workspace is gone, so Destroy can simply run again.

use super::{super::super::SettingsWindow, Account, CoderCard};
use crate::{
    cloud::{
        SavedDevice,
        worker::{self, Worker},
    },
    coder::setup::{self, Session, Stage},
    i18n::fl,
    progress::{self, Progress},
};
use gpui::{prelude::*, *};

pub(super) enum Update {
    Stage(Stage),
    Done(crate::coder::Result<()>),
}

/// The one destroy that may run at a time.
pub(in crate::settings_window) struct Destroying {
    pub(super) id: String,
    pub(super) label: String,
    /// `None` until Coder first answers.
    pub(super) stage: Option<Stage>,
    pub(super) _job: Worker,
}

/// How the last destroy ended, shown above the device list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Destroyed(String),
    Failed(String),
}

pub(super) fn status(stage: Option<Stage>) -> (String, Progress) {
    match stage {
        None => (
            fl!("settings-cloud-coder-destroy-contacting"),
            Progress::Busy,
        ),
        Some(Stage::Waiting) => (
            fl!("settings-cloud-coder-destroy-waiting"),
            Progress::Working(0.15),
        ),
        Some(Stage::Queued) => (
            fl!("settings-cloud-coder-destroy-queued"),
            Progress::Working(0.4),
        ),
        Some(Stage::Deleting) => (
            fl!("settings-cloud-coder-destroy-deleting"),
            Progress::Working(0.7),
        ),
    }
}

impl CoderCard {
    /// Destroying needs a working sign-in and no other destroy running.
    pub(super) fn can_destroy(&self) -> bool {
        self.destroying.is_none()
            && matches!(
                self.account,
                Account::Known(Session::SignedIn { .. } | Session::Unreadable { .. })
            )
    }

    fn destroying(&self, id: &str) -> bool {
        self.destroying.as_ref().is_some_and(|job| job.id == id)
    }
}

impl SettingsWindow {
    fn ask_destroy(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.coder_card {
            card.confirm_destroy = Some(id);
            card.outcome = None;
            cx.notify();
        }
    }

    fn cancel_destroy(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.coder_card {
            card.confirm_destroy = None;
            cx.notify();
        }
    }

    pub(super) fn destroy_coder_device(&mut self, device: &SavedDevice, cx: &mut Context<Self>) {
        let Some(settings) = self.coder_settings() else {
            return;
        };
        let Some(card) = &mut self.coder_card else {
            return;
        };
        if !card.can_destroy() || card.confirm_destroy.as_deref() != Some(&device.id) {
            return;
        }
        card.confirm_destroy = None;
        card.outcome = None;
        let id = device.id.clone();
        let spawned = worker::spawn(
            "herdr-coder-destroy",
            cx,
            move |cancelled, send| {
                let result = setup::destroy_device(&settings, &id, cancelled, |stage| {
                    send(Update::Stage(stage))
                });
                send(Update::Done(result));
            },
            |this: &mut Self, update, cx| this.apply_destroy(update, cx),
        );
        let Some(card) = &mut self.coder_card else {
            return;
        };
        match spawned {
            Ok(job) => {
                card.destroying = Some(Destroying {
                    id: device.id.clone(),
                    label: device.label.clone(),
                    stage: None,
                    _job: job,
                });
            }
            Err(error) => {
                tracing::error!(category = "coder_worker", error_kind = ?error.kind(), "Could not start Coder destroy worker");
                card.outcome = Some(Outcome::Failed(
                    crate::coder::Error::Worker("destroy").to_string(), // i18n: data
                ));
            }
        }
        cx.notify();
    }

    pub(super) fn apply_destroy(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(card) = &mut self.coder_card else {
            return;
        };
        let Some(job) = &mut card.destroying else {
            return;
        };
        match update {
            Update::Stage(stage) => job.stage = Some(stage),
            Update::Done(result) => {
                let label = job.label.clone();
                card.destroying = None;
                card.outcome = Some(match result {
                    Ok(()) => Outcome::Destroyed(label),
                    Err(error) => Outcome::Failed(fl!(
                        "settings-cloud-coder-destroy-failed",
                        name = label,
                        error = error.to_string()
                    )),
                });
                self.refresh_cloud(cx);
            }
        }
    }

    /// The row's Destroy… button, or Cancel and the confirming button.
    pub(super) fn coder_destroy_actions(
        &self,
        index: usize,
        device: &SavedDevice,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(card) = &self.coder_card else {
            return Vec::new();
        };
        let danger = crate::menu::danger(&self.theme);
        let enabled = ready && card.can_destroy();
        if card.confirm_destroy.as_deref() == Some(&device.id) {
            let saved = device.clone();
            return vec![
                self.cloud_button(
                    SharedString::from(format!("cloud-coder-destroy-cancel-{index}")),
                    fl!("settings-cloud-cancel"),
                    true,
                )
                .debug_selector(move || format!("cloud-coder-destroy-cancel-{index}"))
                .on_click(cx.listener(|this, _, _, cx| this.cancel_destroy(cx)))
                .into_any_element(),
                self.cloud_button(
                    SharedString::from(format!("cloud-coder-destroy-confirm-{index}")),
                    fl!("settings-cloud-coder-destroy-confirm"),
                    enabled,
                )
                .text_color(danger)
                .debug_selector(move || format!("cloud-coder-destroy-confirm-{index}"))
                .when(enabled, |button| {
                    button.on_click(
                        cx.listener(move |this, _, _, cx| this.destroy_coder_device(&saved, cx)),
                    )
                })
                .into_any_element(),
            ];
        }
        let id = device.id.clone();
        vec![
            self.cloud_button(
                SharedString::from(format!("cloud-coder-destroy-{index}")),
                fl!("settings-cloud-coder-destroy"),
                enabled,
            )
            .text_color(danger)
            .debug_selector(move || format!("cloud-coder-destroy-{index}"))
            .when(enabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| this.ask_destroy(id.clone(), cx)))
            })
            .into_any_element(),
        ]
    }

    /// The warning under a row awaiting confirmation, or the progress of the
    /// destroy running on it.
    pub(super) fn coder_destroy_detail(&self, device: &SavedDevice) -> Option<Div> {
        let card = self.coder_card.as_ref()?;
        if card.confirm_destroy.as_deref() == Some(&device.id) {
            return Some(div().child(self.control_note(fl!(
                "settings-cloud-coder-destroy-warning",
                workspace = device.machine.clone()
            ))));
        }
        let job = card.destroying.as_ref().filter(|job| job.id == device.id)?;
        let (text, progress) = status(job.stage);
        Some(
            div()
                .debug_selector(|| "cloud-coder-destroying".into())
                .flex()
                .flex_col()
                .gap(px(6.))
                .min_w_0()
                .child(progress::bar(
                    "cloud-coder-destroy-progress",
                    progress,
                    crate::menu::danger(&self.theme).into(),
                    rgb(self.theme.active).into(),
                ))
                .child(
                    div()
                        .truncate()
                        .text_color(rgb(self.theme.muted))
                        .child(text),
                ),
        )
    }

    /// Whether Remove stays offered on this row: never while it is destroyed.
    pub(super) fn coder_removable(&self, device: &SavedDevice) -> bool {
        self.coder_card
            .as_ref()
            .is_some_and(|card| !card.destroying(&device.id))
    }

    pub(super) fn coder_destroy_outcome(&self) -> Option<Div> {
        let outcome = self.coder_card.as_ref()?.outcome.as_ref()?;
        Some(match outcome {
            Outcome::Destroyed(label) => div()
                .debug_selector(|| "cloud-coder-destroyed".into())
                .child(fl!("settings-cloud-coder-destroyed", name = label.clone())),
            Outcome::Failed(message) => div()
                .debug_selector(|| "cloud-coder-destroy-failed".into())
                .text_color(crate::menu::danger(&self.theme))
                .child(message.clone()),
        })
    }
}
