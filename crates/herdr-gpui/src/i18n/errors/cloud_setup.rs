//! A new cloud machine's setup script: `error-cloud-setup.ftl`.

use super::Localize;
use crate::{cloud_setup::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::NoScript(provider) => {
                fl!("error-cloud-setup-no-script", provider = *provider)
            }
            Self::NoDevice(device) => fl!("error-cloud-setup-no-device", device = device.as_str()),
            Self::ScriptNul => fl!("error-cloud-setup-script-nul"),
            Self::ScriptTooLong { limit } => {
                fl!("error-cloud-setup-script-too-long", limit = *limit)
            }
        }
    }
}
