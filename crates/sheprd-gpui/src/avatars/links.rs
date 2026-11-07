//! Which saved SSH devices are connected, so remote avatar reads follow the
//! window's own reconnection rather than probing a host it knows is down.
use super::{Avatars, Checkout, Slot};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

/// Hosts with no live connection, shared with the SSH readers so a read
/// queued or running for one stops instead of waiting out its timeout.
pub(super) type Offline = Arc<Mutex<HashSet<String>>>;

pub(super) fn is_offline(offline: &Offline, target: &str) -> bool {
    offline.lock().is_ok_and(|hosts| hosts.contains(target))
}

impl Avatars {
    /// Follow one saved SSH device's connection: `online` is when its current
    /// connection came up with a snapshot, `None` while it is down. While no
    /// device on a host is connected nothing is read there, and reads queued
    /// or running for it stop. Each new connection makes the host's unread
    /// checkouts due at once, so the snapshot it brings asks for them again.
    ///
    /// Called on every poll; it allocates and locks only on a change.
    pub(crate) fn follow(&mut self, endpoint: &str, target: &str, online: Option<Instant>) {
        let Some(since) = online else {
            self.unlink(endpoint);
            return;
        };
        if self
            .links
            .get(endpoint)
            .is_some_and(|(host, at)| host == target && *at == since)
        {
            return;
        }
        self.unlink(endpoint);
        self.links
            .insert(endpoint.to_owned(), (target.to_owned(), since));
        if let Ok(mut hosts) = self.offline.lock() {
            hosts.remove(target);
        }
        for slot in self
            .remote
            .get_mut(target)
            .into_iter()
            .flat_map(HashMap::values_mut)
        {
            if let Slot::Unreachable { retry_at } = slot {
                *retry_at = since;
            }
        }
    }

    /// Forget the connections of endpoints that left the catalog, which are
    /// never followed again; `kept` says whether an endpoint is still saved.
    pub(crate) fn keep_links(&mut self, mut kept: impl FnMut(&str) -> bool) {
        let gone: Vec<String> = self
            .links
            .keys()
            .filter(|endpoint| !kept(endpoint))
            .cloned()
            .collect();
        for endpoint in gone {
            self.unlink(&endpoint);
        }
    }

    /// Whether a read on SSH host `target` failed and its retry time has
    /// passed at `now`. A connected host that stays idle sends no snapshot to
    /// ask again with, so the poll loop asks for its checkouts when this holds.
    pub(crate) fn retry_due(&self, target: &str, now: Instant) -> bool {
        self.remote.get(target).is_some_and(|slots| {
            slots
                .values()
                .any(|slot| matches!(slot, Slot::Unreachable { retry_at } if now >= *retry_at))
        })
    }

    /// Forget `target`'s failed reads for directories `listed` no longer
    /// names, such as a closed workspace's, so they never keep the host due.
    /// Only failed reads go: a read that worked is kept for good. Another
    /// session on the same host whose read is forgotten asks again with its
    /// next snapshot.
    pub(crate) fn forget_unlisted(&mut self, target: &str, listed: impl Fn(&str) -> bool) {
        if let Some(slots) = self.remote.get_mut(target) {
            slots.retain(|dir, slot| !matches!(slot, Slot::Unreachable { .. }) || listed(dir));
        }
    }

    /// Whether a pending SSH read's host has connected again since it was
    /// asked for.
    pub(super) fn reconnected_since_asked(&self, checkout: Checkout<&str>) -> bool {
        let Checkout::Ssh { target, .. } = checkout else {
            return false;
        };
        let Some(Slot::Pending { asked }) = self.slot(checkout) else {
            return false;
        };
        self.links
            .values()
            .any(|(host, since)| host == target && since > asked)
    }

    /// Forget an endpoint's connection; its host goes offline once no other
    /// endpoint (another session on the same host) is connected to it.
    fn unlink(&mut self, endpoint: &str) {
        let Some((target, _)) = self.links.remove(endpoint) else {
            return;
        };
        if self.links.values().any(|(host, _)| *host == target) {
            return;
        }
        if let Ok(mut hosts) = self.offline.lock() {
            hosts.insert(target);
        }
    }
}
