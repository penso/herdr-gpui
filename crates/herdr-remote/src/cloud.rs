//! Machines cloud providers create and this app uses as devices. What every
//! provider shares lives here: the saved device list, the background jobs that
//! add machines (with their progress and toasts), and the connector that hands
//! a cloud target to its provider. Each provider module owns its API, sign-in,
//! readiness rules, and the command that reaches a machine; supporting a
//! provider adds a `CloudProvider` variant, a module, and a row in
//! [`providers`].

mod catalog;
mod command;
pub mod exec;
// Only containers and Daytona hold images so far.
#[cfg_attr(not(any(feature = "container", feature = "daytona")), allow(dead_code))]
pub mod image;
pub mod install;
pub mod names;
mod providers;
#[cfg(any(feature = "opensandbox", feature = "sprites"))]
pub mod socket;
pub mod unsynced;

pub use catalog::{SavedDevice, load, remove, rename, save};
pub use command::with_command;
pub use herdr_client::CloudProvider;
pub use providers::ProviderError;
pub use providers::{
    Deletion, Shape, Spec, connect, deletion, icon, image as image_support, name, noun, offer,
    provision,
};

use std::{io, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not update {}: {source}", path.display())]
    Catalog {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Invalid saved cloud devices.")]
    Json(#[source] serde_json::Error),
    #[error("{0} is not valid.")]
    Invalid(&'static str),
    #[error("The cloud device is no longer saved.")]
    Missing,
    #[error("No state directory for saved cloud devices.")]
    StateDirectory,
    #[error("The cloud {0} worker stopped.")]
    Worker(&'static str),
    #[error("Could not run the command that reaches the machine.")]
    Process(#[source] io::Error),
    #[error("Cancelled.")]
    Cancelled,
    #[error("Installing Herdr failed: {0}")]
    Install(String),
    #[error("Installing Herdr did not finish within 5 minutes.")]
    InstallTimeout,
    #[error("{0}")]
    Unavailable(&'static str),
}

/// Why cloud machines cannot be used on this system, if they cannot. Each is
/// reached by running Herdr's bridge over a provider command's standard
/// streams, which only the Unix client supports; offering setup elsewhere
/// would save devices that can never connect.
pub fn unavailable() -> Option<&'static str> {
    cfg!(windows).then_some("Cloud devices are unavailable on Windows.")
}

/// What a job reports while it adds a machine, in words every provider shares.
// Railway reports only the steps it has; the rest belong to the others.
#[cfg_attr(
    not(any(
        feature = "azure",
        feature = "boat",
        feature = "coder",
        feature = "daytona"
    )),
    allow(dead_code)
)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Creating,
    /// Reaching a machine that may already exist, such as a Railway VM, whose
    /// first connection creates it when it does not.
    #[cfg_attr(not(feature = "railway"), allow(dead_code))]
    Connecting,
    Starting,
    /// The provider's own word for where the build is, e.g. `pending`.
    Building(String),
    CheckingHerdr,
    /// Building the project's sandbox image first.
    Image(image::Progress),
    InstallingHerdr,
}

impl Step {
    /// About how far along a job at this step is, for a progress bar: a
    /// build's own steps take the most time.
    pub fn progress(&self) -> f32 {
        match self {
            Self::Creating => 0.15,
            Self::Connecting => 0.25,
            Self::Starting | Self::Building(_) => 0.4,
            Self::CheckingHerdr => 0.7,
            // The build is most of a dispatch that needs one.
            Self::Image(progress) => 0.05 + 0.5 * progress.fraction().unwrap_or(0.0),
            Self::InstallingHerdr => 0.85,
        }
    }
}

/// Finish a machine the provider has just made with `finish`, and delete it
/// with `discard` when that fails. A create that fails before the device is
/// saved would otherwise leave the machine billing with no saved device to
/// find it by. A failed delete is logged; `finish`'s error is returned
/// either way.
#[cfg_attr(not(any(feature = "coder", feature = "daytona")), allow(dead_code))]
pub fn discard_on_error<T>(
    provider: CloudProvider,
    id: &str,
    finish: impl FnOnce() -> crate::Result<T>,
    discard: impl FnOnce() -> crate::Result<()>,
) -> crate::Result<T> {
    finish().inspect_err(|_| {
        tracing::info!(
            category = "cloud",
            ?provider,
            id,
            "Deleting a failed create"
        );
        if let Err(error) = discard() {
            tracing::warn!(
                category = "cloud",
                ?provider,
                id,
                %error,
                "Could not delete a failed create; delete it at the provider"
            );
        }
    })
}

#[cfg(test)]
pub(crate) mod tests;
