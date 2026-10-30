//! Device menu failures, from `error-devices.ftl`.

use super::Localize;
use crate::{i18n::fl, menu::devices::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Adding => fl!("error-devices-adding"),
            Self::Exists(label) => fl!("error-devices-exists", label = label.as_str()),
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
            Self::SetupInput(reason) => setup_input(reason),
            Self::SetupTimeout => fl!("error-devices-setup-timeout"),
            Self::TailscaleJson(error) => {
                fl!("error-devices-tailscale-json", error = error.to_string())
            }
            Self::TailscaleTimeout => fl!("error-devices-tailscale-timeout"),
        }
    }
}

/// `SetupInput` carries one of a few fixed English sentences written in
/// `menu::devices::setup`, matched here as data; each has its own message.
/// A sentence added there without one here shows in English until it gets
/// one.
fn setup_input(reason: &'static str) -> String {
    match reason {
        // i18n: data
        "This device has no saved profile." => fl!("error-devices-setup-no-profile"),
        // i18n: data
        "Device names are at most 128 bytes, without control characters." => {
            fl!("error-devices-setup-label")
        }
        // i18n: data
        "Enter an SSH target such as user@hostname or an SSH alias." => {
            fl!("error-devices-setup-target")
        }
        // i18n: data
        "Session names use letters, digits, dots, underscores, or hyphens (at most 64 bytes), excluding '.' and '..'." =>
        {
            fl!("error-devices-setup-session")
        }
        // i18n: data
        "Herdr UI has no state directory." => fl!("error-devices-setup-state-directory"),
        // i18n: data
        "The setup paths must be UTF-8." => fl!("error-devices-setup-paths"),
        // i18n: data
        "Setting up a device in a terminal is unavailable on Windows." => {
            fl!("error-devices-setup-windows")
        }
        other => other.to_owned(),
    }
}
