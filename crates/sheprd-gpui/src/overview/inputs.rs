//! What decides whether the Overview's rows could have moved, so the window
//! tick rebuilds them only when they could. Building a row formats its text,
//! which at the tick's rate cost more than everything else the tick does.

use crate::state::LiveState;
use sheprd_client::protocol::ClientShellSnapshot;
use std::{
    sync::{Arc, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

/// One host as the rows read it.
#[derive(Debug)]
struct Host {
    enabled: bool,
    connected: bool,
    /// Weak, so an old snapshot is not kept alive for this, while the
    /// allocation stays reserved: a new snapshot cannot reuse its address
    /// and pass for it.
    snapshot: Option<Weak<ClientShellSnapshot>>,
}

impl PartialEq for Host {
    fn eq(&self, other: &Self) -> bool {
        self.enabled == other.enabled
            && self.connected == other.connected
            && match (&self.snapshot, &other.snapshot) {
                (Some(this), Some(other)) => this.ptr_eq(other),
                (this, other) => this.is_none() && other.is_none(),
            }
    }
}

/// The inputs of the last rebuild. Snapshots and marks say exactly when they
/// change. The second stands in for the rest: snoozes run out on whole
/// seconds, the page's elapsed times roll over on a minute, and device
/// labels, pull request answers, and orchestrator runs have no revision to
/// compare, so those reach the rows within a second.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Inputs {
    hosts: Vec<Host>,
    marks: u64,
    mutes: u64,
    closed: u64,
    second: u64,
}

impl Inputs {
    /// `hosts` gives each host's enabled setting and live state.
    pub(super) fn new<'a>(
        hosts: impl IntoIterator<Item = (bool, &'a LiveState)>,
        (marks, mutes, closed): (u64, u64, u64),
        now: SystemTime,
    ) -> Self {
        Self {
            hosts: hosts
                .into_iter()
                .map(|(enabled, live)| Host {
                    enabled,
                    connected: live.status.is_connected(),
                    snapshot: live.snapshot.as_ref().map(Arc::downgrade),
                })
                .collect(),
            marks,
            mutes,
            closed,
            second: now
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs()),
        }
    }
}
