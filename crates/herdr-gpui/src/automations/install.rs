//! Enabling automations on a host that has no `herdr-automate`, or
//! updating one that runs another version than this app ships: copy the
//! build for the host's platform into the user's home, link it onto
//! `PATH`, and start the daemon. Nothing needs root and nothing is written
//! outside `$HOME`:
//!
//! - `~/.local/share/herdr-automate/bin/VERSION/herdr-automate`, the build;
//! - `~/.local/bin/herdr-automate`, a link to it;
//! - a user service from `herdr-automate install-service` (a systemd user
//!   unit or a launchd agent), when enabling.
//!
//! The bytes go through the host script's stdin and are checked against
//! their SHA-256 on the host before they are moved into place.

use super::{Error, fetch};
use crate::{host_build, teleport::Host};
use herdr_client::ScriptLimits;
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

/// Where builds for other platforms ship, beside the app's executable.
const BUILDS: &str = "automate";
/// A slow link can take a while to carry a few megabytes.
const COPY_IDLE: Duration = Duration::from_secs(300);
/// What the install script itself may print.
const COPY_OUTPUT: u64 = 64 * 1024;
/// `shutdown` waits up to about 40 s for runs to finish before it answers.
const SHUTDOWN_IDLE: Duration = Duration::from_secs(90);
pub(crate) use crate::host_build::Platform;

/// What enabling copies, shown before the user confirms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) platform: Platform,
    pub(crate) source: PathBuf,
    pub(crate) version: String,
    pub(crate) size: u64,
    pub(crate) sha256: String,
}

/// The build for `platform` this app ships: its own `herdr-automate` when
/// the host is this kind of computer, else `automate/TRIPLE/herdr-automate`
/// beside the executable.
pub(crate) fn build_for(
    platform: &Platform,
    beside: &Path,
    bundled: Option<PathBuf>,
) -> Option<PathBuf> {
    host_build::build_for(platform, beside, BUILDS, fetch::PROGRAM, bundled)
}

/// Finds out what enabling `host` would copy. Blocks.
pub(crate) fn prepare(
    host: &Host,
    shipped: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<Plan, Error> {
    // Builds for every platform ship together, so each has the version the
    // app's own `herdr-automate --version` printed; it names the directory.
    let version = shipped.ok_or(Error::UnknownVersion)?.to_owned();
    let uname = host.capture("uname -sm\n", cancelled).map_err(Error::Run)?;
    let platform = Platform::parse(&String::from_utf8_lossy(&uname))?;
    let source =
        build_for(&platform, &host_build::beside_app(), fetch::bundled()).ok_or_else(|| {
            Error::NoBuild {
                platform: platform.to_string(),
            }
        })?;
    let (size, sha256) = digest(&source)?;
    Ok(Plan {
        platform,
        source,
        version,
        size,
        sha256,
    })
}

/// The file's size and SHA-256, read in pieces.
pub(crate) fn digest(path: &Path) -> Result<(u64, String), Error> {
    Ok(host_build::digest(fetch::PROGRAM, path)?)
}

/// The host script that receives the build on stdin and puts it in place.
pub(crate) fn script(plan: &Plan) -> String {
    host_build::copy_script(fetch::PROGRAM, &plan.version, &plan.sha256)
}

/// Copies the build and starts the daemon; what the host said. Blocks.
pub(crate) fn install(
    host: &Host,
    plan: &Plan,
    update: bool,
    cancelled: &AtomicBool,
) -> Result<String, Error> {
    let file = std::fs::File::open(&plan.source).map_err(|source| host_build::Error::Binary {
        program: fetch::PROGRAM,
        path: plan.source.clone(),
        source,
    })?;
    let mut output = Vec::new();
    host.stream(
        &script(plan),
        file,
        &mut output,
        ScriptLimits {
            output: COPY_OUTPUT,
            idle: COPY_IDLE,
        },
        cancelled,
    )
    .map_err(|error| match &error {
        herdr_client::Error::ScriptExit { status, .. }
            if status.code() == Some(host_build::CHECKSUM_EXIT) =>
        {
            host_build::Error::Checksum.into()
        }
        _ => Error::Run(error),
    })?;
    if update {
        // `start` does nothing while a daemon holds the lock: stop the old
        // one first. It lets its runs finish, which can take a while.
        fetch::act_within(host, &["shutdown"], SHUTDOWN_IDLE, cancelled)?;
    }
    // On an update too: the service names the executable it runs, and the
    // one written before names the old version.
    fetch::act(host, &["install-service"], cancelled)?;
    let started = fetch::act(host, &["start"], cancelled)?;
    Ok(started.message)
}
