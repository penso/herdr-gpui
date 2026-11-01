//! Remote control's failures in the app, from `error-remote-control.ftl`:
//! its settings and service install (`app::Error`) and the daemon's
//! control socket (`control::Error`), and enrolling a host's daemon
//! (`enroll::Error`). Program names, paths, exit statuses,
//! and the daemon's and commands' own words are placeables.

use super::Localize;
use crate::i18n::fl;
use herdr_remote_control::{app, app::enroll, control};

impl Localize for app::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::CertificatePath { which } => {
                fl!("error-remote-control-certificate-path", key = *which)
            }
            Self::Control(error) => error.localize(),
            Self::Enroll(error) => error.localize(),
            Self::IdleDays => fl!("error-remote-control-idle-days"),
            Self::Io {
                operation, path, ..
            } => fl!(
                "error-remote-control-io",
                operation = *operation,
                path = path.display().to_string()
            ),
            Self::NoStateDir => fl!("error-remote-control-no-state-dir"),
            Self::PairingAddress => fl!("error-remote-control-pairing-address"),
            Self::PairingUrl(_) => fl!("error-remote-control-pairing-url"),
            Self::PathText { path } => fl!(
                "error-remote-control-path-text",
                path = path.display().to_string()
            ),
            Self::Port => fl!("error-remote-control-port"),
            Self::ProgramMissing => fl!("error-remote-control-program-missing"),
            Self::Qr(_) => fl!("error-remote-control-qr"),
            Self::Service {
                program,
                status,
                stderr,
            } => fl!(
                "error-remote-control-service",
                program = *program,
                status = status.to_string(),
                stderr = stderr.as_str()
            ),
            Self::Spawn { program, .. } => fl!("error-remote-control-spawn", program = *program),
            Self::Unexpected => fl!("error-remote-control-unexpected"),
            Self::UnknownUser => fl!("error-remote-control-unknown-user"),
            Self::Unsupported => fl!("error-remote-control-unsupported"),
            // keep-sorted end
        }
    }
}

impl Localize for control::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Connect(_) => fl!("error-remote-control-connect"),
            Self::Daemon { message, .. } => {
                fl!("error-remote-control-daemon", message = message.as_str())
            }
            // The chat protocol's framing errors have no mapping of their
            // own yet, so their English `Display` shows.
            Self::Frame(error) => error.to_string(),
            Self::NoAnswer => fl!("error-remote-control-no-answer"),
            // keep-sorted end
        }
    }
}

impl Localize for enroll::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::ApnsSize => fl!("error-remote-control-enroll-apns-size"),
            Self::Config { path, .. } => fl!(
                "error-remote-control-enroll-config",
                path = path.display().to_string()
            ),
            Self::NoHome => fl!("error-remote-control-enroll-no-home"),
            Self::NotStarted(_) => fl!("error-remote-control-enroll-not-started"),
            Self::Outdated => fl!("error-remote-control-enroll-outdated"),
            Self::Unexpected => fl!("error-remote-control-enroll-unexpected"),
            // keep-sorted end
        }
    }
}
