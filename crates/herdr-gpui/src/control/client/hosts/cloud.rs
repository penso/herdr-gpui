//! `cloud list` and `cloud images`, read here from the app's saved cloud
//! machines and sandbox image records, whether or not Herdr UI runs. Nothing
//! here creates, starts, or destroys a machine or an image.
use super::Output;
#[cfg(feature = "cloud")]
use serde::Serialize;

/// One saved cloud machine, as the command prints it.
#[cfg(feature = "cloud")]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct Machine {
    pub provider: String,
    pub id: String,
    pub label: String,
    pub account: String,
    pub session: String,
    pub enabled: bool,
}

#[cfg(feature = "cloud")]
impl From<&crate::cloud::SavedDevice> for Machine {
    fn from(device: &crate::cloud::SavedDevice) -> Self {
        Self {
            provider: crate::cloud::name(device.provider).to_owned(),
            id: device.id.clone(),
            label: device.label.clone(),
            account: device.account.clone(),
            session: device.session.clone(),
            enabled: device.enabled,
        }
    }
}

#[cfg(feature = "cloud")]
pub(super) fn text(machines: &[Machine]) -> String {
    if machines.is_empty() {
        return "No cloud machine is saved; `herdr-gpui cloud add` opens the settings that add one"
            .into();
    }
    machines
        .iter()
        .map(|machine| {
            format!(
                "{} {} ({}, {}) session {}{}",
                machine.provider,
                machine.label,
                machine.id,
                machine.account,
                machine.session,
                if machine.enabled { "" } else { ", turned off" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Blocking file read, which the command line may do.
#[cfg(feature = "cloud")]
pub(super) fn list() -> crate::Result<Output> {
    let machines: Vec<Machine> = crate::cloud::load()?.iter().map(Machine::from).collect();
    let output = Output::data(text(&machines), &machines)?;
    Ok(if machines.is_empty() {
        output.nothing()
    } else {
        output
    })
}

/// One project's sandbox image at one provider, as `cloud images` prints it.
#[cfg(feature = "cloud")]
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct Image {
    pub project: String,
    pub provider: String,
    /// `ready` or `failed`.
    pub state: &'static str,
    /// What new machines start from, when ready.
    pub reference: Option<String>,
    pub key: String,
    /// Seconds since the Unix epoch.
    pub finished_at: u64,
    /// Where the recipe was read: the host and checkout.
    pub host: String,
    pub path: String,
}

#[cfg(feature = "cloud")]
fn images_of(entries: &[crate::cloud::image::catalog::Entry]) -> Vec<Image> {
    use crate::cloud::image::catalog::Outcome;
    entries
        .iter()
        .flat_map(|entry| {
            entry.images.iter().map(|image| {
                let (state, reference) = match &image.outcome {
                    Outcome::Ready { reference } => ("ready", Some(reference.clone())),
                    Outcome::Failed { .. } => ("failed", None),
                };
                Image {
                    project: entry.project.label.clone(),
                    provider: crate::cloud::name(image.provider).to_owned(),
                    state,
                    reference,
                    key: image.key.clone(),
                    finished_at: image.finished_at,
                    host: entry.origin.endpoint_id.clone(),
                    path: entry.origin.path.clone(),
                }
            })
        })
        .collect()
}

#[cfg(feature = "cloud")]
pub(super) fn images_text(images: &[Image]) -> String {
    if images.is_empty() {
        return "No sandbox image is built; `herdr-gpui cloud images build` builds the current repository's".into();
    }
    images
        .iter()
        .map(|image| match &image.reference {
            Some(reference) => format!(
                "{} on {}: ready, {reference}",
                image.project, image.provider
            ),
            None => format!(
                "{} on {}: the last build failed; Settings > Cloud devices shows its log",
                image.project, image.provider
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Blocking file read, which the command line may do.
#[cfg(feature = "cloud")]
pub(super) fn images() -> crate::Result<Output> {
    let images = images_of(&crate::cloud::image::catalog::load()?);
    let output = Output::data(images_text(&images), &images)?;
    Ok(if images.is_empty() {
        output.nothing()
    } else {
        output
    })
}

#[cfg(not(feature = "cloud"))]
pub(super) fn images() -> crate::Result<Output> {
    list()
}

#[cfg(not(feature = "cloud"))]
pub(super) fn list() -> crate::Result<Output> {
    Err(crate::Error::Control(crate::control::Error::Rejected {
        code: crate::control::ErrorCode::Unsupported,
        message: "This build has no cloud providers".into(),
    }))
}

#[cfg(all(test, feature = "cloud"))]
mod tests;
