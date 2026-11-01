//! Pairing a phone with the other machines this app connects to, so the
//! phone reaches each one's own remote control daemon while this computer
//! sleeps (`herdr_remote_control::control::hosts`).
//!
//! This app knows the SSH and cloud machines, and can run scripts on them;
//! its daemon cannot. So the daemon relays: the phone hands it one key per
//! host, and the supervisor takes each request and runs a job here. A job:
//! - copies this app's `herdr-remote-control` into the host's home when the
//!   host lacks this exact build (`crate::host_build`);
//! - runs `herdr-remote-control enroll` there with the phone's key, this
//!   computer's name, and this daemon's APNs key, which starts the host's
//!   daemon when it does not run and adds the phone;
//! - hands the host's answer (its fingerprint and addresses) back to this
//!   daemon, which the phone reads.
//!
//! Revoking the phone here owes each host a revocation, which a job carries
//! out with `herdr-remote-control revoke` once the host can be reached.
//! Everything here blocks; the supervisor runs it on the background
//! executor.

pub(crate) mod ask;
pub(crate) mod install;

use crate::{automations::Machine, host_build, teleport::Host};
use herdr_client::{ConnectTarget, ScriptLimits, shell_quote};
use herdr_remote_control::{
    app::enroll::{Apns, Input},
    control::{
        DeviceId,
        hosts::{self, Enroll, Enrolled, HostOutcome, HostPairing},
    },
};
use std::{sync::atomic::AtomicBool, time::Duration};

/// The daemon's name on a host, and the folder beside the app holding its
/// builds for other platforms.
const PROGRAM: &str = "herdr-remote-control";
/// What `enroll` may print: one JSON answer.
const ENROLL_OUTPUT: u64 = 64 * 1024;
/// `enroll` may wait up to 20 s for a daemon it started.
const ENROLL_IDLE: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The host's answer is not one this app can read")]
    Answer(#[source] serde_json::Error),
    #[error(transparent)]
    Build(#[from] host_build::Error),
    #[error("Could not keep the downloaded build in {}", path.display())]
    Cache {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Could not download this release's herdr-remote-control for {target}")]
    Download {
        target: String,
        #[source]
        source: herdr_updater::Error,
    },
    #[error("Could not run git to send this checkout's source")]
    Git(#[source] std::io::Error),
    #[error("This app no longer connects to that host")]
    Gone,
    #[error("This app has no herdr-remote-control build for {platform}")]
    NoBuild { platform: String },
    #[error(
        "Rust is not installed on that machine; install it with rustup (https://rustup.rs) to build herdr-remote-control there"
    )]
    NoRust,
    #[error("This development build cannot find the checkout it was built from")]
    NoSource,
    #[error("This app has no state directory to keep downloads in")]
    NoStateDir,
    #[error("Could not run herdr-remote-control on the host: {0}")]
    Run(#[source] herdr_client::Error),
    #[error("Remote control cannot be paired on this kind of host")]
    Unsupported,
    // keep-sorted end
}

/// A machine this app connects to: what the daemon calls it, and how to
/// reach it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Known {
    pub(crate) host: hosts::Host,
    pub(crate) target: ConnectTarget,
}

/// The machines a phone may be paired with: every enabled SSH and cloud
/// endpoint, once per machine however many sessions reach it, in the
/// window's order. This computer, WSL, and custom sockets are not hosts of
/// their own.
pub(crate) fn known<'a>(
    endpoints: impl IntoIterator<Item = (&'a str, &'a str, &'a ConnectTarget, bool)>,
) -> Vec<Known> {
    let mut machines = Vec::new();
    let mut known = Vec::new();
    for (id, label, target, enabled) in endpoints {
        let Some(machine) = Machine::of(target).filter(|machine| !machine.is_local()) else {
            continue;
        };
        if !enabled || machines.contains(&machine) {
            continue;
        }
        let host = hosts::Host {
            id: hosts::HostId(id.to_owned()),
            name: crate::notifications::safe_text(label, 64),
        };
        if !host.valid() {
            continue;
        }
        machines.push(machine);
        known.push(Known {
            host,
            target: target.clone(),
        });
    }
    known.truncate(hosts::MAX_HOSTS);
    known
}

/// One phone to pair with one host.
#[derive(Clone, Debug)]
pub(crate) struct Job {
    pub(crate) pairing: HostPairing,
    /// `None` when this app no longer connects to the host.
    pub(crate) target: Option<ConnectTarget>,
    /// This computer's name, kept beside the phone on the host.
    pub(crate) via: String,
    pub(crate) apns: Option<Apns>,
}

/// Runs `job`; how it ended, for the daemon to tell the phone.
pub(crate) fn pair(job: &Job, cancelled: &AtomicBool) -> HostOutcome {
    match enroll(job, cancelled) {
        Ok(enrolled) => HostOutcome::Paired(enrolled),
        Err(error) => {
            tracing::warn!(host = %job.pairing.host.0, %error, "Could not pair a phone with a host");
            HostOutcome::Failed {
                reason: reason(&error),
            }
        }
    }
}

fn enroll(job: &Job, cancelled: &AtomicBool) -> Result<Enrolled, Error> {
    let host = host(job.target.as_ref())?;
    let program = install::ensure(&host, cancelled)?;
    let input = Input {
        enroll: Enroll {
            name: job.pairing.name.clone(),
            public_key: job.pairing.public_key.clone(),
            via: job.via.clone(),
            replace: job.pairing.replace.clone(),
        },
        apns: job.apns.clone(),
    };
    let input = serde_json::to_vec(&input).map_err(Error::Answer)?;
    let mut output = Vec::new();
    host.stream(
        &format!("exec {program} enroll\n"),
        input.as_slice(),
        &mut output,
        ScriptLimits {
            output: ENROLL_OUTPUT,
            idle: ENROLL_IDLE,
        },
        cancelled,
    )
    .map_err(Error::Run)?;
    parse_enrolled(&output)
}

/// The host's answer to `enroll`, refused unless it is in bounds.
pub(crate) fn parse_enrolled(output: &[u8]) -> Result<Enrolled, Error> {
    let enrolled: Enrolled = serde_json::from_slice(output).map_err(Error::Answer)?;
    if !enrolled.valid() {
        return Err(Error::Answer(serde::de::Error::custom(
            "an answer out of bounds", // i18n: data
        )));
    }
    Ok(enrolled)
}

/// Revokes `device` on the host `target` reaches. A host with no
/// `herdr-remote-control` holds no device to revoke.
pub(crate) fn revoke(
    target: Option<&ConnectTarget>,
    device: &DeviceId,
    cancelled: &AtomicBool,
) -> Result<(), Error> {
    let host = host(target)?;
    host.capture(&revoke_script(device), cancelled)
        .map_err(Error::Run)?;
    Ok(())
}

pub(crate) fn revoke_script(device: &DeviceId) -> String {
    format!(
        "program=$(command -v {PROGRAM} || true)\n\
         [ -n \"$program\" ] || exit 0\n\
         exec \"$program\" revoke --device {}\n",
        shell_quote(&device.0)
    )
}

fn host(target: Option<&ConnectTarget>) -> Result<Host, Error> {
    let target = target.ok_or(Error::Gone)?;
    Host::new(target).map_err(|_| Error::Unsupported)
}

/// `error` and its causes, for the phone to show.
fn reason(error: &Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let said = cause.to_string();
        if !text.contains(&said) {
            text.push_str(": ");
            text.push_str(&said);
        }
        source = cause.source();
    }
    crate::notifications::safe_text(&text, hosts::MAX_REASON)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
