//! Per-key token buckets, used per client address for pairing claims and
//! challenges, with a bounded number of keys.

use std::{collections::HashMap, hash::Hash};

#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: u32,
    /// When `tokens` was last refilled.
    refilled_ms: u64,
    /// When the key last asked, for eviction.
    used_ms: u64,
}

/// Lets each key spend up to `capacity` requests at once, earning one back
/// every `refill_ms`. It remembers at most `max_keys` keys: a full bucket
/// is forgotten first (it is the same as none), then the least recently
/// used.
#[derive(Debug)]
pub(crate) struct RateLimiter<K> {
    buckets: HashMap<K, Bucket>,
    capacity: u32,
    max_keys: usize,
    refill_ms: u64,
}

impl<K: Eq + Hash + Clone> RateLimiter<K> {
    pub(crate) fn new(capacity: u32, refill_ms: u64, max_keys: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            capacity: capacity.max(1),
            max_keys: max_keys.max(1),
            refill_ms: refill_ms.max(1),
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.buckets.len()
    }

    /// Spends one request for `key`; `false` when its bucket is empty.
    pub(crate) fn allow(&mut self, key: K, now_ms: u64) -> bool {
        if !self.buckets.contains_key(&key) && self.buckets.len() >= self.max_keys {
            self.evict(now_ms);
        }
        let (capacity, refill_ms) = (self.capacity, self.refill_ms);
        let bucket = self.buckets.entry(key).or_insert(Bucket {
            tokens: capacity,
            refilled_ms: now_ms,
            used_ms: now_ms,
        });
        refill(bucket, capacity, refill_ms, now_ms);
        bucket.used_ms = bucket.used_ms.max(now_ms);
        if bucket.tokens == 0 {
            return false;
        }
        bucket.tokens -= 1;
        true
    }

    fn evict(&mut self, now_ms: u64) {
        let (capacity, refill_ms) = (self.capacity, self.refill_ms);
        self.buckets.retain(|_, bucket| {
            refill(bucket, capacity, refill_ms, now_ms);
            bucket.tokens < capacity
        });
        if self.buckets.len() < self.max_keys {
            return;
        }
        if let Some(oldest) = self
            .buckets
            .iter()
            .min_by_key(|(_, bucket)| bucket.used_ms)
            .map(|(key, _)| key.clone())
        {
            self.buckets.remove(&oldest);
        }
    }
}

fn refill(bucket: &mut Bucket, capacity: u32, refill_ms: u64, now_ms: u64) {
    let earned = now_ms.saturating_sub(bucket.refilled_ms) / refill_ms;
    if earned == 0 {
        return;
    }
    let tokens = u64::from(bucket.tokens).saturating_add(earned);
    bucket.tokens = u32::try_from(tokens).unwrap_or(u32::MAX).min(capacity);
    bucket.refilled_ms = if bucket.tokens == capacity {
        now_ms
    } else {
        bucket.refilled_ms + earned * refill_ms
    };
}
