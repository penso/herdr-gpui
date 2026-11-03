//! A newly created worktree can still be running shell initialization when
//! `agent start` arrives. Only a definite busy rejection may be retried: a
//! transport failure or readiness timeout may follow an accepted launch.

use super::{Error, Host, Result, herdr, script};
use serde::Deserialize;
use sheprd_remote::teleport::{Envelope, PaneInfoResult};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const SHELL_START_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const STARTING: &str = "Starting the agent";

/// Called only for the pane returned by this dispatch's worktree creation.
pub(super) fn agent(
    host: &Host,
    args: &[&str],
    pane: &str,
    terminal: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<()> {
    let started = Instant::now();
    retry_busy(
        cancelled,
        || herdr(host, args, STARTING, cancelled).map(drop),
        |budget| {
            let Some(terminal) = terminal.filter(|id| !id.is_empty()) else {
                // Older output without an identity cannot authorize a retry.
                return Ok(false);
            };
            // A stalled host must not hold the dispatch past the retry window.
            let output = host
                .herdr_exec(&["pane", "get", pane], budget, cancelled)
                .map_err(script(STARTING))?;
            let current: Envelope<PaneInfoResult> =
                serde_json::from_slice(&output).map_err(|source| Error::OutputJson {
                    operation: STARTING,
                    source,
                })?;
            Ok(current.result.pane.pane_id == pane
                && current.result.pane.terminal_id.as_deref() == Some(terminal))
        },
        || started.elapsed(),
        std::thread::sleep,
    )
}

fn retry_busy(
    cancelled: &AtomicBool,
    mut attempt: impl FnMut() -> Result<()>,
    // Receives the time left in the retry window, which bounds the query.
    mut same_terminal: impl FnMut(Duration) -> Result<bool>,
    mut elapsed: impl FnMut() -> Duration,
    mut pause: impl FnMut(Duration),
) -> Result<()> {
    loop {
        check_cancelled(cancelled)?;
        let error = match attempt() {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        if !is_busy_rejection(&error) {
            return Err(error);
        }
        check_cancelled(cancelled)?;
        let remaining = SHELL_START_TIMEOUT.saturating_sub(elapsed());
        if remaining.is_zero() {
            return Err(error);
        }
        pause(POLL_INTERVAL.min(remaining));
        check_cancelled(cancelled)?;
        // Public pane IDs may be reused. Never carry a rejected launch onto
        // a different terminal, and never retry after the bounded wait.
        let remaining = SHELL_START_TIMEOUT.saturating_sub(elapsed());
        if remaining.is_zero() || !same_terminal(remaining)? {
            return Err(error);
        }
        check_cancelled(cancelled)?;
        if elapsed() >= SHELL_START_TIMEOUT {
            return Err(error);
        }
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Deserialize)]
struct Rejection {
    id: String,
    error: RejectionBody,
}

#[derive(Deserialize)]
struct RejectionBody {
    code: RejectionCode,
}

#[derive(Deserialize, PartialEq, Eq)]
enum RejectionCode {
    #[serde(rename = "agent_pane_busy")]
    Busy,
    #[serde(other)]
    Other,
}

fn is_busy_rejection(error: &Error) -> bool {
    let Error::Script {
        operation: STARTING,
        source: sheprd_client::Error::ScriptExit { status, stderr },
    } = error
    else {
        return false;
    };
    status.code() == Some(1)
        && serde_json::from_str::<Rejection>(stderr).is_ok_and(|rejection| {
            rejection.id == "cli:agent:start" && rejection.error.code == RejectionCode::Busy
        })
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
