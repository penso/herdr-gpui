//! Rate limits, as token buckets in memory: a restart resets them, and the
//! keys never reach the disk or the logs.

use std::{
    collections::HashMap,
    hash::Hash,
    net::{IpAddr, Ipv6Addr},
};

/// The most keys one limiter tracks, which bounds its memory.
const MAX_KEYS: usize = 65_536;
const MINUTE_MS: u64 = 60_000;

/// A bucket's size and how fast it refills.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rate {
    pub burst: u32,
    pub per_minute: u32,
}

/// The limits on `/v1/push` and `/v1/revoke`. The daemon's pacer holds a chat
/// to one update per ten seconds, so these only stop what is not a daemon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rates {
    pub device: Rate,
    pub handle: Rate,
    /// Generous: many computers can share one address.
    pub source: Rate,
}

impl Default for Rates {
    fn default() -> Self {
        Self {
            device: Rate {
                burst: 60,
                per_minute: 120,
            },
            handle: Rate {
                burst: 30,
                per_minute: 60,
            },
            source: Rate {
                burst: 120,
                per_minute: 600,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Bucket {
    /// Thousandths of a push, so slow refills are not rounded away.
    milli: u64,
    updated_ms: u64,
}

/// One token bucket per key, up to [`MAX_KEYS`].
#[derive(Debug)]
pub struct Limiter<K> {
    buckets: HashMap<K, Bucket>,
    max_keys: usize,
    rate: Rate,
}

impl<K: Eq + Hash> Limiter<K> {
    #[must_use]
    pub fn new(rate: Rate) -> Self {
        Self::bounded(rate, MAX_KEYS)
    }

    #[must_use]
    pub fn bounded(rate: Rate, max_keys: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            max_keys,
            rate,
        }
    }

    /// Whether `key` may go ahead at `now_ms`, spending one if so. When every
    /// slot is in use by a key that has spent something, a new key is refused
    /// rather than pushing out one that is being limited.
    pub fn allow(&mut self, key: K, now_ms: u64) -> bool {
        let rate = self.rate;
        let full = u64::from(rate.burst) * 1000;
        if self.buckets.len() >= self.max_keys && !self.buckets.contains_key(&key) {
            self.buckets
                .retain(|_, bucket| refilled(*bucket, rate, now_ms) < full);
            if self.buckets.len() >= self.max_keys {
                return false;
            }
        }
        let bucket = self.buckets.entry(key).or_insert(Bucket {
            milli: full,
            updated_ms: now_ms,
        });
        bucket.milli = refilled(*bucket, rate, now_ms);
        bucket.updated_ms = now_ms;
        if bucket.milli < 1000 {
            return false;
        }
        bucket.milli -= 1000;
        true
    }
}

/// `bucket`'s level at `now_ms`. A clock that went back adds nothing.
fn refilled(bucket: Bucket, rate: Rate, now_ms: u64) -> u64 {
    let elapsed = now_ms.saturating_sub(bucket.updated_ms);
    let gained = elapsed.saturating_mul(u64::from(rate.per_minute)) * 1000 / MINUTE_MS;
    bucket
        .milli
        .saturating_add(gained)
        .min(u64::from(rate.burst) * 1000)
}

/// What a source is limited by: its IPv4 address, or its IPv6 /64, since one
/// customer usually holds a whole /64.
#[must_use]
pub fn source_key(address: IpAddr) -> IpAddr {
    match address.to_canonical() {
        IpAddr::V6(address) => {
            let bits = u128::from(address) & !u128::from(u64::MAX);
            IpAddr::V6(Ipv6Addr::from(bits))
        }
        address => address,
    }
}
