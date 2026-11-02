//! Local containers run by Docker or Podman.

use crate::i18n::{errors::Localize, fl};
use herdr_remote::container::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Field(value) => super::value::invalid(*value),
            Self::NoEngine => fl!("error-remote-container-no-engine"),
            Self::Spawn { engine, .. } => fl!("error-remote-container-spawn", engine = *engine),
            Self::Failed {
                engine,
                action,
                detail,
            } => fl!(
                "error-remote-container-failed",
                engine = *engine,
                action = *action,
                detail = detail.as_str()
            ),
            Self::Timeout { engine, action } => fl!(
                "error-remote-container-timeout",
                engine = *engine,
                action = *action
            ),
            Self::Gone(name) => fl!("error-remote-container-gone", name = name.as_str()),
            Self::Suspended(name) => {
                fl!("error-remote-container-suspended", name = name.as_str())
            }
            Self::NameTaken(name) => {
                fl!("error-remote-container-name-taken", name = name.as_str())
            }
            Self::NotOurs(name) => fl!("error-remote-container-not-ours", name = name.as_str()),
            Self::Limit(count) => fl!("error-remote-container-limit", count = *count),
            Self::Cancelled => fl!("error-remote-container-cancelled"),
            Self::BuildContext(_) => fl!("error-remote-container-build-context"),
            Self::ImageNotOurs(image) => {
                fl!(
                    "error-remote-container-image-not-ours",
                    image = image.as_str()
                )
            }
            Self::Bridge(error) => error.localize(),
            Self::Cloud(error) => error.localize(),
            Self::Storage(error) => error.localize(),
        }
    }
}
