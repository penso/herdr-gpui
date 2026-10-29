//! Destroying an Azure device from Settings: confirm on the device's row, then
//! delete the VM or sandbox at Azure, with its disk and public IP, on a
//! worker of its own, with a progress bar until Azure reports it gone.
//! Remove, beside it, only forgets the device and leaves it billing.
//!
//! The destroy has its own worker so the card's refreshes, which replace the
//! card's job, never cancel it. Closing Settings does cancel it; the device
//! stays saved until Azure has deleted it, so Destroy can simply run again.

use super::{super::super::SettingsWindow, Account, AzureCard, machine_kind};
use crate::{
    azure::{destroy::Stage, machine::Machine},
    cloud::{
        SavedDevice,
        worker::{self, Worker},
    },
    i18n::fl,
    progress::{self, Progress},
};
use gpui::{prelude::*, *};

pub(super) enum Update {
    Stage(Stage),
    Done(crate::azure::Result<()>),
}

/// The one destroy that may run at a time.
pub(in crate::settings_window) struct Destroying {
    pub(super) id: String,
    pub(super) label: String,
    /// `None` until the worker starts.
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
            fl!("settings-cloud-azure-destroy-contacting"),
            Progress::Busy,
        ),
        Some(Stage::Vm) => (
            fl!("settings-cloud-azure-destroy-vm"),
            Progress::Working(0.3),
        ),
        Some(Stage::Leftovers) => (
            fl!("settings-cloud-azure-destroy-leftovers"),
            Progress::Working(0.7),
        ),
        Some(Stage::Sandbox) => (
            fl!("settings-cloud-azure-destroy-sandbox"),
            Progress::Working(0.5),
        ),
    }
}

impl AzureCard {
    /// Destroying needs a resolved account and no other destroy running.
    pub(super) fn can_destroy(&self) -> bool {
        self.destroying.is_none() && matches!(self.account, Account::Known { .. })
    }

    fn destroying(&self, id: &str) -> bool {
        self.destroying.as_ref().is_some_and(|job| job.id == id)
    }
}

impl SettingsWindow {
    fn ask_azure_destroy(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.azure_card {
            card.confirm_destroy = Some(id);
            card.outcome = None;
            cx.notify();
        }
    }

    fn cancel_azure_destroy(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.azure_card {
            card.confirm_destroy = None;
            cx.notify();
        }
    }

    pub(super) fn destroy_azure_device(&mut self, device: &SavedDevice, cx: &mut Context<Self>) {
        let Some(settings) = self.azure_settings() else {
            return;
        };
        let Some(card) = &mut self.azure_card else {
            return;
        };
        if !card.can_destroy() || card.confirm_destroy.as_deref() != Some(&device.id) {
            return;
        }
        card.confirm_destroy = None;
        card.outcome = None;
        let saved = device.clone();
        let spawned = worker::spawn(
            "herdr-azure-destroy",
            cx,
            move |cancelled, send| {
                let result =
                    crate::azure::destroy::destroy_device(&settings, &saved, cancelled, |stage| {
                        send(Update::Stage(stage))
                    });
                send(Update::Done(result));
            },
            |this: &mut Self, update, cx| this.apply_azure_destroy(update, cx),
        );
        let Some(card) = &mut self.azure_card else {
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
                tracing::error!(category = "azure_worker", error_kind = ?error.kind(), "Could not start Azure destroy worker");
                card.outcome = Some(Outcome::Failed(
                    crate::cloud::Error::Worker("destroy").to_string(), // i18n: data
                ));
            }
        }
        cx.notify();
    }

    pub(super) fn apply_azure_destroy(&mut self, update: Update, cx: &mut Context<Self>) {
        // The main window pulses the device's header while it is destroyed,
        // and stops once it is gone or the destroy failed.
        let _ = self.source.update(cx, |_, cx| cx.notify());
        let Some(card) = &mut self.azure_card else {
            return;
        };
        let Some(job) = &mut card.destroying else {
            return;
        };
        match update {
            Update::Stage(stage) => {
                job.stage = Some(stage);
                cx.notify();
            }
            Update::Done(result) => {
                let label = job.label.clone();
                card.destroying = None;
                card.outcome = Some(match result {
                    Ok(()) => Outcome::Destroyed(label),
                    Err(error) => Outcome::Failed(fl!(
                        "settings-cloud-delete-failed",
                        name = label,
                        error = error.to_string()
                    )),
                });
                self.refresh_azure(cx);
            }
        }
    }

    /// The row's Remove… button, or the confirmation: deleting at Azure is
    /// the default, with removing from Herdr only beside it.
    pub(super) fn azure_destroy_actions(
        &self,
        index: usize,
        device: &SavedDevice,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(card) = &self.azure_card else {
            return Vec::new();
        };
        let danger = crate::menu::danger(&self.theme);
        let enabled = ready && card.can_destroy();
        if card.confirm_destroy.as_deref() == Some(&device.id) {
            let saved = device.clone();
            let forgotten = device.id.clone();
            let kind = machine_kind(&device.id);
            return vec![
                self.cloud_button(
                    SharedString::from(format!("cloud-azure-remove-{index}")),
                    fl!("settings-cloud-remove-from-herdr"),
                    ready,
                )
                .debug_selector(move || format!("cloud-azure-remove-{index}"))
                .when(ready, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.cancel_azure_destroy(cx);
                        this.forget_azure_device(forgotten.clone(), cx);
                    }))
                })
                .into_any_element(),
                self.cloud_button(
                    SharedString::from(format!("cloud-azure-destroy-cancel-{index}")),
                    fl!("settings-cloud-cancel"),
                    true,
                )
                .debug_selector(move || format!("cloud-azure-destroy-cancel-{index}"))
                .on_click(cx.listener(|this, _, _, cx| this.cancel_azure_destroy(cx)))
                .into_any_element(),
                self.cloud_button(
                    SharedString::from(format!("cloud-azure-destroy-confirm-{index}")),
                    fl!("settings-cloud-azure-delete", kind = kind),
                    enabled,
                )
                .text_color(danger)
                .debug_selector(move || format!("cloud-azure-destroy-confirm-{index}"))
                .when(enabled, |button| {
                    button.on_click(
                        cx.listener(move |this, _, _, cx| this.destroy_azure_device(&saved, cx)),
                    )
                })
                .into_any_element(),
            ];
        }
        let id = device.id.clone();
        vec![
            self.cloud_button(
                SharedString::from(format!("cloud-azure-destroy-{index}")),
                fl!("settings-cloud-remove-ask"),
                enabled,
            )
            .text_color(danger)
            .debug_selector(move || format!("cloud-azure-destroy-{index}"))
            .when(enabled, |button| {
                button.on_click(
                    cx.listener(move |this, _, _, cx| this.ask_azure_destroy(id.clone(), cx)),
                )
            })
            .into_any_element(),
        ]
    }

    /// The warning under a row awaiting confirmation, or the progress of the
    /// destroy running on it.
    pub(super) fn azure_destroy_detail(&self, device: &SavedDevice) -> Option<Div> {
        let card = self.azure_card.as_ref()?;
        if card.confirm_destroy.as_deref() == Some(&device.id) {
            let warning = match Machine::parse(&device.id) {
                Machine::Vm(name) => fl!("settings-cloud-azure-delete-vm-warning", name = name),
                Machine::Sandbox(_) => fl!(
                    "settings-cloud-azure-delete-sandbox-warning",
                    name = device.label.clone()
                ),
            };
            return Some(div().child(self.control_note(warning)));
        }
        let job = card.destroying.as_ref().filter(|job| job.id == device.id)?;
        let (text, progress) = status(job.stage);
        Some(
            div()
                .debug_selector(|| "cloud-azure-destroying".into())
                .flex()
                .flex_col()
                .gap(px(6.))
                .min_w_0()
                .child(progress::bar(
                    "cloud-azure-destroy-progress",
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
    pub(super) fn azure_removable(&self, device: &SavedDevice) -> bool {
        self.azure_card
            .as_ref()
            .is_some_and(|card| !card.destroying(&device.id))
    }

    pub(super) fn azure_destroy_outcome(&self) -> Option<Div> {
        let outcome = self.azure_card.as_ref()?.outcome.as_ref()?;
        Some(match outcome {
            Outcome::Destroyed(label) => div()
                .debug_selector(|| "cloud-azure-destroyed".into())
                .child(fl!("settings-cloud-deleted", name = label.clone())),
            Outcome::Failed(message) => div()
                .debug_selector(|| "cloud-azure-destroy-failed".into())
                .text_color(crate::menu::danger(&self.theme))
                .child(message.clone()),
        })
    }
}
