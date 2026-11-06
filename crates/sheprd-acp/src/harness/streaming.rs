//! Bounded, lazy publication of live tool snapshots across native bridges.
//!
//! Each adapter validates event identity, bounds its stored value and chooses
//! whether an event appends a delta or replaces an aggregate. This layer only
//! schedules publication: the first change is immediate, subsequent changes
//! share a fixed deadline, and idle bridges have no periodic wakeup.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const CAPACITY: usize = 16;
const ID_BYTES: usize = 1024;
const INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug)]
struct Entry<T> {
    dirty: bool,
    published: bool,
    value: T,
}

#[derive(Debug)]
pub(crate) struct Streams<T> {
    entries: BTreeMap<String, Entry<T>>,
    flush_at: Option<Instant>,
}

impl<T> Default for Streams<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            flush_at: None,
        }
    }
}

impl<T> Streams<T> {
    /// Excess or duplicate streams keep the adapter's ordinary start/final
    /// path. They must not replace a retained stream's state.
    pub(crate) fn insert(&mut self, id: &str, value: T) -> bool {
        if id.is_empty()
            || id.len() > ID_BYTES
            || self.entries.len() >= CAPACITY
            || self.contains(id)
        {
            return false;
        }
        self.entries.insert(
            id.to_owned(),
            Entry {
                dirty: false,
                published: false,
                value,
            },
        );
        true
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }

    /// A terminal event supersedes a queued intermediate update.
    pub(crate) fn remove(&mut self, id: &str) -> Option<T> {
        let value = self.entries.remove(id).map(|entry| entry.value);
        if self.entries.values().all(|entry| !entry.dirty) {
            self.flush_at = None;
        }
        value
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.flush_at = None;
    }

    pub(crate) fn next_tick(&self) -> Option<Instant> {
        self.flush_at
    }

    /// Mutate the bounded source in place. Constructing a full wire update
    /// happens only when publishing, not once for every incoming byte.
    pub(crate) fn update<U>(
        &mut self,
        id: &str,
        now: Instant,
        modify: impl FnOnce(&mut T) -> bool,
        render: impl FnOnce(&str, &T) -> U,
    ) -> Option<U> {
        let entry = self.entries.get_mut(id)?;
        if !modify(&mut entry.value) {
            return None;
        }
        if !entry.published {
            entry.published = true;
            return Some(render(id, &entry.value));
        }
        entry.dirty = true;
        // New chunks never push back an already armed deadline.
        self.flush_at.get_or_insert(now + INTERVAL);
        None
    }

    /// Called at the deadline or before an adapter/transport finishes.
    pub(crate) fn flush<U>(&mut self, mut render: impl FnMut(&str, &T) -> U) -> Vec<U> {
        self.flush_at = None;
        self.entries
            .iter_mut()
            .filter_map(|(id, entry)| {
                std::mem::take(&mut entry.dirty).then(|| render(id, &entry.value))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
