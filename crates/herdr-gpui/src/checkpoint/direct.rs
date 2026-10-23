//! Checkpoints read and taken in this process, for the command line: the same
//! scripts, refs, and answers the app uses, run on this machine and waited
//! on, so `herdr-gpui checkpoint` and the checkpoints dialog always agree.
//! Blocking: only the command line, never the UI thread, calls these.
//!
//! Each function takes the runner that executes its script, so tests can
//! isolate Git from the user's configuration; [`here`] is the real one.

use super::{Checkout, Checkpoint, Error, KEEP, LIMITS, script};
use crate::Result;
use herdr_client::ScriptLimits;
use std::time::{SystemTime, UNIX_EPOCH};

/// A diff can be far longer than a listing; the caller bounds what it prints.
const DIFF_LIMITS: ScriptLimits = ScriptLimits {
    output: 8 * 1024 * 1024,
    idle: LIMITS.idle,
};

/// Runs a script on the checkout's own host, as the app's workers do.
pub(crate) fn here(checkout: &Checkout, body: &str, limits: ScriptLimits) -> Result<String> {
    super::run_limited(checkout, body, limits)
}

/// [`here`], keeping the output's bytes as they are.
pub(crate) fn here_bytes(checkout: &Checkout, body: &str, limits: ScriptLimits) -> Result<Vec<u8>> {
    super::run_bytes(checkout, body, limits)
}

/// The checkout's checkpoints, newest first.
pub(crate) fn list(
    checkout: &Checkout,
    run: impl FnOnce(&Checkout, &str, ScriptLimits) -> Result<String>,
) -> Result<Vec<Checkpoint>> {
    script::parse_list(&run(checkout, &script::list(checkout), LIMITS)?)
}

/// Take a checkpoint now. `None` when nothing changed since the latest one.
pub(crate) fn create(
    checkout: &Checkout,
    label: &str,
    run: impl FnOnce(&Checkout, &str, ScriptLimits) -> Result<String>,
) -> Result<Option<String>> {
    let body = script::capture(checkout, label, &stamp(), KEEP);
    script::parse_capture(&run(checkout, &body, LIMITS)?)
}

/// The patch from checkpoint `from` to `to`, or to the working tree, as Git
/// printed it: a file's bytes need not be UTF-8.
pub(crate) fn diff(
    checkout: &Checkout,
    from: &str,
    to: Option<&str>,
    run: impl FnOnce(&Checkout, &str, ScriptLimits) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    if !script::valid_id(from) || to.is_some_and(|to| !script::valid_id(to)) {
        return Err(Error::Missing.into());
    }
    run(checkout, &script::diff(checkout, from, to), DIFF_LIMITS)
}

/// Milliseconds since the epoch, zero-padded, as the app stamps them.
fn stamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    format!("{now:016}")
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;
