//! `herdr-remote`'s root `Error` from `error-remote.ftl`, and through it
//! each cloud provider's, the shared cloud and sandbox image errors, and
//! Teleport's, one file and one `error-remote-<part>.ftl` each.

#[cfg(feature = "azure")]
mod azure;
#[cfg(feature = "boat")]
mod boat;
#[cfg(feature = "cloud")]
mod cloud;
#[cfg(feature = "coder")]
mod coder;
#[cfg(feature = "container")]
mod container;
#[cfg(feature = "daytona")]
mod daytona;
#[cfg(feature = "opensandbox")]
mod opensandbox;
#[cfg(feature = "railway")]
mod railway;
#[cfg(feature = "sprites")]
mod sprites;
mod teleport;
#[cfg(any(
    feature = "azure",
    feature = "boat",
    feature = "coder",
    feature = "container",
    feature = "daytona",
    feature = "opensandbox",
    feature = "sprites"
))]
mod value;

use super::Localize;
use crate::i18n::fl;
use herdr_remote::Error;

#[cfg(feature = "cloud")]
fn path(path: &std::path::Path) -> String {
    path.display().to_string()
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Client(error) => error.localize(),
            #[cfg(feature = "cloud")]
            Self::Cloud(error) => error.localize(),
            // The app's config loader boxes the app's own error; anything
            // else it boxed is shown as it is.
            Self::Config(error) => error
                .downcast_ref::<crate::Error>()
                .map_or_else(|| error.to_string(), Localize::localize),
            Self::Credentials(error) => error.localize(),
            // The system's own words, as `Json` is its parser's.
            Self::Io(error) => error.to_string(),
            Self::Json(error) => error.to_string(),
            Self::NoConfig => fl!("error-remote-no-config"),
            Self::Path(error) => error.localize(),
            Self::PreferencesPath => fl!("error-remote-preferences-path"),
            #[cfg(feature = "cloud")]
            Self::Provider(error) => error.localize(),
            #[cfg(feature = "cloud")]
            Self::SandboxImage(error) => error.localize(),
            Self::Teleport(error) => error.localize(),
            // keep-sorted end
        }
    }
}
