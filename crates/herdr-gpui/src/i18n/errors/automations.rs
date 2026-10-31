//! Automations: `error-automations.ftl`.

use super::Localize;
use crate::{automations::Error, i18n::fl};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Approved { path, host } => fl!(
                "error-automations-approved",
                path = path.as_str(),
                host = host.as_str()
            ),
            Self::Binary { path, .. } => {
                fl!(
                    "error-automations-binary",
                    path = path.display().to_string()
                )
            }
            Self::Checksum => fl!("error-automations-checksum"),
            Self::Json(_) => fl!("error-automations-json"),
            Self::Missing => fl!("error-automations-missing"),
            Self::NoBuild { platform } => {
                fl!("error-automations-no-build", platform = platform.as_str())
            }
            Self::NoScript { path, host } => fl!(
                "error-automations-no-script",
                path = path.as_str(),
                host = host.as_str()
            ),
            Self::NotRead { host } => fl!("error-automations-not-read", host = host.as_str()),
            Self::Off => fl!("error-automations-off"),
            Self::Platform(output) => {
                fl!("error-automations-platform", output = output.as_str())
            }
            Self::Protocol { found, expected } => fl!(
                "error-automations-protocol",
                found = *found,
                expected = *expected
            ),
            Self::Refused(error) => fl!("error-automations-refused", error = error.as_str()),
            Self::Run(error) => fl!("error-automations-run", error = error.localize()),
            Self::UnknownHost(host) => {
                fl!("error-automations-unknown-host", host = host.as_str())
            }
            Self::UnknownVersion => fl!("error-automations-unknown-version"),
            Self::UnsupportedHost => fl!("error-automations-unsupported-host"),
            // keep-sorted end
        }
    }
}
