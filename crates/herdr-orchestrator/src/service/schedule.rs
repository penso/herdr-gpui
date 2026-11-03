//! When a source reads GitHub again. Every repository a window holds keeps a
//! worker, and GitHub's GraphQL quota is the account's: a few points a minute
//! per repository add up past it, so reads are spaced by what the user sees
//! and by what can have changed.

use super::{CheckState, Item, PrStatus};
use chrono::{DateTime, Utc};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// How often sources sync while the view is not on screen. The activity
/// bar's counts still move, only more slowly; showing the view syncs at once
/// when the last sync is older than the visible period.
pub(super) const HIDDEN_SYNC: Duration = Duration::from_secs(600);

/// How often checks and reviews are read when no open pull request moved and
/// none has a check running: a review or a re-run check can change without
/// either.
pub(super) const CHECKS_EVERY: Duration = Duration::from_secs(300);

/// The longest repeated failures wait, unless the provider asked for longer.
const MAX_BACKOFF: Duration = Duration::from_secs(1800);

/// The sync period for whether the view is shown.
pub(super) fn period(sync: Duration, visible: bool) -> Duration {
    if visible { sync } else { sync.max(HIDDEN_SYNC) }
}

/// `wait` doubled for each failure in a row after the first, up to
/// [`MAX_BACKOFF`]; never shorter than `wait`, which may carry GitHub's own
/// retry time. A source GitHub keeps failing, such as one whose pages time
/// out, then stops spending a full listing every few minutes.
pub(super) fn backoff(wait: Duration, failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(8);
    wait.saturating_mul(1 << doublings)
        .min(MAX_BACKOFF)
        .max(wait)
}

/// An open pull request as checks care about it: a new head runs new checks,
/// and a review or a request for one moves its update time.
pub(super) type OpenPull = (u64, String, Option<DateTime<Utc>>);

/// The open pull requests of `items`, sorted, to compare between syncs.
pub(super) fn open_pulls(items: &[Item]) -> Vec<OpenPull> {
    let mut open: Vec<OpenPull> = items
        .iter()
        .filter(|item| matches!(item.state.as_str(), "open" | "draft"))
        .filter_map(|item| {
            let pull = item.pull_request.as_ref()?;
            Some((pull.number, pull.head_sha.clone(), item.updated_at))
        })
        .collect();
    open.sort_unstable();
    open
}

/// The last time a source's checks were read, and its open pull requests
/// then.
#[derive(Clone, Debug)]
pub(super) struct ChecksRead {
    pub(super) at: Instant,
    pub(super) open: Vec<OpenPull>,
}

/// Whether `repository`'s checks are read this sync: never read, an open
/// pull request moved, a check of it is still running, or
/// [`CHECKS_EVERY`] passed.
pub(super) fn checks_due(
    last: Option<&ChecksRead>,
    open: &[OpenPull],
    repository: &str,
    statuses: &HashMap<(String, u64), PrStatus>,
    now: Instant,
) -> bool {
    let Some(last) = last else {
        return true;
    };
    let running = statuses.iter().any(|((shown, _), status)| {
        shown == repository && status.checks_state() == Some(CheckState::Pending)
    });
    running || last.open != open || now.duration_since(last.at) >= CHECKS_EVERY
}

#[cfg(test)]
mod tests;
