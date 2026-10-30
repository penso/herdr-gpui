//! What a machine being made is doing, in words, from the steps
//! `herdr-remote` reports (docs/i18n.md: crates below the app return typed
//! values, and the app writes them out).
// i18n: migrated

use super::{CloudProvider, Step, image::Progress};
use crate::i18n::fl;

/// A job's status before its first step.
pub(crate) fn contacting(provider: CloudProvider) -> String {
    fl!("cloud-status-contacting", provider = super::name(provider))
}

pub(crate) fn step(step: &Step) -> String {
    match step {
        Step::Creating => fl!("cloud-status-creating"),
        Step::Connecting => fl!("cloud-status-connecting"),
        Step::Starting => fl!("cloud-status-starting"),
        Step::Building(status) => fl!("cloud-status-building", status = status.as_str()),
        Step::CheckingHerdr => fl!("cloud-status-checking-herdr"),
        Step::Image(progress) => fl!("cloud-status-image", progress = build(progress)),
        Step::InstallingHerdr => fl!("cloud-status-installing-herdr"),
    }
}

/// A sandbox image build's line: its step when the builder numbers them,
/// then what the builder last said, which stays as it is.
pub(crate) fn build(progress: &Progress) -> String {
    match progress.step {
        Some((step, total)) => fl!(
            "cloud-status-image-step",
            step = step,
            total = total,
            status = progress.status.as_str()
        ),
        None => progress.status.clone(),
    }
}

#[cfg(test)]
mod tests;
