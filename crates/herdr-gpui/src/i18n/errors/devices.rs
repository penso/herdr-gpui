//! Device menu failures, from `error-devices.ftl`.

use super::Localize;
use crate::{i18n::fl, menu::devices::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Adding => fl!("error-devices-adding"),
            Self::Exists(label) => fl!("error-devices-exists", label = label.as_str()),
            Self::InvalidLabel => fl!("error-devices-setup-label"),
            Self::InvalidSession => fl!("error-devices-setup-session"),
            Self::InvalidTarget => fl!("error-devices-setup-target"),
            Self::NonUtf8SetupPath => fl!("error-devices-setup-paths"),
            Self::NoProfile => fl!("error-devices-setup-no-profile"),
            Self::NoStateDirectory => fl!("error-devices-setup-state-directory"),
            Self::Remove { status, detail } if detail.is_empty() => {
                fl!("error-devices-remove", status = status.to_string())
            }
            Self::Remove { status, detail } => fl!(
                "error-devices-remove-detail",
                status = status.to_string(),
                detail = detail.as_str()
            ),
            Self::Rename { status, detail } if detail.is_empty() => {
                fl!("error-devices-rename", status = status.to_string())
            }
            Self::Rename { status, detail } => fl!(
                "error-devices-rename-detail",
                status = status.to_string(),
                detail = detail.as_str()
            ),
            Self::ServerRestart => fl!("error-devices-server-restart"),
            Self::Setup { status, detail } if detail.is_empty() => {
                fl!("error-devices-setup", status = status.to_string())
            }
            Self::Setup { status, detail } => fl!(
                "error-devices-setup-detail",
                status = status.to_string(),
                detail = detail.as_str()
            ),
            Self::SetupTimeout => fl!("error-devices-setup-timeout"),
            Self::TailscaleJson(error) => {
                fl!("error-devices-tailscale-json", error = error.to_string())
            }
            Self::TailscaleTimeout => fl!("error-devices-tailscale-timeout"),
            Self::TerminalSetupUnavailable => fl!("error-devices-setup-windows"),
        }
    }
}
