//! GitHub's rate limits as both the updater and the GitHub client read them:
//! whether a refusal is an exhausted quota, when it refills, and how to say
//! when to retry. A crate of its own so the GitHub client needs neither the
//! updater nor the updater the client; no I/O, so nothing here fails.

use chrono::{DateTime, DurationRound as _, Local, TimeDelta, Utc};
use http::HeaderMap;

/// The first whole minute at or after `reset`, so retrying at the shown minute
/// is never still early, even by a fraction of a second.
pub fn retry_minute(reset: DateTime<Utc>) -> DateTime<Utc> {
    // Rounding fails only for times chrono cannot represent a minute later.
    reset
        .duration_round_up(TimeDelta::minutes(1))
        .unwrap_or(reset)
}

/// When to try again, in local time, or `Retry later` without a reset time.
pub fn retry_hint(reset: Option<DateTime<Utc>>) -> String {
    match reset {
        Some(reset) => format!(
            "Retry after {}",
            retry_minute(reset).with_timezone(&Local).format("%H:%M")
        ),
        None => "Retry later".to_owned(),
    }
}

/// Whether a 403 is an exhausted quota (primary or secondary) rather than a
/// permission refusal: GitHub then sends `x-ratelimit-remaining: 0` or a
/// `retry-after` delay.
pub fn quota_exhausted(headers: &HeaderMap) -> bool {
    header_number(headers, "x-ratelimit-remaining") == Some(0)
        || headers.contains_key("retry-after")
}

/// When GitHub's quota refills: a `retry-after` delay in seconds, else the
/// `x-ratelimit-reset` epoch. `None` without either, or once it has passed.
pub fn rate_limit_reset(headers: &HeaderMap, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    header_number(headers, "retry-after")
        .and_then(|seconds| now.checked_add_signed(TimeDelta::try_seconds(seconds)?))
        .or_else(|| {
            header_number(headers, "x-ratelimit-reset")
                .and_then(|epoch| DateTime::from_timestamp(epoch, 0))
        })
        .filter(|reset| *reset > now)
}

fn header_number(headers: &HeaderMap, name: &str) -> Option<i64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(test)]
mod tests;
