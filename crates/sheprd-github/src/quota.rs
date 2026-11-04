//! Last reported GraphQL allowance, captured from existing requests rather
//! than spending more requests to measure usage. No credentials are retained.

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, sync::Mutex};

/// GitHub's account-wide GraphQL points, not a count of this app's requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Quota {
    pub limit: u64,
    pub remaining: u64,
    pub used: u64,
    pub reset: Option<DateTime<Utc>>,
}

impl Quota {
    /// A depleted primary quota is paused even if the last request succeeded.
    pub fn retry_after(&self, now: DateTime<Utc>) -> Option<std::time::Duration> {
        if self.remaining != 0 {
            return None;
        }
        (self.reset? - now)
            .to_std()
            .ok()
            .filter(|wait| !wait.is_zero())
    }
}

const CAPACITY: usize = 128;
static QUOTAS: Mutex<VecDeque<([u8; 32], Quota)>> = Mutex::new(VecDeque::new());

fn key(token: &SecretString) -> [u8; 32] {
    Sha256::digest(token.expose_secret().as_bytes()).into()
}

/// An unavailable read must not erase an account's last observation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Observation {
    // keep-sorted start
    Ready(Option<Quota>),
    Unavailable,
    // keep-sorted end
}

/// Nonblocking read used by the account's UI-thread poll.
pub(crate) fn latest(token: &SecretString) -> Observation {
    let key = key(token);
    let Ok(quotas) = QUOTAS.try_lock() else {
        return Observation::Unavailable;
    };
    Observation::Ready(
        quotas
            .iter()
            .find(|(stored, _)| *stored == key)
            .map(|(_, quota)| *quota),
    )
}

pub(crate) fn record(token: &SecretString, headers: &ureq::http::HeaderMap) {
    let Some(quota) = parse(headers) else { return };
    let key = key(token);
    let Ok(mut quotas) = QUOTAS.lock() else {
        return;
    };
    quotas.retain(|(stored, _)| *stored != key);
    if quotas.len() == CAPACITY {
        quotas.pop_front();
    }
    quotas.push_back((key, quota));
}

fn parse(headers: &ureq::http::HeaderMap) -> Option<Quota> {
    let number = |name| headers.get(name)?.to_str().ok()?.parse::<u64>().ok();
    let limit = number("x-ratelimit-limit")?;
    let remaining = number("x-ratelimit-remaining")?;
    let used = number("x-ratelimit-used").unwrap_or(limit.checked_sub(remaining)?);
    if remaining > limit {
        return None;
    }
    Some(Quota {
        limit,
        remaining,
        used,
        reset: number("x-ratelimit-reset")
            .and_then(|at| i64::try_from(at).ok())
            .and_then(|at| DateTime::from_timestamp(at, 0)),
    })
}

#[cfg(test)]
mod tests;
