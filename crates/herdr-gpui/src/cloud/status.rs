//! What a machine being made is doing, in words, from the steps
//! `herdr-remote` reports (docs/i18n.md: crates below the app return typed
//! values, and the app writes them out).
// i18n: migrated

use super::{CloudProvider, Step, image::Progress};
use crate::i18n::{self, FluentValue, fl};

/// A job's status before its first step.
pub(crate) fn contacting(provider: CloudProvider) -> String {
    fl!("cloud-status-contacting", provider = super::name(provider))
}

/// `step` in the current locale, for the window.
pub(crate) fn step(step: &Step) -> String {
    say(step, i18n::message)
}

/// `step` in English, for command line output such as a batch's progress.
pub(crate) fn english(step: &Step) -> String {
    say(step, i18n::english)
}

fn say(step: &Step, say: impl Fn(&str, &[(&str, FluentValue<'_>)]) -> String) -> String {
    match step {
        Step::Creating => say("cloud-status-creating", &[]),
        Step::Connecting => say("cloud-status-connecting", &[]),
        Step::Starting => say("cloud-status-starting", &[]),
        Step::Building(status) => say(
            "cloud-status-building",
            &[("status", FluentValue::from(status.as_str()))],
        ),
        Step::CheckingHerdr => say("cloud-status-checking-herdr", &[]),
        Step::Image(progress) => say(
            "cloud-status-image",
            &[("progress", FluentValue::from(built(progress, &say)))],
        ),
        Step::InstallingHerdr => say("cloud-status-installing-herdr", &[]),
    }
}

/// A sandbox image build's line: its step when the builder numbers them,
/// then what the builder last said, which stays as it is.
pub(crate) fn build(progress: &Progress) -> String {
    built(progress, i18n::message)
}

fn built(progress: &Progress, say: impl Fn(&str, &[(&str, FluentValue<'_>)]) -> String) -> String {
    match progress.step {
        Some((step, total)) => say(
            "cloud-status-image-step",
            &[
                ("step", FluentValue::from(step)),
                ("total", FluentValue::from(total)),
                ("status", FluentValue::from(progress.status.as_str())),
            ],
        ),
        None => progress.status.clone(),
    }
}

#[cfg(test)]
mod tests;
